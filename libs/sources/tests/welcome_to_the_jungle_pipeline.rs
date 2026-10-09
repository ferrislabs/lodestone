use std::{
    num::NonZeroU32,
    slice::from_ref,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{TimeZone, Utc};
use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{Method::GET, Mock, MockServer, prelude::HttpMockRequest};
use lodestone_core::{
    ActiveProfile, Candidate, Profile, ProfileId, ProfileName, ProfileRevision, RevisionNumber,
    SourceError, SourceName, ValidatedProfile, passes_filters,
};
use serde_json::{Value, json};
use sources::welcome_to_the_jungle::{
    Access, AccessSource, Endpoints, WelcomeToTheJungleSettings, WelcomeToTheJungleSource, check,
};
use url::Url;
use uuid::Uuid;

const REFERER: &str = "https://www.welcometothejungle.com/fr/jobs";
const INDEX: &str = "jobs_test";
const ORGS: &str = "orgs_test";

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
        "providers": ["welcome_to_the_jungle"],
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
    profile_with_agents(json!([search_agent("wttj", keywords, json!({}))]))
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

fn access(jobs_index: Option<&str>, max_pages: u32, requests_per_minute: u32) -> Access {
    Access {
        referer: REFERER.to_owned(),
        jobs_index: jobs_index.map(str::to_owned),
        requests_per_minute,
        max_pages,
    }
}

fn endpoints(server: &MockServer) -> Endpoints {
    Endpoints {
        site: Url::parse(&server.url("/")).unwrap(),
        search: Some(Url::parse(&server.url("/")).unwrap()),
    }
}

fn source_with(
    server: &MockServer,
    access: Access,
    max_company_lookups: usize,
) -> WelcomeToTheJungleSource<PoliteFetcher> {
    WelcomeToTheJungleSource::new(
        polite(),
        WelcomeToTheJungleSettings {
            endpoints: endpoints(server),
            access: AccessSource::Fixed(access),
            max_company_lookups,
        },
    )
}

fn source(server: &MockServer, max_pages: u32) -> WelcomeToTheJungleSource<PoliteFetcher> {
    source_with(server, access(Some(INDEX), max_pages, 6000), 20)
}

fn env_body(extra: &str) -> String {
    format!(
        "window.env = {{\"PUBLIC_ALGOLIA_API_KEY_CLIENT\":\"search-only-key\",\"PUBLIC_ALGOLIA_APPLICATION_ID\":\"TESTAPP\",\"PUBLIC_ALGOLIA_ARTICLES_INDEX\":\"articles_test\",\"PUBLIC_ALGOLIA_ORGANIZATIONS_INDEX\":\"{ORGS}\",\"PUBLIC_WTTJ_URL\":\"https://www.welcometothejungle.com\"{extra}}};"
    )
}

async fn mock_env(server: &MockServer) -> Mock<'_> {
    server
        .mock_async(|when, then| {
            when.method(GET).path("/api/env");
            then.status(200)
                .header("content-type", "application/javascript")
                .body(env_body(""));
        })
        .await
}

fn organization(name: &str, slug: &str, website: Option<&str>) -> Value {
    let mut value = json!({"name": name, "slug": slug, "reference": slug.to_uppercase()});
    if let Some(website) = website {
        value["website"] = json!(website);
    }
    value
}

fn job_hit(id: &str, title: &str, organization: Value, extra: Value) -> Value {
    let mut hit = json!({
        "objectID": id,
        "reference": id,
        "name": title,
        "slug": format!("{}_paris", id.to_lowercase()),
        "language": "fr",
        "contract_type": "full_time",
        "remote": "partial",
        "published_at": "2026-10-01T08:30:00Z",
        "offices": [{"city": "Paris", "country": "France", "country_code": "FR"}],
        "summary": "Déploiement de Keycloak et du SSO.",
        "organization": organization,
    });
    if let (Some(hit), Value::Object(extra)) = (hit.as_object_mut(), extra) {
        hit.extend(extra);
    }
    hit
}

fn page(hits: &[Value], pages: u64) -> Value {
    json!({"hits": hits, "nbHits": hits.len(), "nbPages": pages, "page": 0, "hitsPerPage": 30})
}

async fn mock_search<'a>(
    server: &'a MockServer,
    index: &str,
    keyword: &str,
    page_index: u32,
    body: Value,
) -> Mock<'a> {
    let path = format!("/1/indexes/{index}");
    let keyword = keyword.to_owned();
    server
        .mock_async(move |when, then| {
            when.method(GET)
                .path(path)
                .query_param("query", keyword)
                .query_param("page", page_index.to_string())
                .query_param("hitsPerPage", "30")
                .header("referer", REFERER)
                .header("x-algolia-application-id", "TESTAPP")
                .header("x-algolia-api-key", "search-only-key");
            then.status(200).json_body(body);
        })
        .await
}

async fn mock_organisations<'a>(server: &'a MockServer, name: &str, hits: Value) -> Mock<'a> {
    let name = name.to_owned();
    server
        .mock_async(move |when, then| {
            when.method(GET)
                .path(format!("/1/indexes/{ORGS}"))
                .query_param("query", name)
                .header("referer", REFERER);
            then.status(200)
                .json_body(json!({"hits": hits, "nbPages": 1}));
        })
        .await
}

fn lodestone_agent(request: &HttpMockRequest) -> bool {
    request.headers.as_ref().is_some_and(|headers| {
        headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("user-agent")
                && value.starts_with("Lodestone/")
                && value.contains("bot@example.com")
        })
    })
}

fn acme() -> Value {
    organization("Acme", "acme", Some("https://www.acme.fr/"))
}

#[tokio::test]
async fn each_keyword_is_searched_with_the_operator_referer_and_becomes_a_job_offer_signal() {
    let server = MockServer::start_async().await;
    let env = mock_env(&server).await;
    let search = mock_search(
        &server,
        INDEX,
        "keycloak",
        0,
        page(&[job_hit("J1", "Ingénieur IAM", acme(), json!({}))], 1),
    )
    .await;

    let (candidates, stats) = source(&server, 3)
        .collect_with_stats(&[profile(&["keycloak"])])
        .await
        .unwrap();

    env.assert_hits_async(1).await;
    search.assert_hits_async(1).await;
    assert_eq!(stats.queries, 1);
    assert_eq!(stats.requests, 2);
    assert_eq!(candidates.len(), 1);
    let offer = &candidates[0];
    assert_eq!(offer.source, SourceName::WelcomeToTheJungle);
    assert_eq!(offer.kind.as_str(), "job_offer");
    assert_eq!(offer.domain.as_str(), "acme.fr");
    assert_eq!(offer.title, "Ingénieur IAM — Acme");
    assert_eq!(
        offer.provenance.as_str(),
        "https://www.welcometothejungle.com/fr/companies/acme/jobs/j1_paris"
    );
    assert_eq!(
        offer.observed_at,
        Utc.with_ymd_and_hms(2026, 10, 1, 8, 30, 0).unwrap()
    );
    assert!(offer.text.contains("Keycloak"));
    assert_eq!(offer.details["company"], "Acme");
    assert_eq!(offer.details["company_slug"], "acme");
    assert_eq!(offer.details["country"], "FRA");
}

#[tokio::test]
async fn the_user_agent_is_lodestone() {
    let server = MockServer::start_async().await;
    let env = server
        .mock_async(|when, then| {
            when.method(GET).path("/api/env").matches(lodestone_agent);
            then.status(200).body(env_body(""));
        })
        .await;
    let search = server
        .mock_async(|when, then| {
            when.method(GET)
                .path(format!("/1/indexes/{INDEX}"))
                .matches(lodestone_agent);
            then.status(200).json_body(page(&[], 1));
        })
        .await;

    source(&server, 1)
        .collect_with_stats(&[profile(&["keycloak"])])
        .await
        .unwrap();

    env.assert_hits_async(1).await;
    search.assert_hits_async(1).await;
}

#[tokio::test]
async fn keywords_are_deduplicated_and_an_offer_found_twice_counts_once() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    let hit = job_hit("J1", "Ingénieur IAM", acme(), json!({}));
    let keycloak = mock_search(&server, INDEX, "Keycloak", 0, page(from_ref(&hit), 1)).await;
    let sso = mock_search(&server, INDEX, "SSO", 0, page(from_ref(&hit), 1)).await;
    let both = profile_with_agents(json!([
        search_agent("a", &["Keycloak", "SSO"], json!({})),
        search_agent("b", &["keycloak"], json!({})),
    ]));

    let (candidates, stats) = source(&server, 3)
        .collect_with_stats(&[both])
        .await
        .unwrap();

    keycloak.assert_hits_async(1).await;
    sso.assert_hits_async(1).await;
    assert_eq!(stats.queries, 2);
    assert_eq!(stats.records, 2);
    assert_eq!(stats.duplicates, 1);
    assert_eq!(candidates.len(), 1);
}

#[tokio::test]
async fn pages_are_followed_up_to_the_page_cap() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    let first = mock_search(
        &server,
        INDEX,
        "keycloak",
        0,
        page(&[job_hit("J1", "Ingénieur IAM", acme(), json!({}))], 10),
    )
    .await;
    let second = mock_search(
        &server,
        INDEX,
        "keycloak",
        1,
        page(&[job_hit("J2", "Architecte IAM", acme(), json!({}))], 10),
    )
    .await;
    let third = mock_search(&server, INDEX, "keycloak", 2, page(&[], 10)).await;

    let (candidates, stats) = source(&server, 2)
        .collect_with_stats(&[profile(&["keycloak"])])
        .await
        .unwrap();

    first.assert_hits_async(1).await;
    second.assert_hits_async(1).await;
    third.assert_hits_async(0).await;
    assert_eq!(stats.requests, 3);
    assert_eq!(candidates.len(), 2);
}

#[tokio::test]
async fn the_last_page_ends_the_keyword() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    mock_search(
        &server,
        INDEX,
        "keycloak",
        0,
        page(&[job_hit("J1", "Ingénieur IAM", acme(), json!({}))], 1),
    )
    .await;
    let second = mock_search(&server, INDEX, "keycloak", 1, page(&[], 1)).await;

    source(&server, 3)
        .collect_with_stats(&[profile(&["keycloak"])])
        .await
        .unwrap();

    second.assert_hits_async(0).await;
}

#[tokio::test]
async fn a_company_without_website_is_looked_up_once_in_the_organisation_index() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    let globex = organization("Globex", "globex", None);
    mock_search(
        &server,
        INDEX,
        "keycloak",
        0,
        page(
            &[
                job_hit("J1", "Ingénieur IAM", globex.clone(), json!({})),
                job_hit("J2", "Architecte IAM", globex, json!({})),
            ],
            1,
        ),
    )
    .await;
    let lookup = mock_organisations(
        &server,
        "Globex",
        json!([
            {"name": "Globex Studio", "slug": "globex-studio", "website": "https://studio.example"},
            {"name": "Globex", "slug": "globex", "website": "https://globex.io/fr"}
        ]),
    )
    .await;
    let source = source(&server, 1);
    let keycloak = profile(&["keycloak"]);

    let (candidates, stats) = source
        .collect_with_stats(from_ref(&keycloak))
        .await
        .unwrap();
    let (again, second) = source.collect_with_stats(&[keycloak]).await.unwrap();

    lookup.assert_hits_async(1).await;
    assert_eq!(stats.company_lookups, 1);
    assert_eq!(stats.company_cache_hits, 1);
    assert_eq!(second.company_lookups, 0);
    assert_eq!(candidates.len(), 2);
    assert_eq!(again.len(), 2);
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.domain.as_str() == "globex.io")
    );
}

#[tokio::test]
async fn an_offer_without_a_company_website_is_skipped() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    mock_search(
        &server,
        INDEX,
        "keycloak",
        0,
        page(
            &[
                job_hit(
                    "J1",
                    "Ingénieur IAM",
                    organization("Initech", "initech", None),
                    json!({}),
                ),
                job_hit(
                    "J2",
                    "Architecte IAM",
                    organization(
                        "Hooli",
                        "hooli",
                        Some("https://www.welcometothejungle.com/fr/companies/hooli"),
                    ),
                    json!({}),
                ),
            ],
            1,
        ),
    )
    .await;
    mock_organisations(
        &server,
        "Initech",
        json!([{"name": "Initech", "slug": "initech"}]),
    )
    .await;
    mock_organisations(
        &server,
        "Hooli",
        json!([{"name": "Hooli", "slug": "hooli"}]),
    )
    .await;

    let (candidates, stats) = source(&server, 1)
        .collect_with_stats(&[profile(&["keycloak"])])
        .await
        .unwrap();

    assert!(candidates.is_empty());
    assert_eq!(stats.skipped_no_domain, 2);
}

#[tokio::test]
async fn company_lookups_stop_at_the_run_budget() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    mock_search(
        &server,
        INDEX,
        "keycloak",
        0,
        page(
            &[
                job_hit(
                    "J1",
                    "IAM",
                    organization("Initech", "initech", None),
                    json!({}),
                ),
                job_hit("J2", "SSO", organization("Hooli", "hooli", None), json!({})),
            ],
            1,
        ),
    )
    .await;
    let initech = mock_organisations(
        &server,
        "Initech",
        json!([{"name": "Initech", "slug": "initech", "website": "https://initech.fr"}]),
    )
    .await;
    let hooli = mock_organisations(&server, "Hooli", json!([])).await;

    let (candidates, stats) = source_with(&server, access(Some(INDEX), 1, 6000), 1)
        .collect_with_stats(&[profile(&["keycloak"])])
        .await
        .unwrap();

    initech.assert_hits_async(1).await;
    hooli.assert_hits_async(0).await;
    assert_eq!(candidates.len(), 1);
    assert_eq!(stats.skipped_company_budget, 1);
}

#[tokio::test]
async fn a_refused_referer_stops_the_run_with_an_explicit_message() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    let refused = server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/1/indexes/{INDEX}"));
            then.status(403).json_body(
                json!({"message": "Method not allowed with this referer", "status": 403}),
            );
        })
        .await;

    let error = source(&server, 3)
        .collect_with_stats(&[profile(&["keycloak", "sso"])])
        .await
        .unwrap_err();

    refused.assert_hits_async(1).await;
    let SourceError::Upstream(message) = error else {
        panic!("unexpected error {error:?}");
    };
    assert!(
        message.contains("refused the configured referer"),
        "{message}"
    );
    assert!(
        message.contains("Method not allowed with this referer"),
        "{message}"
    );
}

#[tokio::test]
async fn a_rate_limit_stops_the_run_without_retrying() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    let limited = server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/1/indexes/{INDEX}"));
            then.status(429).header("retry-after", "30");
        })
        .await;

    let error = source(&server, 3)
        .collect_with_stats(&[profile(&["keycloak", "sso"])])
        .await
        .unwrap_err();

    limited.assert_hits_async(1).await;
    assert!(matches!(
        error,
        SourceError::RateLimited { retry_after } if retry_after == Duration::from_secs(30)
    ));
}

#[tokio::test]
async fn a_challenge_or_block_stops_the_run_without_retrying() {
    for (status, header, body) in [
        (403, "cf-mitigated", "challenge"),
        (503, "cf-mitigated", "challenge"),
        (403, "x-other", "Access denied"),
    ] {
        let server = MockServer::start_async().await;
        let blocked = server
            .mock_async(|when, then| {
                when.method(GET).path("/api/env");
                then.status(status)
                    .header(header, body)
                    .header("content-type", "text/html")
                    .body(format!("<html><body>{body}</body></html>"));
            })
            .await;

        let error = source(&server, 3)
            .collect_with_stats(&[profile(&["keycloak"])])
            .await
            .unwrap_err();

        blocked.assert_hits_async(1).await;
        let SourceError::Upstream(message) = error else {
            panic!("unexpected error {error:?}");
        };
        assert!(message.contains("blocked"), "{status} {message}");
    }
}

#[tokio::test]
async fn without_keywords_nothing_is_requested() {
    let server = MockServer::start_async().await;
    let env = mock_env(&server).await;
    let other = profile_with_agents(json!([{
        "type": "search",
        "id": "boamp",
        "name": "boamp",
        "providers": ["boamp"],
        "keywords": ["keycloak"],
    }]));

    let (candidates, stats) = source(&server, 3)
        .collect_with_stats(&[other])
        .await
        .unwrap();

    env.assert_hits_async(0).await;
    assert!(candidates.is_empty());
    assert_eq!(stats.requests, 0);
}

#[tokio::test]
async fn post_filters_read_the_contract_the_remote_mode_and_the_country_of_each_offer() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    mock_search(
        &server,
        INDEX,
        "keycloak",
        0,
        page(
            &[
                job_hit(
                    "J1",
                    "Consultant IAM",
                    organization("Freeco", "freeco", Some("https://freeco.fr")),
                    json!({"contract_type": "freelance", "remote": "fulltime"}),
                ),
                job_hit(
                    "J2",
                    "Ingénieur IAM",
                    organization("Officeco", "officeco", Some("https://officeco.fr")),
                    json!({"contract_type": "full_time", "remote": "no"}),
                ),
                job_hit(
                    "J3",
                    "IAM Engineer",
                    organization("Berlinco", "berlinco", Some("https://berlinco.com")),
                    json!({
                        "contract_type": "freelance",
                        "remote": "fulltime",
                        "offices": [{"city": "Berlin", "country": "Deutschland", "country_code": "DE"}],
                    }),
                ),
            ],
            1,
        ),
    )
    .await;
    let agent =
        |filters: Value| profile_with_agents(json!([search_agent("wttj", &["keycloak"], filters)]));
    let (candidates, _) = source(&server, 1)
        .collect_with_stats(&[agent(json!({}))])
        .await
        .unwrap();
    let kept = |filters: Value| -> Vec<String> {
        let profile = agent(filters);
        let filters = &profile
            .revision
            .definition
            .definition()
            .enabled_search_agents(SourceName::WelcomeToTheJungle)
            .next()
            .unwrap()
            .filters;
        candidates
            .iter()
            .filter(|candidate| passes_filters(filters, candidate))
            .map(|candidate| candidate.details["company"].as_str().unwrap().to_owned())
            .collect()
    };

    assert_eq!(
        kept(json!({"remote": "full_remote"})),
        ["Freeco", "Berlinco"]
    );
    assert_eq!(
        kept(json!({"contracts": ["freelance"]})),
        ["Freeco", "Berlinco"]
    );
    assert_eq!(kept(json!({"contracts": ["employee"]})), ["Officeco"]);
    assert_eq!(kept(json!({"countries": ["FR"]})), ["Freeco", "Officeco"]);
}

fn script_page(chunks: &[&str]) -> String {
    let scripts: String = chunks
        .iter()
        .map(|chunk| {
            format!("<script src=\"/_next/static/chunks/{chunk}.js\" async=\"\"></script>")
        })
        .collect();
    format!("<!DOCTYPE html><html><head>{scripts}</head><body></body></html>")
}

#[tokio::test]
async fn the_jobs_index_is_discovered_once_in_the_public_scripts_when_not_set() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    let html = server
        .mock_async(|when, then| {
            when.method(GET).path("/fr/jobs");
            then.status(200).body(script_page(&["layout", "search"]));
        })
        .await;
    let layout = server
        .mock_async(|when, then| {
            when.method(GET).path("/_next/static/chunks/layout.js");
            then.status(200).body("let a={articles:\"articles_test\"};");
        })
        .await;
    let search_chunk = server
        .mock_async(|when, then| {
            when.method(GET).path("/_next/static/chunks/search.js");
            then.status(200)
                .body("const i=e.useIndex(\"wttj_jobs_production_fr\"),o=\"wttj_jobs_production_fr_published_at_desc\";");
        })
        .await;
    let search = mock_search(
        &server,
        "wttj_jobs_production_fr",
        "keycloak",
        0,
        page(&[job_hit("J1", "Ingénieur IAM", acme(), json!({}))], 1),
    )
    .await;
    let source = source_with(&server, access(None, 1, 6000), 5);
    let keycloak = profile(&["keycloak"]);

    let (candidates, stats) = source
        .collect_with_stats(from_ref(&keycloak))
        .await
        .unwrap();
    source.collect_with_stats(&[keycloak]).await.unwrap();

    html.assert_hits_async(1).await;
    search_chunk.assert_hits_async(1).await;
    layout.assert_hits_async(0).await;
    search.assert_hits_async(2).await;
    assert_eq!(stats.discovery_requests, 2);
    assert_eq!(candidates.len(), 1);
}

#[tokio::test]
async fn the_discovery_reads_at_most_eight_scripts_then_asks_for_the_index() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    let names: Vec<String> = (0..12).map(|index| format!("chunk{index}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    server
        .mock_async(|when, then| {
            when.method(GET).path("/fr/jobs");
            then.status(200).body(script_page(&refs));
        })
        .await;
    let chunks = server
        .mock_async(|when, then| {
            when.method(GET).path_contains("/_next/static/chunks/");
            then.status(200).body("let nothing=\"here\";");
        })
        .await;

    let error = source_with(&server, access(None, 1, 6000), 5)
        .collect_with_stats(&[profile(&["keycloak"])])
        .await
        .unwrap_err();

    chunks.assert_hits_async(8).await;
    let SourceError::Upstream(message) = error else {
        panic!("unexpected error {error:?}");
    };
    assert!(message.contains("jobs_index"), "{message}");
}

#[tokio::test]
async fn requests_to_one_host_are_spaced_by_the_rate_setting() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    mock_search(
        &server,
        INDEX,
        "keycloak",
        0,
        page(&[job_hit("J1", "Ingénieur IAM", acme(), json!({}))], 3),
    )
    .await;
    mock_search(&server, INDEX, "keycloak", 1, page(&[], 3)).await;
    let started = Instant::now();

    source_with(&server, access(Some(INDEX), 3, 120), 5)
        .collect_with_stats(&[profile(&["keycloak"])])
        .await
        .unwrap();

    assert!(started.elapsed() >= Duration::from_millis(1000));
}

#[tokio::test]
async fn the_probe_checks_the_referer_against_the_jobs_index() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    let probe = server
        .mock_async(|when, then| {
            when.method(GET)
                .path(format!("/1/indexes/{INDEX}"))
                .query_param("hitsPerPage", "1")
                .header("referer", REFERER);
            then.status(200)
                .json_body(json!({"hits": [], "nbHits": 1234, "nbPages": 1}));
        })
        .await;
    let settings = json!({
        "authorised": true,
        "authorisation_reference": "Courriel du 9 octobre 2026",
        "referer": REFERER,
        "jobs_index": INDEX,
    });

    let message = check(
        polite().as_ref(),
        &endpoints(&server),
        settings.as_object().unwrap(),
    )
    .await
    .unwrap();

    probe.assert_hits_async(1).await;
    assert!(message.contains(INDEX), "{message}");
}

#[tokio::test]
async fn the_probe_without_jobs_index_checks_the_referer_on_the_organisation_index() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    let probe = server
        .mock_async(|when, then| {
            when.method(GET)
                .path(format!("/1/indexes/{ORGS}"))
                .header("referer", REFERER);
            then.status(200)
                .json_body(json!({"hits": [], "nbPages": 1}));
        })
        .await;
    let settings = json!({
        "authorised": true,
        "authorisation_reference": "Courriel du 9 octobre 2026",
        "referer": REFERER,
    });

    let message = check(
        polite().as_ref(),
        &endpoints(&server),
        settings.as_object().unwrap(),
    )
    .await
    .unwrap();

    probe.assert_hits_async(1).await;
    assert!(message.contains("découvert"), "{message}");
}

#[tokio::test]
async fn the_probe_reports_a_refused_referer() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/1/indexes/{INDEX}"));
            then.status(403).json_body(
                json!({"message": "Method not allowed with this referer", "status": 403}),
            );
        })
        .await;
    let settings = json!({
        "authorised": true,
        "authorisation_reference": "Courriel du 9 octobre 2026",
        "referer": REFERER,
        "jobs_index": INDEX,
    });

    let message = check(
        polite().as_ref(),
        &endpoints(&server),
        settings.as_object().unwrap(),
    )
    .await
    .unwrap_err();

    assert!(message.contains("Referer"), "{message}");
    assert!(
        message.contains("Method not allowed with this referer"),
        "{message}"
    );
}

#[tokio::test]
async fn the_probe_needs_the_referer_from_the_settings_before_any_request() {
    let server = MockServer::start_async().await;
    let env = mock_env(&server).await;
    let settings = json!({
        "authorised": true,
        "authorisation_reference": "Courriel du 9 octobre 2026",
    });

    let message = check(
        polite().as_ref(),
        &endpoints(&server),
        settings.as_object().unwrap(),
    )
    .await
    .unwrap_err();

    env.assert_hits_async(0).await;
    assert!(message.contains("Referer"), "{message}");
}

fn find<'a>(candidates: &'a [Candidate], company: &str) -> Option<&'a Candidate> {
    candidates
        .iter()
        .find(|candidate| candidate.details["company"] == company)
}

#[tokio::test]
async fn an_offer_without_a_title_or_date_is_malformed() {
    let server = MockServer::start_async().await;
    mock_env(&server).await;
    mock_search(
        &server,
        INDEX,
        "keycloak",
        0,
        page(
            &[
                job_hit("J1", "IAM", acme(), json!({"name": null})),
                job_hit(
                    "J2",
                    "SSO",
                    organization("Datedco", "datedco", Some("https://datedco.fr")),
                    json!({"published_at": null, "published_at_timestamp": 1_790_000_000}),
                ),
                job_hit(
                    "J3",
                    "PKI",
                    organization("Undated", "undated", Some("https://undated.fr")),
                    json!({"published_at": null}),
                ),
            ],
            1,
        ),
    )
    .await;

    let (candidates, stats) = source(&server, 1)
        .collect_with_stats(&[profile(&["keycloak"])])
        .await
        .unwrap();

    assert_eq!(stats.skipped_malformed, 2);
    assert!(find(&candidates, "Datedco").is_some());
}
