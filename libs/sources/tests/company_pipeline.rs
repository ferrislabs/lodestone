use std::{num::NonZeroU32, sync::Arc, time::Duration};

use common::RobotsPolicy;
use fetch::{FetchSettings, PoliteFetcher, RobotsGate};
use httpmock::{Method::GET, Mock, MockServer};
use lodestone_core::{
    CompanyBuilder, CompanyPage, DossierBuild, DossierState, LinkRejection, NormalizedDomain,
    PAGE_STORE_CHARS, PageVerifier,
};
use sources::{
    company::{CompanyResearch, CompanySettings},
    link_check::BASELINE_PATH_PREFIX,
};
use url::Url;

const SIREN: &str = "123456782";

fn fetcher() -> Arc<PoliteFetcher> {
    Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact: "bot@example.com".to_owned(),
            requests_per_host_per_minute: NonZeroU32::new(6_000).unwrap(),
            request_timeout: Duration::from_secs(5),
            max_body_bytes: 256 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(60),
            source_quotas: Vec::new(),
            allow_private_targets: true,
        })
        .unwrap(),
    )
}

fn research(
    site: &MockServer,
    register: &MockServer,
    max_pages: usize,
) -> CompanyResearch<PoliteFetcher> {
    CompanyResearch::new(
        fetcher(),
        CompanySettings {
            entreprises_url: Url::parse(&register.url("/search")).unwrap().into(),
            max_pages,
            max_rate_limit_wait: Duration::from_secs(1),
            origin: Some(Url::parse(&site.base_url()).unwrap()),
        },
    )
}

fn domain() -> NormalizedDomain {
    NormalizedDomain::from_host("acme.fr").unwrap()
}

fn html(body: &str) -> String {
    format!(
        "<html><body>{body}<footer>Acme, pièces mécaniques de précision à Lyon</footer></body></html>"
    )
}

fn page<'a>(server: &'a MockServer, path: &str, body: &str) -> Mock<'a> {
    let content = html(body);
    let path = path.to_owned();
    server.mock(|when, then| {
        when.method(GET).path(path);
        then.status(200)
            .header("content-type", "text/html; charset=utf-8")
            .body(content);
    })
}

fn register_answer(register: &MockServer) -> Mock<'_> {
    register.mock(|when, then| {
        when.method(GET)
            .path("/search")
            .query_param("q", SIREN)
            .query_param("per_page", "1");
        then.status(200)
            .header("content-type", "application/json")
            .body(format!(
                r#"{{"results":[{{"siren":"{SIREN}","nom_complet":"ACME INDUSTRIE","activite_principale":"62.01Z","section_activite_principale":"J","tranche_effectif_salarie":"12","date_creation":"2010-05-04","siege":{{"code_postal":"69002","libelle_commune":"LYON"}}}}]}}"#
            ));
    })
}

async fn built(builder: &CompanyResearch<PoliteFetcher>) -> DossierBuild {
    builder.build(&domain()).await.unwrap()
}

#[tokio::test]
async fn a_legal_notice_with_a_siret_gives_the_official_data_of_its_siren() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    page(
        &site,
        "/",
        r#"<h1>ACME Industrie</h1><a href="/a-propos">A propos</a><a href="/mentions-legales">Mentions légales</a>"#,
    );
    page(
        &site,
        "/a-propos",
        "<p>Nous fabriquons des pièces depuis 2010.</p>",
    );
    page(
        &site,
        "/mentions-legales",
        "<p>ACME Industrie SAS, SIRET : 123 456 782 00019, RCS Lyon.</p>",
    );
    let lookup = register_answer(&register);
    let build = built(&research(&site, &register, 5)).await;
    assert_eq!(build.state(), DossierState::Complete);
    assert_eq!(build.siren.as_ref().unwrap().as_str(), SIREN);
    let official = build.official.unwrap();
    assert_eq!(official.legal_name, "ACME INDUSTRIE");
    assert_eq!(official.naf_code.as_deref(), Some("62.01Z"));
    assert_eq!(
        official.headcount_range.as_deref(),
        Some("20 à 49 salariés")
    );
    assert_eq!(official.head_office_city.as_deref(), Some("LYON"));
    assert_eq!(build.pages.len(), 3);
    assert!(build.pages[0].text.contains("ACME Industrie"));
    assert!(
        build
            .pages
            .iter()
            .any(|page| page.url.ends_with("/mentions-legales"))
    );
    assert!(build.failure.is_none());
    lookup.assert_hits(1);
}

#[tokio::test]
async fn a_site_without_a_siren_is_partial_and_calls_no_register() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    page(&site, "/", r#"<a href="/mentions-legales">Mentions</a>"#);
    page(
        &site,
        "/mentions-legales",
        "<p>Editeur : ACME. Téléphone 123 456 782.</p>",
    );
    let lookup = register_answer(&register);
    let build = built(&research(&site, &register, 5)).await;
    assert_eq!(build.state(), DossierState::Partial);
    assert!(build.siren.is_none());
    assert!(build.official.is_none());
    assert_eq!(build.pages.len(), 2);
    assert!(build.failure.is_none());
    lookup.assert_hits(0);
}

#[tokio::test]
async fn a_siren_that_fails_the_check_digit_is_not_looked_up() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    page(&site, "/", r#"<a href="/mentions-legales">Mentions</a>"#);
    page(&site, "/mentions-legales", "<p>SIRET 123 456 789 00019</p>");
    let lookup = register_answer(&register);
    let build = built(&research(&site, &register, 5)).await;
    assert_eq!(build.state(), DossierState::Partial);
    lookup.assert_hits(0);
}

#[tokio::test]
async fn a_register_failure_keeps_the_siren_and_marks_the_build_for_retry() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    page(&site, "/", r#"<a href="/mentions-legales">Mentions</a>"#);
    page(&site, "/mentions-legales", "<p>SIREN 123 456 782</p>");
    let lookup = register.mock(|when, then| {
        when.method(GET).path("/search");
        then.status(503);
    });
    let build = built(&research(&site, &register, 5)).await;
    assert_eq!(build.state(), DossierState::Partial);
    assert_eq!(build.siren.as_ref().unwrap().as_str(), SIREN);
    assert!(build.failure.as_deref().unwrap().contains("503"));
    lookup.assert_hits(1);
}

#[tokio::test]
async fn a_robots_txt_that_disallows_the_site_makes_it_unreachable_without_fetching_a_page() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    site.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200).body("User-agent: *\nDisallow: /\n");
    });
    let home = page(&site, "/", "<p>Hello</p>");
    let legal = page(&site, "/mentions-legales", "<p>SIRET 123 456 782 00019</p>");
    let lookup = register_answer(&register);
    let build = built(&research(&site, &register, 5)).await;
    assert_eq!(build.state(), DossierState::Unreachable);
    assert!(!build.reachable);
    assert!(build.pages.is_empty());
    assert!(build.failure.as_deref().unwrap().contains("robots"));
    home.assert_hits(0);
    legal.assert_hits(0);
    lookup.assert_hits(0);
}

#[tokio::test]
async fn an_unreachable_home_page_makes_the_site_unreachable() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    site.mock(|when, then| {
        when.method(GET).path("/");
        then.status(503);
    });
    let build = built(&research(&site, &register, 5)).await;
    assert_eq!(build.state(), DossierState::Unreachable);
    assert!(build.failure.is_some());
}

#[tokio::test]
async fn the_page_budget_counts_the_home_page_and_is_respected() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    let links: String = [
        "mentions-legales",
        "cgu",
        "a-propos",
        "qui-sommes-nous",
        "about",
        "societe",
    ]
    .iter()
    .map(|path| format!(r#"<a href="/{path}">{path}</a>"#))
    .collect();
    let home = page(&site, "/", &links);
    let others: Vec<_> = [
        "mentions-legales",
        "cgu",
        "a-propos",
        "qui-sommes-nous",
        "about",
        "societe",
    ]
    .iter()
    .map(|path| page(&site, &format!("/{path}"), "<p>texte</p>"))
    .collect();
    let build = built(&research(&site, &register, 3)).await;
    assert_eq!(build.pages.len(), 3);
    home.assert_hits(1);
    let fetched: usize = others.iter().map(|mock| mock.hits()).sum();
    assert_eq!(fetched, 2);
    others[0].assert_hits(1);
    others[1].assert_hits(1);
}

#[tokio::test]
async fn only_pages_of_the_same_site_are_fetched() {
    let site = MockServer::start_async().await;
    let other = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    let foreign = page(
        &other,
        "/mentions-legales",
        "<p>SIRET 123 456 782 00019</p>",
    );
    page(
        &site,
        "/",
        &format!(
            r#"<a href="{}">Mentions</a><a href="/a-propos">A propos</a>"#,
            other.url("/mentions-legales")
        ),
    );
    page(&site, "/a-propos", "<p>Nous</p>");
    let lookup = register_answer(&register);
    let build = built(&research(&site, &register, 5)).await;
    foreign.assert_hits(0);
    lookup.assert_hits(0);
    assert!(build.siren.is_none());
    assert!(
        build
            .pages
            .iter()
            .all(|page| page.url.starts_with(&site.base_url()))
    );
}

#[tokio::test]
async fn a_siren_past_the_stored_text_budget_is_still_found() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    page(&site, "/", r#"<a href="/mentions-legales">Mentions</a>"#);
    let menu = "<p>Menu de navigation</p>".repeat(1_000);
    page(
        &site,
        "/mentions-legales",
        &format!("{menu}<p>ACME SAS, RCS Lyon 123 456 782</p>"),
    );
    let lookup = register_answer(&register);
    let build = built(&research(&site, &register, 5)).await;
    assert_eq!(build.state(), DossierState::Complete);
    assert_eq!(build.siren.as_ref().unwrap().as_str(), SIREN);
    let legal = build
        .pages
        .iter()
        .find(|page| page.url.ends_with("/mentions-legales"))
        .unwrap();
    assert_eq!(legal.text.chars().count(), PAGE_STORE_CHARS);
    assert!(!legal.text.contains("123 456 782"));
    lookup.assert_hits(1);
}

const NOTION_MISSING: &str = "<html><head><title>Notion</title></head><body><h1>Cette page est introuvable</h1><p>Retournez à l'accueil de cet espace de travail.</p></body></html>";
const PARKED_MISSING: &str = "<html><head><title>Jawagg</title></head><body><h1>Oups</h1><p>Le contenu demandé a peut-être été déplacé, retournez à l'accueil pour continuer votre visite.</p></body></html>";
const GUESSED: [&str; 4] = ["/mentions-legales", "/legal", "/a-propos", "/about"];

fn raw<'a>(server: &'a MockServer, path: &str, body: &str) -> Mock<'a> {
    let path = path.to_owned();
    let body = body.to_owned();
    server.mock(|when, then| {
        when.method(GET).path(path);
        then.status(200)
            .header("content-type", "text/html; charset=utf-8")
            .body(body);
    })
}

fn missing_everywhere<'a>(server: &'a MockServer, body: &str) -> Mock<'a> {
    let body = body.to_owned();
    server.mock(|when, then| {
        when.method(GET).path_contains(BASELINE_PATH_PREFIX);
        then.status(200)
            .header("content-type", "text/html; charset=utf-8")
            .body(body);
    })
}

fn urls(build: &DossierBuild) -> Vec<String> {
    build.pages.iter().map(|page| page.url.clone()).collect()
}

#[tokio::test]
async fn a_site_answering_200_with_a_not_found_page_keeps_only_its_home_page() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    page(
        &site,
        "/",
        "<h1>Jawagg</h1><p>Le studio qui conçoit vos jeux.</p>",
    );
    for path in GUESSED {
        raw(&site, path, NOTION_MISSING);
    }
    let baseline = missing_everywhere(&site, NOTION_MISSING);
    let build = built(&research(&site, &register, 5)).await;
    assert_eq!(build.state(), DossierState::Partial);
    assert_eq!(urls(&build), [site.url("/")]);
    assert_eq!(build.unconfirmed.soft_404_marker, 4);
    assert_eq!(build.unconfirmed.total(), 4);
    baseline.assert_hits(0);
}

#[tokio::test]
async fn pages_identical_to_the_missing_page_of_the_site_are_dropped_and_real_ones_kept() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    page(
        &site,
        "/",
        r#"<h1>Jawagg</h1><a href="/qui-sommes-nous">Qui sommes-nous</a>"#,
    );
    for path in ["/mentions-legales", "/legal"] {
        raw(&site, path, PARKED_MISSING);
    }
    page(
        &site,
        "/qui-sommes-nous",
        "<p>Une équipe de douze personnes installée à Nantes depuis 2015.</p>",
    );
    let baseline = missing_everywhere(&site, PARKED_MISSING);
    let build = built(&research(&site, &register, 5)).await;
    assert_eq!(urls(&build), [site.url("/"), site.url("/qui-sommes-nous")]);
    assert_eq!(build.unconfirmed.soft_404_baseline, 2);
    baseline.assert_hits(1);
}

#[tokio::test]
async fn guessed_paths_redirected_home_missing_failing_or_empty_are_not_kept() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    let home = page(&site, "/", "<h1>Acme</h1>");
    site.mock(|when, then| {
        when.method(GET).path("/mentions-legales");
        then.status(301).header("location", "/");
    });
    site.mock(|when, then| {
        when.method(GET).path("/legal");
        then.status(500);
    });
    raw(
        &site,
        "/about",
        "<html><body><div id=\"app\"></div></body></html>",
    );
    let build = built(&research(&site, &register, 5)).await;
    assert_eq!(urls(&build), [site.url("/")]);
    assert_eq!(build.unconfirmed.redirected_home, 1);
    assert_eq!(build.unconfirmed.status, 2);
    assert_eq!(build.unconfirmed.empty_content, 1);
    home.assert_hits(2);
}

#[tokio::test]
async fn a_page_reached_twice_through_redirects_is_kept_once() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    page(&site, "/", "<h1>Acme</h1>");
    page(
        &site,
        "/mentions-legales",
        "<p>Editeur : Acme SAS, Lyon.</p>",
    );
    site.mock(|when, then| {
        when.method(GET).path("/legal");
        then.status(301).header("location", "/mentions-legales");
    });
    let build = built(&research(&site, &register, 5)).await;
    assert_eq!(urls(&build), [site.url("/"), site.url("/mentions-legales")]);
}

#[tokio::test]
async fn a_javascript_only_home_page_makes_the_site_unusable() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    raw(
        &site,
        "/",
        r#"<html><head><title>Acme</title><script src="/app.js"></script></head><body><div id="root"></div></body></html>"#,
    );
    let build = built(&research(&site, &register, 5)).await;
    assert_eq!(build.state(), DossierState::Unreachable);
    assert!(build.failure.as_deref().unwrap().contains("empty_content"));
    assert_eq!(build.unconfirmed.empty_content, 1);
}

#[tokio::test]
async fn the_review_of_stored_pages_rejects_only_the_unconfirmed_ones() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    page(&site, "/", "<h1>Jawagg</h1>");
    raw(&site, "/mentions-legales", NOTION_MISSING);
    page(
        &site,
        "/a-propos",
        "<p>Une équipe de douze personnes à Nantes.</p>",
    );
    site.mock(|when, then| {
        when.method(GET).path("/legal");
        then.status(404);
    });
    let stored: Vec<CompanyPage> = ["/", "/mentions-legales", "/legal", "/a-propos"]
        .iter()
        .map(|path| CompanyPage {
            url: site.url(*path),
            text: "texte".to_owned(),
        })
        .collect();
    let review = research(&site, &register, 5).review(&stored).await.unwrap();
    assert_eq!(
        review.rejected,
        [
            (site.url("/mentions-legales"), LinkRejection::Soft404Marker),
            (site.url("/legal"), LinkRejection::Status),
        ]
    );
}

#[tokio::test]
async fn with_the_ignore_policy_a_site_refused_by_robots_txt_is_read() {
    let site = MockServer::start_async().await;
    let register = MockServer::start_async().await;
    let robots = site.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200).body("User-agent: *\nDisallow: /\n");
    });
    let home = page(
        &site,
        "/",
        "<a href=\"/mentions-legales\">Mentions légales</a>",
    );
    page(&site, "/mentions-legales", "<p>SIRET 123 456 782 00019</p>");
    register_answer(&register);
    let fetcher =
        PoliteFetcher::clone(&fetcher()).with_robots_gate(RobotsGate::new(RobotsPolicy::Ignore));
    let research = CompanyResearch::new(
        Arc::new(fetcher),
        CompanySettings {
            entreprises_url: Url::parse(&register.url("/search")).unwrap().into(),
            max_pages: 5,
            max_rate_limit_wait: Duration::from_secs(1),
            origin: Some(Url::parse(&site.base_url()).unwrap()),
        },
    );
    let build = built(&research).await;
    assert!(build.reachable);
    assert!(!build.pages.is_empty());
    assert!(
        build
            .failure
            .as_deref()
            .is_none_or(|failure| !failure.contains("robots"))
    );
    home.assert_hits(1);
    robots.assert_hits(0);
}
