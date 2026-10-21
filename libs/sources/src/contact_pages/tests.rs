use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
    time::Duration,
};

use chrono::TimeZone;
use lodestone_core::FetchResponse;

use super::*;
use crate::link_check::BASELINE_PATH_PREFIX;

type Reply = Result<(u16, Option<&'static str>, String), FetchError>;

struct FakeFetcher {
    script: HashMap<String, Reply>,
    final_urls: HashMap<String, String>,
    missing: Option<String>,
    limited_once: Mutex<HashSet<String>>,
    requests: Mutex<Vec<(String, FetchPurpose, SourceName)>>,
}

impl FakeFetcher {
    fn new(script: Vec<(&str, Reply)>) -> Self {
        Self {
            script: script
                .into_iter()
                .map(|(url, reply)| (url.to_owned(), reply))
                .collect(),
            final_urls: HashMap::new(),
            missing: None,
            limited_once: Mutex::new(HashSet::new()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn urls(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|(url, _, _)| url.clone())
            .filter(|url| !url.contains(BASELINE_PATH_PREFIX))
            .collect()
    }

    fn probes(&self) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(url, _, _)| url.contains(BASELINE_PATH_PREFIX))
            .count()
    }
}

impl Fetcher for FakeFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let key = request.url.to_string();
        self.requests
            .lock()
            .unwrap()
            .push((key.clone(), request.purpose, request.source));
        if self.limited_once.lock().unwrap().remove(&key) {
            return Err(FetchError::RateLimited {
                retry_after: Duration::from_secs(20),
            });
        }
        if let Some(body) = &self.missing
            && key.contains(BASELINE_PATH_PREFIX)
        {
            return Ok(FetchResponse {
                status: 200,
                headers: vec![("content-type".to_owned(), "text/html".to_owned())],
                body: body.clone().into_bytes(),
                final_url: request.url.clone(),
                truncated: false,
            });
        }
        let final_url = self.final_urls.get(&key).unwrap_or(&key);
        match self.script.get(&key) {
            Some(Ok((status, content_type, body))) => Ok(FetchResponse {
                status: *status,
                headers: content_type
                    .iter()
                    .map(|value| ("Content-Type".to_owned(), (*value).to_owned()))
                    .collect(),
                body: body.clone().into_bytes(),
                final_url: Url::parse(final_url).unwrap(),
                truncated: false,
            }),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(FetchResponse {
                status: 404,
                headers: Vec::new(),
                body: Vec::new(),
                final_url: request.url.clone(),
                truncated: false,
            }),
        }
    }
}

const FRAME: &str = "<h1>Contactez la société Acme</h1>";

fn html(body: &str) -> Reply {
    Ok((
        200,
        Some("text/html; charset=utf-8"),
        format!("{FRAME}{body}"),
    ))
}

fn domain() -> NormalizedDomain {
    NormalizedDomain::from_host("acme.fr").unwrap()
}

fn now() -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000, 0).unwrap()
}

fn settings(paths: &[&str], max_pages: usize) -> ContactPageSettings {
    ContactPageSettings {
        paths: paths.iter().map(|path| (*path).to_owned()).collect(),
        max_pages,
        max_rate_limit_wait: Duration::ZERO,
    }
}

fn finder(
    fetcher: FakeFetcher,
    paths: &[&str],
    max_pages: usize,
) -> (PublicPageFinder<FakeFetcher>, Arc<FakeFetcher>) {
    let fetcher = Arc::new(fetcher);
    (
        PublicPageFinder::new(Arc::clone(&fetcher), settings(paths, max_pages)),
        fetcher,
    )
}

#[tokio::test]
async fn name_is_public_page() {
    let (finder, _) = finder(FakeFetcher::new(vec![]), &[], 1);
    assert_eq!(finder.name(), SourceName::PublicPage);
}

#[tokio::test]
async fn paths_are_fetched_in_order_with_crawl_purpose() {
    let (finder, fetcher) = finder(FakeFetcher::new(vec![]), &["/", "/contact", "about"], 10);
    finder.find(&domain(), &[], now()).await.unwrap();
    assert_eq!(
        fetcher.urls(),
        [
            "https://acme.fr/",
            "https://acme.fr/contact",
            "https://acme.fr/about"
        ]
    );
    assert!(
        fetcher
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|(_, purpose, source)| *purpose == FetchPurpose::Crawl
                && *source == SourceName::PublicPage)
    );
}

#[tokio::test]
async fn fetching_is_capped_at_max_pages() {
    let (finder, fetcher) = finder(FakeFetcher::new(vec![]), &["/a", "/b", "/c"], 2);
    finder.find(&domain(), &[], now()).await.unwrap();
    assert_eq!(fetcher.urls(), ["https://acme.fr/a", "https://acme.fr/b"]);
}

#[tokio::test]
async fn failing_pages_are_skipped() {
    let script = vec![
        ("https://acme.fr/a", Err(FetchError::DisallowedByRobots)),
        (
            "https://acme.fr/b",
            Ok((404, Some("text/html"), "x@acme.fr".to_owned())),
        ),
        ("https://acme.fr/c", Err(FetchError::Timeout)),
        (
            "https://acme.fr/d",
            Err(FetchError::Network("boom".to_owned())),
        ),
        ("https://acme.fr/e", Err(FetchError::Redirect)),
        ("https://acme.fr/f", Err(FetchError::NonPublicTarget)),
        ("https://acme.fr/g", Ok((500, None, "y@acme.fr".to_owned()))),
        ("https://acme.fr/h", html("<p>ok@acme.fr</p>")),
    ];
    let (finder, fetcher) = finder(
        FakeFetcher::new(script),
        &["/a", "/b", "/c", "/d", "/e", "/f", "/g", "/h"],
        10,
    );
    let found = finder.find(&domain(), &[], now()).await.unwrap();
    assert_eq!(fetcher.urls().len(), 8);
    assert_eq!(fetcher.probes(), 1);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].address.as_str(), "ok@acme.fr");
}

#[tokio::test]
async fn rate_limit_stops_and_fails() {
    let script = vec![
        ("https://acme.fr/a", html("<p>a@acme.fr</p>")),
        (
            "https://acme.fr/b",
            Err(FetchError::RateLimited {
                retry_after: Duration::from_secs(30),
            }),
        ),
    ];
    let (finder, fetcher) = finder(FakeFetcher::new(script), &["/a", "/b", "/c"], 10);
    let error = finder.find(&domain(), &[], now()).await.unwrap_err();
    assert_eq!(
        error,
        SourceError::RateLimited {
            retry_after: Duration::from_secs(30)
        }
    );
    assert_eq!(fetcher.urls().len(), 2);
}

#[tokio::test]
async fn non_text_content_is_ignored() {
    let script = vec![
        (
            "https://acme.fr/a",
            Ok((200, Some("application/pdf"), "a@acme.fr".to_owned())),
        ),
        (
            "https://acme.fr/b",
            Ok((200, Some("text/plain"), format!("{FRAME} b@acme.fr"))),
        ),
        (
            "https://acme.fr/c",
            Ok((200, None, format!("{FRAME} c@acme.fr"))),
        ),
        (
            "https://acme.fr/d",
            Ok((200, Some("image/png"), "d@acme.fr".to_owned())),
        ),
    ];
    let (finder, _) = finder(FakeFetcher::new(script), &["/a", "/b", "/c", "/d"], 10);
    let found = finder.find(&domain(), &[], now()).await.unwrap();
    let addresses: Vec<&str> = found.iter().map(|f| f.address.as_str()).collect();
    assert_eq!(addresses, ["b@acme.fr", "c@acme.fr"]);
}

#[tokio::test]
async fn provenance_is_the_final_url_and_method_is_public_page() {
    let mut fetcher = FakeFetcher::new(vec![("https://acme.fr/contact", html("<p>a@acme.fr</p>"))]);
    fetcher.final_urls.insert(
        "https://acme.fr/contact".to_owned(),
        "https://www.acme.fr/nous-contacter".to_owned(),
    );
    let (finder, _) = finder(fetcher, &["/contact"], 10);
    let found = finder.find(&domain(), &[], now()).await.unwrap();
    assert_eq!(
        found[0].provenance.as_str(),
        "https://www.acme.fr/nous-contacter"
    );
    assert_eq!(found[0].method, ContactMethod::PublicPage);
}

#[tokio::test]
async fn duplicates_keep_first_provenance_and_highest_confidence() {
    let script = vec![
        ("https://acme.fr/a", html("a [at] acme [dot] fr")),
        (
            "https://acme.fr/b",
            html(r#"<a href="mailto:a@acme.fr">a</a> <p>z@acme.fr</p>"#),
        ),
    ];
    let (finder, _) = finder(FakeFetcher::new(script), &["/a", "/b"], 10);
    let found = finder.find(&domain(), &[], now()).await.unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].address.as_str(), "a@acme.fr");
    assert_eq!(found[0].provenance.as_str(), "https://acme.fr/a");
    assert_eq!(found[0].confidence.get(), 0.9);
    assert_eq!(found[1].provenance.as_str(), "https://acme.fr/b");
}

#[tokio::test(start_paused = true)]
async fn short_rate_limit_waits_then_retries_the_same_page() {
    let script = vec![
        ("https://acme.fr/a", html("<p>a@acme.fr</p>")),
        ("https://acme.fr/b", html("<p>b@acme.fr</p>")),
    ];
    let fetcher = FakeFetcher::new(script);
    fetcher
        .limited_once
        .lock()
        .unwrap()
        .insert("https://acme.fr/b".to_owned());
    let fetcher = Arc::new(fetcher);
    let finder = PublicPageFinder::new(
        Arc::clone(&fetcher),
        ContactPageSettings {
            max_rate_limit_wait: Duration::from_secs(60),
            ..settings(&["/a", "/b"], 10)
        },
    );
    let started = tokio::time::Instant::now();
    let found = finder.find(&domain(), &[], now()).await.unwrap();
    assert_eq!(found.len(), 2);
    assert!(started.elapsed() >= Duration::from_secs(20));
    assert_eq!(
        fetcher.urls(),
        [
            "https://acme.fr/a",
            "https://acme.fr/b",
            "https://acme.fr/b"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn long_rate_limit_is_not_waited_for() {
    let script = vec![(
        "https://acme.fr/a",
        Err(FetchError::RateLimited {
            retry_after: Duration::from_secs(600),
        }),
    )];
    let fetcher = Arc::new(FakeFetcher::new(script));
    let finder = PublicPageFinder::new(
        Arc::clone(&fetcher),
        ContactPageSettings {
            max_rate_limit_wait: Duration::from_secs(60),
            ..settings(&["/a"], 10)
        },
    );
    assert!(matches!(
        finder.find(&domain(), &[], now()).await,
        Err(SourceError::RateLimited { .. })
    ));
    assert_eq!(fetcher.urls().len(), 1);
}

const PARKED: &str = "<h1>Oups</h1><p>Le contenu demandé a peut-être été déplacé, écrivez à oops@acme.fr pour continuer.</p>";

#[tokio::test]
async fn addresses_of_unconfirmed_contact_pages_are_not_kept() {
    let script = vec![
        ("https://acme.fr/", html("<p>Bienvenue chez Acme.</p>")),
        (
            "https://acme.fr/contact",
            Ok((
                200,
                Some("text/html"),
                "<title>Page introuvable</title><p>Écrivez à lost@acme.fr</p>".to_owned(),
            )),
        ),
        (
            "https://acme.fr/mentions-legales",
            Ok((200, Some("text/html"), PARKED.to_owned())),
        ),
        (
            "https://acme.fr/a-propos",
            Ok((
                200,
                Some("text/html"),
                r#"<div id="root"></div><noscript>shell@acme.fr</noscript>"#.to_owned(),
            )),
        ),
        (
            "https://acme.fr/equipe",
            html("<p>Notre équipe : team@acme.fr</p>"),
        ),
        (
            "https://acme.fr/nous-ecrire",
            html("<p>Ecrire à home@acme.fr</p>"),
        ),
        (
            "https://acme.fr/presse",
            html("<p>Presse : press@acme.fr</p>"),
        ),
    ];
    let mut fetcher = FakeFetcher::new(script);
    fetcher.missing = Some(PARKED.to_owned());
    fetcher.final_urls.insert(
        "https://acme.fr/nous-ecrire".to_owned(),
        "https://acme.fr/".to_owned(),
    );
    fetcher.final_urls.insert(
        "https://acme.fr/presse".to_owned(),
        "https://press.acme.fr/presse".to_owned(),
    );
    let (finder, fetcher) = finder(
        fetcher,
        &[
            "/",
            "/contact",
            "/mentions-legales",
            "/a-propos",
            "/equipe",
            "/nous-ecrire",
            "/presse",
        ],
        10,
    );
    let found = finder.find(&domain(), &[], now()).await.unwrap();
    let addresses: Vec<(&str, &str)> = found
        .iter()
        .map(|item| (item.address.as_str(), item.provenance.as_str()))
        .collect();
    assert_eq!(addresses, [("team@acme.fr", "https://acme.fr/equipe")]);
    assert_eq!(fetcher.probes(), 1);
}
