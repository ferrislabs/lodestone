use std::{num::NonZeroU32, sync::Arc, time::Duration};

use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{Method::GET, MockServer};
use lodestone_core::{CompanyBuilder, DossierBuild, DossierState, NormalizedDomain};
use sources::company::{CompanyResearch, CompanySettings};
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
    format!("<html><body>{body}</body></html>")
}

fn page<'a>(server: &'a MockServer, path: &str, body: &str) -> httpmock::Mock<'a> {
    let content = html(body);
    let path = path.to_owned();
    server.mock(|when, then| {
        when.method(GET).path(path);
        then.status(200)
            .header("content-type", "text/html; charset=utf-8")
            .body(content);
    })
}

fn register_answer(register: &MockServer) -> httpmock::Mock<'_> {
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
    assert_eq!(legal.text.chars().count(), lodestone_core::PAGE_STORE_CHARS);
    assert!(!legal.text.contains("123 456 782"));
    lookup.assert_hits(1);
}
