use std::{num::NonZeroU32, time::Duration};

use common::RobotsPolicy;
use fetch::{FetchSettings, PoliteFetcher, RobotsGate};
use httpmock::{Method::GET, MockServer, prelude::HttpMockRequest};
use lodestone_core::{FetchError, FetchPurpose, FetchRequest, Fetcher, SourceName};
use url::Url;

fn settings(per_minute: u32) -> FetchSettings {
    FetchSettings {
        contact: "bot@example.com".to_owned(),
        requests_per_host_per_minute: NonZeroU32::new(per_minute).unwrap(),
        request_timeout: Duration::from_secs(5),
        max_body_bytes: 1024,
        max_redirects: 3,
        robots_ttl: Duration::from_secs(3600),
        source_quotas: Vec::new(),
        allow_private_targets: true,
    }
}

fn request(server: &MockServer, path: &str, purpose: FetchPurpose) -> FetchRequest {
    FetchRequest {
        url: Url::parse(&server.url(path)).unwrap(),
        purpose,
        source: SourceName::PublicPage,
    }
}

fn identified(req: &HttpMockRequest) -> bool {
    req.headers.as_ref().is_some_and(|headers| {
        headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("user-agent")
                && value.starts_with("Lodestone/")
                && value.ends_with(" (+bot@example.com)")
        })
    })
}

#[tokio::test]
async fn a_fetcher_respects_robots_unless_told_otherwise() {
    let fetcher = PoliteFetcher::new(settings(1000)).unwrap();
    assert_eq!(fetcher.robots_policy(), RobotsPolicy::Respect);
}

#[tokio::test]
async fn ignore_reads_a_disallowed_page_without_requesting_robots_txt() {
    let server = MockServer::start_async().await;
    let robots = server
        .mock_async(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(200).body("User-agent: *\nDisallow: /\n");
        })
        .await;
    let page = server
        .mock_async(|when, then| {
            when.method(GET).path("/private").matches(identified);
            then.status(200).body("home");
        })
        .await;
    let gate = RobotsGate::new(RobotsPolicy::Ignore);
    let fetcher = PoliteFetcher::new(settings(1000))
        .unwrap()
        .with_robots_gate(gate);
    let response = fetcher
        .fetch(&request(&server, "/private", FetchPurpose::Crawl))
        .await
        .unwrap();
    assert_eq!(response.body, b"home");
    page.assert_hits_async(1).await;
    robots.assert_hits_async(0).await;
}

#[tokio::test]
async fn the_policy_is_read_at_request_time_and_ignored_refusals_are_counted() {
    let server = MockServer::start_async().await;
    let robots = server
        .mock_async(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(200).body("User-agent: *\nDisallow: /private\n");
        })
        .await;
    let private = server
        .mock_async(|when, then| {
            when.method(GET).path("/private");
            then.status(200);
        })
        .await;
    let open = server
        .mock_async(|when, then| {
            when.method(GET).path("/open");
            then.status(200);
        })
        .await;
    let gate = RobotsGate::default();
    let fetcher = PoliteFetcher::new(settings(1000))
        .unwrap()
        .with_robots_gate(gate.clone());
    let refused = fetcher
        .fetch(&request(&server, "/private", FetchPurpose::Crawl))
        .await
        .unwrap_err();
    assert_eq!(refused, FetchError::DisallowedByRobots);
    private.assert_hits_async(0).await;

    gate.set(RobotsPolicy::Ignore);
    let shared = fetcher.clone();
    shared
        .fetch(&request(&server, "/private", FetchPurpose::Crawl))
        .await
        .unwrap();
    shared
        .fetch(&request(&server, "/open", FetchPurpose::Crawl))
        .await
        .unwrap();
    private.assert_hits_async(1).await;
    open.assert_hits_async(1).await;
    assert_eq!(gate.robots_ignored(), 1);

    gate.set(RobotsPolicy::Respect);
    let refused = fetcher
        .fetch(&request(&server, "/private", FetchPurpose::Crawl))
        .await
        .unwrap_err();
    assert_eq!(refused, FetchError::DisallowedByRobots);
    private.assert_hits_async(1).await;
    robots.assert_hits_async(1).await;
}

#[tokio::test]
async fn ignore_keeps_the_per_host_rate_limit() {
    let server = MockServer::start_async().await;
    let robots = server
        .mock_async(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(200).body("User-agent: *\nDisallow: /\n");
        })
        .await;
    let page = server
        .mock_async(|when, then| {
            when.method(GET).path("/page");
            then.status(200);
        })
        .await;
    let fetcher = PoliteFetcher::new(settings(2))
        .unwrap()
        .with_robots_gate(RobotsGate::new(RobotsPolicy::Ignore));
    let crawl = request(&server, "/page", FetchPurpose::Crawl);
    fetcher.fetch(&crawl).await.unwrap();
    fetcher.fetch(&crawl).await.unwrap();
    let limited = fetcher.fetch(&crawl).await.unwrap_err();
    assert!(matches!(limited, FetchError::RateLimited { .. }));
    page.assert_hits_async(2).await;
    robots.assert_hits_async(0).await;
}

#[tokio::test]
async fn ignore_keeps_the_public_target_check() {
    let server = MockServer::start_async().await;
    let page = server
        .mock_async(|when, then| {
            when.method(GET).path("/page");
            then.status(200);
        })
        .await;
    let mut public_only = settings(1000);
    public_only.allow_private_targets = false;
    let fetcher = PoliteFetcher::new(public_only)
        .unwrap()
        .with_robots_gate(RobotsGate::new(RobotsPolicy::Ignore));
    let refused = fetcher
        .fetch(&request(&server, "/page", FetchPurpose::Crawl))
        .await
        .unwrap_err();
    assert_eq!(refused, FetchError::NonPublicTarget);
    page.assert_hits_async(0).await;
}

#[tokio::test]
async fn ignore_keeps_the_same_site_redirect_policy() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/away");
            then.status(302)
                .header("Location", "http://other-site.example.org/");
        })
        .await;
    let fetcher = PoliteFetcher::new(settings(1000))
        .unwrap()
        .with_robots_gate(RobotsGate::new(RobotsPolicy::Ignore));
    let refused = fetcher
        .fetch(&request(&server, "/away", FetchPurpose::Crawl))
        .await
        .unwrap_err();
    assert_eq!(refused, FetchError::Redirect);
}
