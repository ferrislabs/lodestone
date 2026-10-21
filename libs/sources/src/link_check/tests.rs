use std::{num::NonZeroU32, time::Duration};

use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{Method::GET, Mock, MockServer};

use super::*;

const GENUINE: &str = "<html><head><title>Mentions légales - Acme</title></head><body><h1>Mentions légales</h1><p>Acme SAS, société par actions simplifiée au capital de 10 000 euros, siège à Lyon.</p></body></html>";
const ABOUT: &str = "<html><head><title>À propos - Acme</title></head><body><h1>À propos</h1><p>Nous fabriquons des pièces mécaniques de précision depuis 2010 pour l'industrie.</p></body></html>";
const SOFT_404: &str = "<html><head><title>Acme</title></head><body><h1>Oups</h1><p>Le contenu demandé a peut-être été déplacé, retournez à l'accueil pour continuer votre visite.</p></body></html>";
const MARKER: &str = "<html><head><title>Cette page est introuvable</title></head><body><p>Retourner sur la page d'accueil de ce site Notion.</p></body></html>";
const JS_SHELL: &str = r#"<html><head><title>Acme App</title><script src="/app.js"></script></head><body><div id="root"></div><noscript>You need to enable JavaScript to run this app.</noscript></body></html>"#;

fn fetcher() -> PoliteFetcher {
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
    .unwrap()
}

fn serve<'a>(server: &'a MockServer, path: &str, status: u16, body: &str) -> Mock<'a> {
    let path = path.to_owned();
    let body = body.to_owned();
    server.mock(|when, then| {
        when.method(GET).path(path);
        then.status(status)
            .header("content-type", "text/html; charset=utf-8")
            .body(body);
    })
}

fn soft_404_everywhere(server: &MockServer) -> Mock<'_> {
    server.mock(|when, then| {
        when.method(GET).path_contains(BASELINE_PATH_PREFIX);
        then.status(200)
            .header("content-type", "text/html; charset=utf-8")
            .body(SOFT_404);
    })
}

async fn check(
    server: &MockServer,
    path: &str,
    origin: LinkOrigin,
) -> (Result<LinkVerdict, FetchError>, UnconfirmedLinks) {
    let fetcher = fetcher();
    let mut checker = LinkChecker::new(&fetcher, SourceName::PublicPage, Duration::from_secs(1));
    let url = Url::parse(&server.url(path)).unwrap();
    let request = FetchRequest {
        url: url.clone(),
        purpose: FetchPurpose::Crawl,
        source: SourceName::PublicPage,
    };
    let verdict = match fetcher.fetch(&request).await {
        Ok(response) => checker.confirm(&url, origin, &response).await,
        Err(error) => {
            if let Some(reason) = rejection_of(&error) {
                checker.reject(reason);
                Ok(LinkVerdict::NotFound(reason))
            } else {
                Err(error)
            }
        }
    };
    (verdict, checker.unconfirmed())
}

fn rejected(verdict: &Result<LinkVerdict, FetchError>) -> Option<LinkRejection> {
    match verdict {
        Ok(LinkVerdict::NotFound(reason)) => Some(*reason),
        _ => None,
    }
}

#[tokio::test]
async fn a_404_is_not_found_by_status() {
    let server = MockServer::start_async().await;
    serve(&server, "/legal", 404, GENUINE);
    let (verdict, counts) = check(&server, "/legal", LinkOrigin::Guessed).await;
    assert_eq!(rejected(&verdict), Some(LinkRejection::Status));
    assert_eq!(counts.status, 1);
}

#[tokio::test]
async fn a_500_is_not_found_by_status() {
    let server = MockServer::start_async().await;
    serve(&server, "/about", 500, ABOUT);
    let (verdict, _) = check(&server, "/about", LinkOrigin::Linked).await;
    assert_eq!(rejected(&verdict), Some(LinkRejection::Status));
}

#[tokio::test]
async fn a_guessed_path_redirected_to_the_home_page_is_not_found() {
    let server = MockServer::start_async().await;
    server.mock(|when, then| {
        when.method(GET).path("/mentions-legales");
        then.status(302).header("location", "/");
    });
    serve(&server, "/", 200, ABOUT);
    let (verdict, counts) = check(&server, "/mentions-legales", LinkOrigin::Guessed).await;
    assert_eq!(rejected(&verdict), Some(LinkRejection::RedirectedHome));
    assert_eq!(counts.redirected_home, 1);
}

#[tokio::test]
async fn a_redirect_to_another_host_is_not_found() {
    let server = MockServer::start_async().await;
    let other = MockServer::start_async().await;
    serve(&other, "/a-propos", 200, ABOUT);
    let target = other.url("/a-propos").replace("127.0.0.1", "localhost");
    server.mock(|when, then| {
        when.method(GET).path("/a-propos");
        then.status(301).header("location", target);
    });
    let (verdict, _) = check(&server, "/a-propos", LinkOrigin::Guessed).await;
    assert_eq!(rejected(&verdict), Some(LinkRejection::RedirectedHome));
}

#[test]
fn a_guessed_page_answered_by_another_host_is_redirected_home_but_www_is_the_same_host() {
    let requested = Url::parse("https://acme.fr/a-propos").unwrap();
    let response = |final_url: &str| FetchResponse {
        status: 200,
        headers: vec![("content-type".to_owned(), "text/html".to_owned())],
        body: ABOUT.as_bytes().to_vec(),
        final_url: Url::parse(final_url).unwrap(),
        truncated: false,
    };
    let guessed = LinkCheck {
        requested: &requested,
        origin: LinkOrigin::Guessed,
    };
    assert_eq!(
        guessed.verdict(&response("https://shop.acme.fr/a-propos"), None),
        LinkVerdict::NotFound(LinkRejection::RedirectedHome)
    );
    assert!(matches!(
        guessed.verdict(&response("https://www.acme.fr/a-propos"), None),
        LinkVerdict::Confirmed(_)
    ));
    let linked = LinkCheck {
        requested: &requested,
        origin: LinkOrigin::Linked,
    };
    assert!(matches!(
        linked.verdict(&response("https://shop.acme.fr/a-propos"), None),
        LinkVerdict::Confirmed(_)
    ));
}

#[tokio::test]
async fn a_page_like_the_missing_page_of_its_site_is_a_soft_404() {
    let server = MockServer::start_async().await;
    let baseline = soft_404_everywhere(&server);
    serve(&server, "/legal", 200, SOFT_404);
    let (verdict, counts) = check(&server, "/legal", LinkOrigin::Guessed).await;
    assert_eq!(rejected(&verdict), Some(LinkRejection::Soft404Baseline));
    assert_eq!(counts.soft_404_baseline, 1);
    baseline.assert_hits(1);
}

#[tokio::test]
async fn a_short_page_with_a_not_found_marker_is_a_soft_404() {
    let server = MockServer::start_async().await;
    let baseline = soft_404_everywhere(&server);
    serve(&server, "/a-propos", 200, MARKER);
    let (verdict, counts) = check(&server, "/a-propos", LinkOrigin::Guessed).await;
    assert_eq!(rejected(&verdict), Some(LinkRejection::Soft404Marker));
    assert_eq!(counts.soft_404_marker, 1);
    baseline.assert_hits(0);
}

#[tokio::test]
async fn a_javascript_only_shell_has_no_usable_content() {
    let server = MockServer::start_async().await;
    serve(&server, "/about", 200, JS_SHELL);
    let (verdict, counts) = check(&server, "/about", LinkOrigin::Linked).await;
    assert_eq!(rejected(&verdict), Some(LinkRejection::EmptyContent));
    assert_eq!(counts.empty_content, 1);
}

#[tokio::test]
async fn a_genuine_page_is_confirmed_against_the_missing_page_of_its_site() {
    let server = MockServer::start_async().await;
    let baseline = soft_404_everywhere(&server);
    serve(&server, "/mentions-legales", 200, GENUINE);
    let (verdict, counts) = check(&server, "/mentions-legales", LinkOrigin::Guessed).await;
    match verdict {
        Ok(LinkVerdict::Confirmed(page)) => {
            assert_eq!(page.title, "Mentions légales - Acme");
            assert!(page.text.contains("capital de 10 000 euros"));
        }
        other => panic!("expected a confirmed page, got {other:?}"),
    }
    assert_eq!(counts.total(), 0);
    baseline.assert_hits(1);
}

#[tokio::test]
async fn the_missing_page_is_fetched_once_per_host_and_never_for_the_home_page() {
    let server = MockServer::start_async().await;
    let baseline = soft_404_everywhere(&server);
    serve(&server, "/", 200, ABOUT);
    serve(&server, "/mentions-legales", 200, GENUINE);
    serve(&server, "/legal", 200, SOFT_404);
    let fetcher = fetcher();
    let mut checker = LinkChecker::new(&fetcher, SourceName::PublicPage, Duration::from_secs(1));
    let mut verdicts = Vec::new();
    for (path, origin) in [
        ("/", LinkOrigin::Home),
        ("/mentions-legales", LinkOrigin::Linked),
        ("/legal", LinkOrigin::Guessed),
    ] {
        let url = Url::parse(&server.url(path)).unwrap();
        let response = fetcher
            .fetch(&FetchRequest {
                url: url.clone(),
                purpose: FetchPurpose::Crawl,
                source: SourceName::PublicPage,
            })
            .await
            .unwrap();
        verdicts.push(checker.confirm(&url, origin, &response).await.unwrap());
    }
    assert!(matches!(verdicts[0], LinkVerdict::Confirmed(_)));
    assert!(matches!(verdicts[1], LinkVerdict::Confirmed(_)));
    assert_eq!(
        verdicts[2],
        LinkVerdict::NotFound(LinkRejection::Soft404Baseline)
    );
    baseline.assert_hits(1);
    assert_eq!(checker.unconfirmed().total(), 1);
}

#[tokio::test]
async fn a_site_whose_missing_pages_answer_404_has_no_baseline() {
    let server = MockServer::start_async().await;
    serve(&server, "/a-propos", 200, ABOUT);
    let (verdict, _) = check(&server, "/a-propos", LinkOrigin::Guessed).await;
    assert!(matches!(verdict, Ok(LinkVerdict::Confirmed(_))));
}

#[test]
fn similarity_ignores_the_words_of_each_path() {
    let left = Url::parse("https://acme.fr/a-propos").unwrap();
    let right = Url::parse("https://acme.fr/lodestone-missing-page-1234").unwrap();
    let score = similarity(
        "La page a propos est vide ici",
        "La page lodestone missing page 1234 est vide ici",
        &left,
        &right,
    );
    assert!(score >= SIMILARITY_THRESHOLD, "{score}");
    assert_eq!(similarity("", "texte", &left, &right), 0.0);
}

#[test]
fn markers_cover_french_and_english_wordings() {
    for body in [
        "<title>Page non trouvée</title><p>Cette adresse n’existe pas sur notre site web.</p>",
        "<title>Oops</title><p>Sorry, this page doesn't exist anymore on this website.</p>",
        "<title>Erreur 404</title><p>Retour à l'accueil de notre beau site internet.</p>",
        "<title>Not Found</title><p>The requested URL was not found on this server.</p>",
    ] {
        assert!(PageSnapshot::of(body).has_marker(), "{body}");
    }
    assert!(!PageSnapshot::of(GENUINE).has_marker());
}

#[test]
fn a_long_page_mentioning_not_found_is_not_a_marker_page() {
    let requested = Url::parse("https://acme.fr/faq").unwrap();
    let long = format!(
        "<title>FAQ</title><p>Que faire si un colis est not found ?</p>{}",
        "<p>Une réponse détaillée et utile pour nos clients.</p>".repeat(60)
    );
    let response = FetchResponse {
        status: 200,
        headers: Vec::new(),
        body: long.into_bytes(),
        final_url: requested.clone(),
        truncated: false,
    };
    let check = LinkCheck {
        requested: &requested,
        origin: LinkOrigin::Linked,
    };
    assert!(matches!(
        check.verdict(&response, None),
        LinkVerdict::Confirmed(_)
    ));
}
