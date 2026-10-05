use std::{
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use common::Secret;
use fetch::{ApiFetcher, ApiRequest, FetchSettings, PoliteFetcher, PostFetcher, PostRequest};
use httpmock::{
    Method::{GET, POST},
    MockServer,
};
use lodestone_core::{FetchError, FetchPurpose, FetchRequest, Fetcher, SourceName};
use rate_limit::{Clock, InMemoryRateLimiter, Quota};
use url::Url;

#[derive(Clone)]
struct FakeClock {
    base: Instant,
    offset: Arc<Mutex<Duration>>,
}

impl FakeClock {
    fn new() -> Self {
        Self {
            base: Instant::now(),
            offset: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }

    fn advance(&self, by: Duration) {
        if let Ok(mut offset) = self.offset.lock() {
            *offset += by;
        }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        let offset = self.offset.lock().map(|o| *o).unwrap_or_default();
        self.base + offset
    }
}

fn has_expected_user_agent(req: &httpmock::prelude::HttpMockRequest) -> bool {
    req.headers.as_ref().is_some_and(|headers| {
        headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("user-agent")
                && value.starts_with("Lodestone/")
                && value.ends_with(" (+bot@example.com)")
        })
    })
}

fn settings() -> FetchSettings {
    FetchSettings {
        contact: "bot@example.com".to_owned(),
        requests_per_host_per_minute: NonZeroU32::new(1000).unwrap(),
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

fn fetcher(settings: FetchSettings) -> PoliteFetcher {
    PoliteFetcher::new(settings).unwrap()
}

#[tokio::test]
async fn user_agent_is_sent_on_page_and_robots() {
    let server = MockServer::start_async().await;
    let page = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/page")
                .matches(has_expected_user_agent);
            then.status(200).body("ok");
        })
        .await;
    let robots = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/robots.txt")
                .matches(has_expected_user_agent);
            then.status(200).body("User-agent: *\nAllow: /\n");
        })
        .await;
    let fetcher = fetcher(settings());
    let response = fetcher
        .fetch(&request(&server, "/page", FetchPurpose::Crawl))
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    page.assert_hits_async(1).await;
    robots.assert_hits_async(1).await;
}

#[tokio::test]
async fn per_host_limit_blocks_and_recovers() {
    let server = MockServer::start_async().await;
    let page = server
        .mock_async(|when, then| {
            when.method(GET).path("/");
            then.status(200);
        })
        .await;
    let clock = FakeClock::new();
    let mut config = settings();
    config.requests_per_host_per_minute = NonZeroU32::new(2).unwrap();
    let fetcher =
        PoliteFetcher::with_limiter(config, InMemoryRateLimiter::with_clock(clock.clone()))
            .unwrap();
    let req = request(&server, "/", FetchPurpose::Api);
    fetcher.fetch(&req).await.unwrap();
    fetcher.fetch(&req).await.unwrap();
    let blocked = fetcher.fetch(&req).await.unwrap_err();
    assert!(matches!(blocked, FetchError::RateLimited { .. }));
    page.assert_hits_async(2).await;
    clock.advance(Duration::from_secs(60));
    fetcher.fetch(&req).await.unwrap();
    page.assert_hits_async(3).await;
}

#[tokio::test]
async fn per_source_quota_is_independent_of_host() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/");
            then.status(200);
        })
        .await;
    let mut config = settings();
    config.source_quotas = vec![(SourceName::Boamp, Quota::per_minute(1))];
    let fetcher = fetcher(config);
    let mut req = request(&server, "/", FetchPurpose::Api);
    req.source = SourceName::Boamp;
    fetcher.fetch(&req).await.unwrap();
    let blocked = fetcher.fetch(&req).await.unwrap_err();
    assert!(matches!(blocked, FetchError::RateLimited { .. }));
    req.source = SourceName::Decp;
    fetcher.fetch(&req).await.unwrap();
    fetcher.fetch(&req).await.unwrap();
}

#[tokio::test]
async fn crawl_respects_robots_and_caches_it() {
    let server = MockServer::start_async().await;
    let robots = server
        .mock_async(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(200).body("User-agent: *\nDisallow: /private\n");
        })
        .await;
    let private = server
        .mock_async(|when, then| {
            when.method(GET).path("/private/a");
            then.status(200);
        })
        .await;
    let public = server
        .mock_async(|when, then| {
            when.method(GET).path("/open");
            then.status(200).body("hello");
        })
        .await;
    let fetcher = fetcher(settings());
    let denied = fetcher
        .fetch(&request(&server, "/private/a", FetchPurpose::Crawl))
        .await
        .unwrap_err();
    assert_eq!(denied, FetchError::DisallowedByRobots);
    let allowed = fetcher
        .fetch(&request(&server, "/open", FetchPurpose::Crawl))
        .await
        .unwrap();
    assert_eq!(allowed.body, b"hello");
    private.assert_hits_async(0).await;
    public.assert_hits_async(1).await;
    robots.assert_hits_async(1).await;
}

#[tokio::test]
async fn robots_404_allows_and_500_refuses() {
    let missing = MockServer::start_async().await;
    missing
        .mock_async(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(404);
        })
        .await;
    missing
        .mock_async(|when, then| {
            when.method(GET).path("/page");
            then.status(200);
        })
        .await;
    let broken = MockServer::start_async().await;
    broken
        .mock_async(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(500);
        })
        .await;
    let page = broken
        .mock_async(|when, then| {
            when.method(GET).path("/page");
            then.status(200);
        })
        .await;
    let fetcher = fetcher(settings());
    let ok = fetcher
        .fetch(&request(&missing, "/page", FetchPurpose::Crawl))
        .await
        .unwrap();
    assert_eq!(ok.status, 200);
    let refused = fetcher
        .fetch(&request(&broken, "/page", FetchPurpose::Crawl))
        .await
        .unwrap_err();
    assert_eq!(refused, FetchError::DisallowedByRobots);
    page.assert_hits_async(0).await;
}

#[tokio::test]
async fn probe_ignores_robots() {
    let server = MockServer::start_async().await;
    let robots = server
        .mock_async(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(200).body("User-agent: *\nDisallow: /\n");
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/.well-known/security.txt");
            then.status(200);
        })
        .await;
    let fetcher = fetcher(settings());
    let response = fetcher
        .fetch(&request(
            &server,
            "/.well-known/security.txt",
            FetchPurpose::Probe,
        ))
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    robots.assert_hits_async(0).await;
}

#[tokio::test]
async fn body_is_capped() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/big");
            then.status(200)
                .header("X-Custom", "Value")
                .body("a".repeat(5000));
        })
        .await;
    let fetcher = fetcher(settings());
    let response = fetcher
        .fetch(&request(&server, "/big", FetchPurpose::Api))
        .await
        .unwrap();
    assert!(response.truncated);
    assert_eq!(response.body.len(), 1024);
    assert_eq!(response.header("x-custom"), Some("Value"));
    assert!(
        response
            .headers
            .iter()
            .all(|(name, _)| name == &name.to_lowercase())
    );
}

#[tokio::test]
async fn slow_response_times_out() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/slow");
            then.status(200).delay(Duration::from_millis(800));
        })
        .await;
    let mut config = settings();
    config.request_timeout = Duration::from_millis(200);
    let fetcher = fetcher(config);
    let error = fetcher
        .fetch(&request(&server, "/slow", FetchPurpose::Api))
        .await
        .unwrap_err();
    assert_eq!(error, FetchError::Timeout);
}

#[tokio::test]
async fn cross_site_redirect_is_refused() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/away");
            then.status(302)
                .header("Location", "http://other-site.example.org/");
        })
        .await;
    let fetcher = fetcher(settings());
    let error = fetcher
        .fetch(&request(&server, "/away", FetchPurpose::Api))
        .await
        .unwrap_err();
    assert_eq!(error, FetchError::Redirect);
}

#[tokio::test]
async fn same_site_redirect_is_followed() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/old");
            then.status(301).header("Location", "/new");
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/new");
            then.status(200).body("moved");
        })
        .await;
    let fetcher = fetcher(settings());
    let response = fetcher
        .fetch(&request(&server, "/old", FetchPurpose::Api))
        .await
        .unwrap();
    assert_eq!(response.body, b"moved");
    assert_eq!(response.final_url.path(), "/new");
}

#[tokio::test]
async fn redirect_loop_is_bounded() {
    let server = MockServer::start_async().await;
    let looping = server
        .mock_async(|when, then| {
            when.method(GET).path("/loop");
            then.status(302).header("Location", "/loop");
        })
        .await;
    let fetcher = fetcher(settings());
    let error = fetcher
        .fetch(&request(&server, "/loop", FetchPurpose::Api))
        .await
        .unwrap_err();
    assert_eq!(error, FetchError::Redirect);
    looping.assert_hits_async(4).await;
}

#[tokio::test]
async fn private_targets_are_refused_by_default() {
    let server = MockServer::start_async().await;
    let page = server
        .mock_async(|when, then| {
            when.method(GET);
            then.status(200);
        })
        .await;
    let mut config = settings();
    config.allow_private_targets = false;
    let fetcher = fetcher(config);
    let ip = request(&server, "/", FetchPurpose::Api);
    assert_eq!(
        fetcher.fetch(&ip).await.unwrap_err(),
        FetchError::NonPublicTarget
    );
    let localhost = FetchRequest {
        url: Url::parse(&format!("http://localhost:{}/", server.port())).unwrap(),
        purpose: FetchPurpose::Api,
        source: SourceName::PublicPage,
    };
    assert_eq!(
        fetcher.fetch(&localhost).await.unwrap_err(),
        FetchError::NonPublicTarget
    );
    page.assert_hits_async(0).await;
}

#[tokio::test]
async fn non_http_scheme_is_invalid() {
    let fetcher = fetcher(settings());
    let req = FetchRequest {
        url: Url::parse("ftp://example.com/file").unwrap(),
        purpose: FetchPurpose::Api,
        source: SourceName::PublicPage,
    };
    assert_eq!(
        fetcher.fetch(&req).await.unwrap_err(),
        FetchError::InvalidUrl
    );
}

#[tokio::test]
async fn robots_redirect_to_same_site_is_followed() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(301).header("Location", "/real-robots.txt");
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/real-robots.txt");
            then.status(200).body("User-agent: *\nDisallow: /private\n");
        })
        .await;
    let public = server
        .mock_async(|when, then| {
            when.method(GET).path("/public");
            then.status(200);
        })
        .await;
    let private = server
        .mock_async(|when, then| {
            when.method(GET).path("/private");
            then.status(200);
        })
        .await;
    let fetcher = fetcher(settings());
    assert_eq!(
        fetcher
            .fetch(&request(&server, "/private", FetchPurpose::Crawl))
            .await
            .unwrap_err(),
        FetchError::DisallowedByRobots
    );
    fetcher
        .fetch(&request(&server, "/public", FetchPurpose::Crawl))
        .await
        .unwrap();
    private.assert_hits_async(0).await;
    public.assert_hits_async(1).await;
}

#[tokio::test]
async fn robots_redirect_to_other_site_refuses_crawl() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(302)
                .header("Location", "http://other-site.example.org/robots.txt");
        })
        .await;
    let page = server
        .mock_async(|when, then| {
            when.method(GET).path("/page");
            then.status(200);
        })
        .await;
    let fetcher = fetcher(settings());
    assert_eq!(
        fetcher
            .fetch(&request(&server, "/page", FetchPurpose::Crawl))
            .await
            .unwrap_err(),
        FetchError::DisallowedByRobots
    );
    page.assert_hits_async(0).await;
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

#[tokio::test]
async fn gzip_encoded_bodies_are_decoded() {
    let server = MockServer::start_async().await;
    let payload = vec![7u8; 1000];
    let encoded = gzip(&payload);
    server
        .mock_async(|when, then| {
            when.method(GET).path("/tile");
            then.status(200)
                .header("content-encoding", "gzip")
                .body(encoded.clone());
        })
        .await;
    let response = fetcher(settings())
        .fetch(&request(&server, "/tile", FetchPurpose::Api))
        .await
        .unwrap();
    assert_eq!(response.body.len(), payload.len());
    assert!(response.body.iter().all(|byte| *byte == 7));
    assert!(!response.truncated);
}

#[tokio::test]
async fn body_cap_applies_after_decompression() {
    let server = MockServer::start_async().await;
    let encoded = gzip(&vec![0u8; 64 * 1024]);
    server
        .mock_async(|when, then| {
            when.method(GET).path("/bomb");
            then.status(200)
                .header("content-encoding", "gzip")
                .body(encoded.clone());
        })
        .await;
    let settings = settings();
    let cap = settings.max_body_bytes;
    let response = fetcher(settings)
        .fetch(&request(&server, "/bomb", FetchPurpose::Api))
        .await
        .unwrap();
    assert!(response.truncated);
    assert_eq!(response.body.len(), cap);
}

#[tokio::test]
async fn documented_api_requests_skip_robots_while_crawl_is_refused() {
    let server = MockServer::start_async().await;
    let robots = server
        .mock_async(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(200).body("User-agent: *\nDisallow: /api/\n");
        })
        .await;
    let api = server
        .mock_async(|when, then| {
            when.method(GET).path("/api/records");
            then.status(200).body("{}");
        })
        .await;
    let fetcher = fetcher(settings());
    let refused = fetcher
        .fetch(&request(&server, "/api/records", FetchPurpose::Crawl))
        .await
        .unwrap_err();
    assert_eq!(refused, FetchError::DisallowedByRobots);
    api.assert_hits_async(0).await;
    let response = fetcher
        .fetch(&request(&server, "/api/records", FetchPurpose::Api))
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    api.assert_hits_async(1).await;
    robots.assert_hits_async(1).await;
}

fn post_request(server: &MockServer, path: &str, body: &str) -> PostRequest {
    PostRequest {
        url: Url::parse(&server.url(path)).unwrap(),
        body: body.as_bytes().to_vec(),
        source: SourceName::Ted,
    }
}

#[tokio::test]
async fn json_post_sends_body_and_skips_robots() {
    let server = MockServer::start_async().await;
    let robots = server
        .mock_async(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(200).body("User-agent: *\nDisallow: /");
        })
        .await;
    let search = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/search")
                .header("content-type", "application/json")
                .matches(has_expected_user_agent)
                .body("{\"query\":\"x\"}");
            then.status(200).body("{\"ok\":true}");
        })
        .await;
    let response = fetcher(settings())
        .post(&post_request(&server, "/search", "{\"query\":\"x\"}"))
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"{\"ok\":true}");
    search.assert_async().await;
    robots.assert_hits_async(0).await;
}

#[tokio::test]
async fn json_post_keeps_the_safeguards() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/big");
            then.status(200).body("x".repeat(4096));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/moved");
            then.status(302).header("location", "/big");
        })
        .await;
    let capped = fetcher(settings())
        .post(&post_request(&server, "/big", "{}"))
        .await
        .unwrap();
    assert!(capped.truncated);
    assert_eq!(capped.body.len(), 1024);
    let redirected = fetcher(settings())
        .post(&post_request(&server, "/moved", "{}"))
        .await;
    assert_eq!(redirected.unwrap_err(), FetchError::Redirect);
    let mut strict = settings();
    strict.allow_private_targets = false;
    let refused = fetcher(strict)
        .post(&post_request(&server, "/big", "{}"))
        .await;
    assert_eq!(refused.unwrap_err(), FetchError::NonPublicTarget);
}

fn api_request(server: &MockServer, path: &str) -> ApiRequest {
    ApiRequest {
        url: Url::parse(&server.url(path)).unwrap(),
        source: SourceName::FranceTravail,
        bearer: None,
        form: None,
    }
}

#[tokio::test]
async fn api_get_sends_the_bearer_and_skips_robots() {
    let server = MockServer::start_async().await;
    let robots = server
        .mock_async(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(200).body("User-agent: *\nDisallow: /");
        })
        .await;
    let search = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/offres")
                .query_param("range", "0-149")
                .header("authorization", "Bearer tok-123")
                .matches(has_expected_user_agent);
            then.status(206)
                .header("content-range", "offres 0-149/300")
                .body("{}");
        })
        .await;
    let mut request = api_request(&server, "/offres?range=0-149");
    request.bearer = Some(Secret::new("tok-123"));
    let response = fetcher(settings()).call(&request).await.unwrap();
    assert_eq!(response.status, 206);
    assert_eq!(response.header("content-range"), Some("offres 0-149/300"));
    search.assert_async().await;
    robots.assert_hits_async(0).await;
}

#[tokio::test]
async fn api_form_is_posted_urlencoded() {
    let server = MockServer::start_async().await;
    let token = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/token")
                .header("content-type", "application/x-www-form-urlencoded")
                .body("grant_type=client_credentials&client_id=a+b&scope=x+y");
            then.status(200).body("{}");
        })
        .await;
    let mut request = api_request(&server, "/token");
    request.form = Some(vec![
        ("grant_type".to_owned(), Secret::new("client_credentials")),
        ("client_id".to_owned(), Secret::new("a b")),
        ("scope".to_owned(), Secret::new("x y")),
    ]);
    let response = fetcher(settings()).call(&request).await.unwrap();
    assert_eq!(response.status, 200);
    token.assert_async().await;
}

#[tokio::test]
async fn api_form_is_not_redirected_and_debug_hides_secrets() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(302).header("location", "/elsewhere");
        })
        .await;
    let mut request = api_request(&server, "/token");
    request.bearer = Some(Secret::new("tok-123"));
    request.form = Some(vec![("client_secret".to_owned(), Secret::new("s3cret"))]);
    let printed = format!("{request:?}");
    assert!(!printed.contains("tok-123"));
    assert!(!printed.contains("s3cret"));
    let error = fetcher(settings()).call(&request).await.unwrap_err();
    assert_eq!(error, FetchError::Redirect);
}
