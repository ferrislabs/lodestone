use std::{num::NonZeroU32, slice::from_ref, sync::Arc, time::Duration};

use chrono::{TimeZone, Utc};
use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{Method::GET, Mock, MockServer};
use lodestone_core::{
    ActiveProfile, Candidate, Profile, ProfileId, ProfileName, ProfileRevision, RevisionNumber,
    SourceError, SourceName, ValidatedProfile, passes_filters,
};
use serde_json::{Value, json};
use sources::hacker_news::{HackerNewsSettings, HackerNewsSource};
use url::Url;
use uuid::Uuid;

const OCTOBER: &str = "4001";
const SEPTEMBER: &str = "3001";

fn legacy_value() -> Value {
    json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": {"boamp": {"keywords": ["keycloak"]}},
        "scoring": {"factors": [{"signal": "saas_provider", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
    })
}

fn search_agent(id: &str, keywords: &[&str], filters: Value) -> Value {
    json!({
        "type": "search",
        "id": id,
        "name": id,
        "providers": ["hacker_news_hiring"],
        "keywords": keywords,
        "filters": filters,
    })
}

fn profile_with_agents(agents: Value) -> ActiveProfile {
    let mut value = ValidatedProfile::parse(legacy_value()).unwrap().to_json();
    value["agents"] = agents;
    let definition = ValidatedProfile::parse(value).unwrap();
    let profile_id = ProfileId(Uuid::from_u128(1));
    let epoch = Utc.timestamp_opt(0, 0).unwrap();
    ActiveProfile {
        profile: Profile {
            id: profile_id,
            name: ProfileName::parse("p").unwrap(),
            active: true,
            created_at: epoch,
            updated_at: epoch,
        },
        revision: ProfileRevision {
            profile_id,
            number: RevisionNumber::new(1).unwrap(),
            definition,
            created_by: None,
            created_at: epoch,
        },
    }
}

fn profile(keywords: &[&str]) -> ActiveProfile {
    profile_with_agents(json!([search_agent("hn", keywords, json!({}))]))
}

fn polite() -> Arc<PoliteFetcher> {
    Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact: "bot@example.com".to_owned(),
            requests_per_host_per_minute: NonZeroU32::new(1000).unwrap(),
            request_timeout: Duration::from_secs(5),
            max_body_bytes: 1024 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(60),
            source_quotas: Vec::new(),
            allow_private_targets: true,
        })
        .unwrap(),
    )
}

fn source(server: &MockServer, max_pages: u32) -> HackerNewsSource<PoliteFetcher> {
    HackerNewsSource::new(
        polite(),
        HackerNewsSettings {
            base_url: Url::parse(&server.url("/api/v1/")).unwrap().into(),
            max_pages,
        },
    )
}

fn thread_list() -> Value {
    json!({"hits": [
        {"objectID": "4002", "title": "Ask HN: Who wants to be hired? (October 2026)", "created_at_i": 1_790_000_100},
        {"objectID": OCTOBER, "title": "Ask HN: Who is hiring? (October 2026)", "created_at_i": 1_790_000_000},
        {"objectID": "3002", "title": "Ask HN: Freelancer? Seeking freelancer? (September 2026)", "created_at_i": 1_787_000_100},
        {"objectID": SEPTEMBER, "title": "Ask HN: Who is hiring? (September 2026)", "created_at_i": 1_787_000_000},
        {"objectID": "2001", "title": "Ask HN: Who is hiring? (August 2026)", "created_at_i": 1_784_000_000}
    ]})
}

fn hit(id: &str, parent: &str, html: &str) -> Value {
    json!({
        "objectID": id,
        "parent_id": parent.parse::<u64>().unwrap(),
        "author": "poster",
        "created_at": "2026-10-01T16:02:03.000Z",
        "created_at_i": 1_790_870_523,
        "comment_text": html,
    })
}

fn page(hits: &[Value], pages: u64) -> Value {
    json!({"hits": hits, "nbPages": pages, "page": 0, "hitsPerPage": 100})
}

async fn mock_threads(server: &MockServer) -> Mock<'_> {
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/api/v1/search_by_date")
                .query_param("tags", "story,author_whoishiring")
                .query_param("hitsPerPage", "10")
                .header_exists("user-agent");
            then.status(200).json_body(thread_list());
        })
        .await
}

async fn mock_search<'a>(
    server: &'a MockServer,
    thread: &str,
    keyword: &str,
    page_index: u32,
    body: Value,
) -> Mock<'a> {
    let tag = format!("comment,story_{thread}");
    let keyword = keyword.to_owned();
    server
        .mock_async(move |when, then| {
            when.method(GET)
                .path("/api/v1/search")
                .query_param("query", keyword)
                .query_param("tags", tag)
                .query_param("hitsPerPage", "100")
                .query_param("page", page_index.to_string());
            then.status(200).json_body(body);
        })
        .await
}

fn offer(company: &str, mode: &str, url: &str) -> String {
    format!(
        "{company} | Senior Engineer | {mode} | Full-time<p>Keycloak and SSO. Apply at <a href=\"{url}\">{url}</a>"
    )
}

fn find<'a>(candidates: &'a [Candidate], company: &str) -> &'a Candidate {
    candidates
        .iter()
        .find(|candidate| candidate.details["company"] == company)
        .unwrap()
}

#[tokio::test]
async fn the_first_run_reads_the_current_and_the_previous_threads_then_only_the_current() {
    let server = MockServer::start_async().await;
    let list = mock_threads(&server).await;
    let october = mock_search(&server, OCTOBER, "keycloak", 0, page(&[], 1)).await;
    let september = mock_search(&server, SEPTEMBER, "keycloak", 0, page(&[], 1)).await;
    let august = mock_search(&server, "2001", "keycloak", 0, page(&[], 1)).await;
    let source = source(&server, 5);
    let profile = profile(&["keycloak"]);

    let (_, first) = source.collect_with_stats(from_ref(&profile)).await.unwrap();
    assert_eq!(first.threads, 2);
    assert_eq!(first.queries, 2);
    assert_eq!(first.requests, 3);
    let (_, second) = source.collect_with_stats(&[profile]).await.unwrap();
    assert_eq!(second.threads, 1);
    assert_eq!(second.queries, 1);

    list.assert_hits_async(2).await;
    october.assert_hits_async(2).await;
    september.assert_hits_async(1).await;
    august.assert_hits_async(0).await;
}

#[tokio::test]
async fn a_failed_first_run_still_reads_the_previous_thread_next_time() {
    let server = MockServer::start_async().await;
    let failing = server
        .mock_async(|when, then| {
            when.method(GET).path("/api/v1/search_by_date");
            then.status(503);
        })
        .await;
    let source = source(&server, 5);
    let profile = profile(&["keycloak"]);

    let error = source
        .collect_with_stats(from_ref(&profile))
        .await
        .unwrap_err();
    assert!(matches!(error, SourceError::Upstream(_)));
    failing.delete_async().await;
    mock_threads(&server).await;
    mock_search(&server, OCTOBER, "keycloak", 0, page(&[], 1)).await;
    mock_search(&server, SEPTEMBER, "keycloak", 0, page(&[], 1)).await;

    let (_, stats) = source.collect_with_stats(&[profile]).await.unwrap();

    assert_eq!(stats.threads, 2);
}

#[tokio::test]
async fn every_deduplicated_keyword_is_queried_and_pages_are_followed_up_to_the_cap() {
    let server = MockServer::start_async().await;
    mock_threads(&server).await;
    mock_search(&server, SEPTEMBER, "keycloak", 0, page(&[], 1)).await;
    mock_search(&server, SEPTEMBER, "sso", 0, page(&[], 1)).await;
    let first = mock_search(
        &server,
        OCTOBER,
        "keycloak",
        0,
        page(
            &[hit(
                "11",
                OCTOBER,
                &offer("Acme", "REMOTE", "https://acme.com"),
            )],
            9,
        ),
    )
    .await;
    let second = mock_search(
        &server,
        OCTOBER,
        "keycloak",
        1,
        page(
            &[hit(
                "12",
                OCTOBER,
                &offer("Beta", "REMOTE", "https://beta.io"),
            )],
            9,
        ),
    )
    .await;
    let beyond_cap = mock_search(&server, OCTOBER, "keycloak", 2, page(&[], 9)).await;
    let sso = mock_search(&server, OCTOBER, "sso", 0, page(&[], 1)).await;
    let source = source(&server, 2);
    let profile = profile_with_agents(json!([
        search_agent("a", &["keycloak", "sso"], json!({})),
        search_agent("b", &["Keycloak"], json!({})),
    ]));

    let (candidates, stats) = source.collect_with_stats(&[profile]).await.unwrap();

    assert_eq!(candidates.len(), 2);
    assert_eq!(stats.queries, 4);
    first.assert_hits_async(1).await;
    second.assert_hits_async(1).await;
    beyond_cap.assert_hits_async(0).await;
    sso.assert_hits_async(1).await;
}

#[tokio::test]
async fn posts_are_deduplicated_by_object_id_and_replies_are_ignored() {
    let server = MockServer::start_async().await;
    mock_threads(&server).await;
    mock_search(&server, SEPTEMBER, "keycloak", 0, page(&[], 1)).await;
    mock_search(&server, SEPTEMBER, "sso", 0, page(&[], 1)).await;
    let acme = hit("11", OCTOBER, &offer("Acme", "REMOTE", "https://acme.com"));
    let reply = hit("99", "11", "Is this still open? https://elsewhere.com");
    mock_search(
        &server,
        OCTOBER,
        "keycloak",
        0,
        page(&[acme.clone(), reply], 1),
    )
    .await;
    mock_search(&server, OCTOBER, "sso", 0, page(&[acme], 1)).await;
    let source = source(&server, 5);
    let profile = profile(&["keycloak", "sso"]);

    let (candidates, stats) = source.collect_with_stats(&[profile]).await.unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(stats.records, 3);
    assert_eq!(stats.duplicates, 1);
    assert_eq!(stats.replies, 1);
    assert_eq!(stats.candidates, 1);
}

#[tokio::test]
async fn a_post_becomes_a_job_offer_candidate_with_its_provenance() {
    let server = MockServer::start_async().await;
    mock_threads(&server).await;
    mock_search(&server, SEPTEMBER, "keycloak", 0, page(&[], 1)).await;
    let html = "Acme Corp | Senior Rust Engineer | Berlin, Germany | REMOTE | Contract<p>We use Keycloak &amp; Rust.<p>Apply at <a href=\"https:&#x2F;&#x2F;www.acme.com&#x2F;jobs\">https:&#x2F;&#x2F;www.acme.com&#x2F;jo...</a>";
    mock_search(
        &server,
        OCTOBER,
        "keycloak",
        0,
        page(&[hit("11", OCTOBER, html)], 1),
    )
    .await;
    let source = source(&server, 5);

    let (candidates, _) = source
        .collect_with_stats(&[profile(&["keycloak"])])
        .await
        .unwrap();

    let candidate = &candidates[0];
    assert_eq!(candidate.source, SourceName::HackerNewsHiring);
    assert_eq!(candidate.kind.as_str(), "job_offer");
    assert_eq!(candidate.domain.as_str(), "acme.com");
    assert_eq!(candidate.title, "Senior Rust Engineer - Acme Corp");
    assert_eq!(
        candidate.provenance.as_str(),
        "https://news.ycombinator.com/item?id=11"
    );
    assert_eq!(
        candidate.observed_at,
        Utc.with_ymd_and_hms(2026, 10, 1, 16, 2, 3).unwrap()
    );
    assert!(candidate.text.contains("We use Keycloak & Rust."));
    assert!(candidate.text.contains("https://www.acme.com/jobs"));
    assert_eq!(candidate.details["role"], "Senior Rust Engineer");
    assert_eq!(candidate.details["location"], json!(["Berlin, Germany"]));
    assert_eq!(candidate.details["work_mode"], "full_remote");
    assert_eq!(candidate.details["contract_hints"], json!(["freelance"]));
}

#[tokio::test]
async fn posts_without_a_company_url_are_skipped_and_counted() {
    let server = MockServer::start_async().await;
    mock_threads(&server).await;
    mock_search(&server, SEPTEMBER, "keycloak", 0, page(&[], 1)).await;
    let hits = [
        hit("11", OCTOBER, "Acme | Dev | REMOTE<p>Email jobs@acme.com"),
        hit(
            "12",
            OCTOBER,
            "Beta | Dev<p>https://news.ycombinator.com/item?id=1 https://jobs.lever.co/beta https://www.linkedin.com/jobs/1",
        ),
        hit(
            "13",
            OCTOBER,
            &offer("Gamma", "REMOTE", "https://gamma.dev"),
        ),
        json!({"objectID": "14", "parent_id": 4001, "created_at_i": 1_790_870_523}),
    ];
    mock_search(&server, OCTOBER, "keycloak", 0, page(&hits, 1)).await;
    let source = source(&server, 5);

    let (candidates, stats) = source
        .collect_with_stats(&[profile(&["keycloak"])])
        .await
        .unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(stats.skipped_no_domain, 2);
    assert_eq!(stats.skipped_malformed, 1);
}

#[tokio::test]
async fn the_shared_post_filters_drop_onsite_posts_for_a_full_remote_agent() {
    let server = MockServer::start_async().await;
    mock_threads(&server).await;
    mock_search(&server, SEPTEMBER, "keycloak", 0, page(&[], 1)).await;
    let hits = [
        hit(
            "11",
            OCTOBER,
            &offer("Remoteco", "REMOTE", "https://remoteco.io"),
        ),
        hit(
            "12",
            OCTOBER,
            &offer("Officeco", "ONSITE", "https://officeco.io"),
        ),
        hit("13", OCTOBER, &offer("Mixco", "Hybrid", "https://mixco.io")),
        hit(
            "14",
            OCTOBER,
            "Freeco | Dev | REMOTE | Contract<p>Keycloak https://freeco.io",
        ),
    ];
    mock_search(&server, OCTOBER, "keycloak", 0, page(&hits, 1)).await;
    let source = source(&server, 5);
    let agent =
        |filters: Value| profile_with_agents(json!([search_agent("hn", &["keycloak"], filters)]));
    let full_remote = agent(json!({"remote": "full_remote"}));
    let (candidates, _) = source
        .collect_with_stats(from_ref(&full_remote))
        .await
        .unwrap();
    let filters_of = |profile: &ActiveProfile| {
        profile
            .revision
            .definition
            .definition()
            .enabled_search_agents(SourceName::HackerNewsHiring)
            .next()
            .unwrap()
            .filters
            .clone()
    };
    let kept = |filters: Value| -> Vec<String> {
        let filters = filters_of(&agent(filters));
        candidates
            .iter()
            .filter(|candidate| passes_filters(&filters, candidate))
            .map(|candidate| candidate.details["company"].as_str().unwrap().to_owned())
            .collect()
    };

    assert_eq!(
        kept(json!({"remote": "full_remote"})),
        ["Remoteco", "Freeco"]
    );
    assert_eq!(
        kept(json!({"remote": "remote_or_hybrid"})),
        ["Remoteco", "Mixco", "Freeco"]
    );
    assert_eq!(kept(json!({"contracts": ["freelance"]})), ["Freeco"]);
    assert_eq!(kept(json!({"contracts": ["employee"]})).len(), 3);
    assert_eq!(find(&candidates, "Officeco").details["work_mode"], "onsite");
}

#[tokio::test]
async fn no_keyword_means_no_request() {
    let server = MockServer::start_async().await;
    let list = mock_threads(&server).await;
    let source = source(&server, 5);
    let profile = profile_with_agents(json!([{
        "type": "search",
        "id": "other",
        "name": "other",
        "providers": ["boamp"],
        "keywords": ["keycloak"],
    }]));

    let (candidates, stats) = source.collect_with_stats(&[profile]).await.unwrap();

    assert!(candidates.is_empty());
    assert_eq!(stats.requests, 0);
    list.assert_hits_async(0).await;
}

#[tokio::test]
async fn a_missing_hiring_thread_is_an_upstream_error() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/api/v1/search_by_date");
            then.status(200).json_body(json!({"hits": [
                {"objectID": "1", "title": "Ask HN: Who wants to be hired?", "created_at_i": 1}
            ]}));
        })
        .await;
    let source = source(&server, 5);

    let error = source
        .collect_with_stats(&[profile(&["keycloak"])])
        .await
        .unwrap_err();

    assert!(matches!(error, SourceError::Upstream(message) if message.contains("thread")));
}

#[tokio::test]
async fn a_long_throttle_is_reported_instead_of_waited_for() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/api/v1/search_by_date");
            then.status(429).header("retry-after", "3600");
        })
        .await;
    let source = source(&server, 5);

    let error = source
        .collect_with_stats(&[profile(&["keycloak"])])
        .await
        .unwrap_err();

    assert!(matches!(error, SourceError::RateLimited { .. }));
}
