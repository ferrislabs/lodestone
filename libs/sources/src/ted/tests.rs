use std::{
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, TimeZone, Utc};
use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{Method::POST, MockServer};
use lodestone_core::{
    ActiveProfile, FetchError, FetchResponse, Profile, ProfileId, ProfileName, ProfileRevision,
    RevisionNumber, SourceName, ValidatedProfile,
    definition::{AgentId, CountryCode},
};
use serde_json::{Value, json};
use url::Url;
use uuid::Uuid;

use super::*;

const PAGE: &str = include_str!("../../tests/fixtures/ted/page.json");

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

fn profile(ted_keywords: Option<&[&str]>) -> ActiveProfile {
    let mut sources = json!({
        "dns_cname": {"patterns": [{"provider": "auth0", "target_suffix": "auth0.com"}]}
    });
    if let Some(keywords) = ted_keywords {
        sources["ted"] = json!({ "keywords": keywords });
    }
    let value = json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": sources,
        "scoring": {"factors": [{"signal": "saas_provider", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
    });
    let profile_id = ProfileId(Uuid::from_u128(7));
    ActiveProfile {
        profile: Profile {
            id: profile_id,
            name: ProfileName::parse("p").unwrap(),
            active: true,
            created_at: at(0),
            updated_at: at(0),
        },
        revision: ProfileRevision {
            profile_id,
            number: RevisionNumber::new(1).unwrap(),
            definition: ValidatedProfile::parse(value).unwrap(),
            created_by: None,
            created_at: at(0),
        },
    }
}

fn fixture(number: &str) -> Value {
    let page: Value = serde_json::from_str(PAGE).unwrap();
    page["notices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|notice| notice["publication-number"] == number)
        .unwrap()
        .clone()
}

fn keywords(list: &[&str]) -> Vec<String> {
    list.iter().map(|keyword| (*keyword).to_owned()).collect()
}

#[test]
fn query_combines_keywords_dates_open_notices_and_country() {
    let query = expert_query(
        &keywords(&["SSO", "gestion des identit\u{e9}s"]),
        &keywords(&["FRA"]),
        at(0),
        7,
    );
    assert_eq!(
        query,
        "(FT~\"SSO\" OR FT~\"gestion des identit\u{e9}s\") AND notice-type=cn* AND deadline-receipt-tender-date-lot>=20270115 AND publication-date>=20270108 AND buyer-country=FRA"
    );
}

#[test]
fn query_escapes_quotes_and_handles_country_lists() {
    let several = expert_query(
        &keywords(&["a\"b"]),
        &keywords(&["fra", "BEL", "x1", "FRA"]),
        at(0),
        1,
    );
    assert!(several.starts_with("(FT~\"a\\\"b\") AND"));
    assert!(several.ends_with("AND buyer-country IN (FRA BEL)"));
    let all = expert_query(&keywords(&["sso"]), &[], at(0), 1);
    assert!(!all.contains("buyer-country"));
    let junk = expert_query(&keywords(&["sso"]), &keywords(&["", "toolong"]), at(0), 1);
    assert!(!junk.contains("buyer-country"));
}

#[test]
fn keywords_are_the_deduplicated_union_of_ted_rules() {
    let profiles = [
        profile(Some(&["SSO", "keycloak"])),
        profile(Some(&["sso", " Okta "])),
        profile(None),
    ];
    assert_eq!(keyword_union(&profiles), ["SSO", "keycloak", "Okta"]);
}

#[test]
fn title_is_the_french_one_without_country_and_cpv_prefix() {
    let candidate = parse_notice(&fixture("625910-2026")).unwrap();
    assert_eq!(
        candidate.title,
        "Prestations autour des applicatifs m\u{e9}tiers du Centre national de la musique (CNM)"
    );
    assert!(
        candidate
            .text
            .starts_with("Maintenance de logiciels de technologies de l'information\n")
    );
    assert!(candidate.text.contains("Tierce maintenance applicative"));
    assert_eq!(candidate.kind.as_str(), "public_tender");
    assert_eq!(candidate.source, SourceName::Ted);
}

#[test]
fn title_falls_back_to_english_then_any_language() {
    let mut notice = fixture("625910-2026");
    notice["notice-title"] = json!({"deu": "Frankreich \u{2013} CPV \u{2013} Titel", "eng": "France \u{2013} CPV label \u{2013} English title"});
    assert_eq!(parse_notice(&notice).unwrap().title, "English title");
    notice["notice-title"] = json!({"deu": "Frankreich \u{2013} CPV \u{2013} Titel"});
    assert_eq!(parse_notice(&notice).unwrap().title, "Titel");
    notice["notice-title"] = json!({"deu": "Titel ohne Praefix"});
    let candidate = parse_notice(&notice).unwrap();
    assert_eq!(candidate.title, "Titel ohne Praefix");
    notice.as_object_mut().unwrap().remove("notice-title");
    notice["title-proc"] = json!({"fra": "Titre de la proc\u{e9}dure"});
    assert_eq!(
        parse_notice(&notice).unwrap().title,
        "Titre de la proc\u{e9}dure"
    );
}

#[test]
fn title_keeps_dashes_inside_the_real_title() {
    let mut notice = fixture("625910-2026");
    notice["notice-title"] = json!({"fra": "France \u{2013} CPV \u{2013} Un \u{2013} deux"});
    assert_eq!(parse_notice(&notice).unwrap().title, "Un \u{2013} deux");
}

#[test]
fn details_carry_number_buyer_earliest_deadline_dates_url_and_country() {
    let candidate = parse_notice(&fixture("625910-2026")).unwrap();
    assert_eq!(candidate.domain.to_string(), "cnm.fr");
    assert_eq!(candidate.details["publication_number"], "625910-2026");
    assert_eq!(candidate.details["buyer"], "CNM");
    assert_eq!(candidate.details["deadline"], "2026-10-07");
    assert_eq!(candidate.details["published"], "2026-09-10");
    assert_eq!(candidate.details["country"], "FRA");
    assert_eq!(
        candidate.details["notice_url"],
        "https://ted.europa.eu/fr/notice/-/detail/625910-2026"
    );
    assert_eq!(
        candidate.observed_at,
        Utc.with_ymd_and_hms(2026, 9, 10, 0, 0, 0).unwrap()
    );
}

#[test]
fn non_french_buyers_use_the_english_or_canonical_url() {
    let candidate = parse_notice(&fixture("665772-2026")).unwrap();
    assert_eq!(candidate.domain.to_string(), "schaarbeek.be");
    assert_eq!(candidate.details["country"], "BEL");
    let mut notice = fixture("665772-2026");
    notice["links"] =
        json!({"html": {"ENG": "https://ted.europa.eu/en/notice/-/detail/665772-2026"}});
    assert_eq!(
        parse_notice(&notice).unwrap().details["notice_url"],
        "https://ted.europa.eu/en/notice/-/detail/665772-2026"
    );
    notice.as_object_mut().unwrap().remove("links");
    assert_eq!(
        parse_notice(&notice).unwrap().details["notice_url"],
        "https://ted.europa.eu/en/notice/-/detail/665772-2026"
    );
}

#[test]
fn notices_without_a_buyer_website_are_skipped() {
    assert_eq!(parse_notice(&fixture("618666-2026")), Err(Skip::NoDomain));
    let mut notice = fixture("625910-2026");
    notice["organisation-internet-address-buyer"] = json!(["https://www.marches-publics.info/x"]);
    assert_eq!(parse_notice(&notice), Err(Skip::NoDomain));
}

#[test]
fn notices_without_number_or_date_are_malformed() {
    let mut notice = fixture("625910-2026");
    notice.as_object_mut().unwrap().remove("publication-date");
    assert_eq!(parse_notice(&notice), Err(Skip::Malformed));
    let mut notice = fixture("625910-2026");
    notice.as_object_mut().unwrap().remove("publication-number");
    assert_eq!(parse_notice(&notice), Err(Skip::Malformed));
}

#[test]
fn a_missing_deadline_is_null() {
    let mut notice = fixture("625910-2026");
    notice
        .as_object_mut()
        .unwrap()
        .remove("deadline-receipt-tender-date-lot");
    assert_eq!(
        parse_notice(&notice).unwrap().details["deadline"],
        Value::Null
    );
}

fn synthetic(count: usize, from: usize) -> Value {
    let notices: Vec<Value> = (0..count)
        .map(|index| {
            let number = from + index;
            json!({
                "publication-number": format!("{number}-2026"),
                "notice-title": {"fra": format!("France \u{2013} Logiciels \u{2013} Titre {number}")},
                "buyer-name": {"fra": ["Acheteur"]},
                "organisation-internet-address-buyer": [format!("https://buyer{number}.example.fr")],
                "publication-date": "2026-09-10+02:00",
                "buyer-country": ["FRA"],
            })
        })
        .collect();
    json!({"notices": notices, "totalNoticeCount": 0})
}

struct PagedPoster {
    total: usize,
    status: u16,
    truncated: bool,
    fixed: Option<Vec<u8>>,
    bodies: Mutex<Vec<Value>>,
}

impl PagedPoster {
    fn new(total: usize) -> Self {
        Self {
            total,
            status: 200,
            truncated: false,
            fixed: None,
            bodies: Mutex::new(Vec::new()),
        }
    }

    fn pages(&self) -> Vec<u64> {
        self.bodies
            .lock()
            .unwrap()
            .iter()
            .map(|body| body["page"].as_u64().unwrap())
            .collect()
    }
}

impl PostFetcher for PagedPoster {
    async fn post(&self, request: &PostRequest) -> Result<FetchResponse, FetchError> {
        assert_eq!(request.source, SourceName::Ted);
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let page = usize::try_from(body["page"].as_u64().unwrap()).unwrap();
        self.bodies.lock().unwrap().push(body);
        let from = (page - 1) * PAGE_SIZE;
        let count = self.total.saturating_sub(from).min(PAGE_SIZE);
        let mut answer = synthetic(count, from);
        answer["totalNoticeCount"] = json!(self.total);
        Ok(FetchResponse {
            status: self.status,
            headers: Vec::new(),
            body: self
                .fixed
                .clone()
                .unwrap_or_else(|| serde_json::to_vec(&answer).unwrap()),
            final_url: request.url.clone(),
            truncated: self.truncated,
        })
    }
}

fn settings(max_pages: u32) -> TedSettings {
    TedSettings {
        base_url: Url::parse("https://example.test/v3/notices/search")
            .unwrap()
            .into(),
        lookback_days: 7,
        max_pages,
        buyer_countries: keywords(&["FRA"]),
    }
}

fn source(poster: PagedPoster, max_pages: u32) -> TedSource<PagedPoster> {
    TedSource::new(Arc::new(poster), settings(max_pages))
}

#[tokio::test]
async fn request_body_carries_query_fields_limit_and_active_scope() {
    let source = source(PagedPoster::new(3), 10);
    source
        .collect(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    let bodies = source.fetcher.bodies.lock().unwrap();
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["limit"], 100);
    assert_eq!(bodies[0]["page"], 1);
    assert_eq!(bodies[0]["scope"], "ACTIVE");
    assert!(bodies[0]["query"].as_str().unwrap().contains("FT~\"sso\""));
    assert!(
        bodies[0]["fields"]
            .as_array()
            .unwrap()
            .contains(&json!("description-lot"))
    );
}

#[tokio::test]
async fn pagination_stops_on_a_short_page() {
    let source = source(PagedPoster::new(110), 10);
    let (candidates, stats) = source
        .collect_with_stats(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    assert_eq!(source.fetcher.pages(), [1, 2]);
    assert_eq!(candidates.len(), 110);
    assert_eq!(stats.pages, 2);
    assert_eq!(stats.records, 110);
    assert_eq!(stats.total_matching, 110);
}

#[tokio::test]
async fn pagination_stops_when_the_total_is_reached_on_a_full_page() {
    let source = source(PagedPoster::new(100), 10);
    let (candidates, _) = source
        .collect_with_stats(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    assert_eq!(source.fetcher.pages(), [1]);
    assert_eq!(candidates.len(), 100);
}

#[tokio::test]
async fn pagination_is_bounded_by_max_pages_and_the_window() {
    let capped = source(PagedPoster::new(10_000), 3);
    capped
        .collect(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    assert_eq!(capped.fetcher.pages(), [1, 2, 3]);
    let windowed = source(PagedPoster::new(100_000), 500);
    windowed
        .collect(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    assert_eq!(windowed.fetcher.pages().len(), WINDOW_CAP / PAGE_SIZE);
}

#[tokio::test]
async fn no_ted_keywords_means_no_request() {
    let source = source(PagedPoster::new(10), 10);
    let candidates = source.collect(&[profile(None)], at(0)).await.unwrap();
    assert!(candidates.is_empty());
    assert!(source.fetcher.pages().is_empty());
}

#[tokio::test]
async fn the_recorded_page_yields_candidates_and_counts_skips() {
    let mut poster = PagedPoster::new(7);
    poster.fixed = Some(PAGE.as_bytes().to_vec());
    let source = source(poster, 10);
    let (candidates, stats) = source
        .collect_with_stats(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    assert_eq!(stats.records, 7);
    assert_eq!(stats.candidates, 4);
    assert_eq!(stats.skipped_no_domain, 3);
    assert_eq!(stats.skipped_malformed, 0);
    let domains: Vec<String> = candidates.iter().map(|c| c.domain.to_string()).collect();
    assert_eq!(
        domains,
        [
            "cnm.fr",
            "lafibrenumerique5962.fr",
            "schaarbeek.be",
            "siaap.fr"
        ]
    );
}

#[tokio::test]
async fn repeated_notices_are_deduplicated() {
    let mut poster = PagedPoster::new(2);
    let page = json!({"notices": [synthetic(1, 5)["notices"][0].clone(), synthetic(1, 5)["notices"][0].clone()], "totalNoticeCount": 2});
    poster.fixed = Some(serde_json::to_vec(&page).unwrap());
    let (candidates, stats) = source(poster, 10)
        .collect_with_stats(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(stats.records, 2);
}

#[tokio::test]
async fn upstream_failures_are_errors() {
    let mut failing = PagedPoster::new(10);
    failing.status = 500;
    let error = source(failing, 10)
        .collect(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap_err();
    assert!(matches!(error, SourceError::Upstream(_)));
    let mut cut = PagedPoster::new(10);
    cut.truncated = true;
    let error = source(cut, 10)
        .collect(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap_err();
    assert!(matches!(error, SourceError::Upstream(_)));
    let mut junk = PagedPoster::new(10);
    junk.fixed = Some(b"{\"nothing\":1}".to_vec());
    let error = source(junk, 10)
        .collect(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap_err();
    assert!(matches!(error, SourceError::Upstream(_)));
}

fn fetch_settings() -> FetchSettings {
    FetchSettings {
        contact: "bot@example.com".to_owned(),
        requests_per_host_per_minute: NonZeroU32::new(1000).unwrap(),
        request_timeout: Duration::from_secs(5),
        max_body_bytes: 8 * 1024 * 1024,
        max_redirects: 3,
        robots_ttl: Duration::from_secs(3600),
        source_quotas: Vec::new(),
        allow_private_targets: true,
    }
}

#[tokio::test]
async fn pages_are_posted_as_json_through_the_polite_fetcher() {
    let server = MockServer::start_async().await;
    let first = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v3/notices/search")
                .header("content-type", "application/json")
                .body_contains("\"page\":1")
                .body_contains("\"limit\":100")
                .body_contains("FT~");
            let mut answer = synthetic(100, 0);
            answer["totalNoticeCount"] = json!(103);
            then.status(200)
                .header("content-type", "application/json")
                .body(serde_json::to_vec(&answer).unwrap());
        })
        .await;
    let second = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v3/notices/search")
                .body_contains("\"page\":2");
            let mut answer = synthetic(3, 100);
            answer["totalNoticeCount"] = json!(103);
            then.status(200)
                .header("content-type", "application/json")
                .body(serde_json::to_vec(&answer).unwrap());
        })
        .await;
    let fetcher = Arc::new(PoliteFetcher::new(fetch_settings()).unwrap());
    let source = TedSource::new(
        fetcher,
        TedSettings {
            base_url: Url::parse(&server.url("/v3/notices/search"))
                .unwrap()
                .into(),
            lookback_days: 7,
            max_pages: 10,
            buyer_countries: keywords(&["FRA"]),
        },
    );
    let (candidates, stats) = source
        .collect_with_stats(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    assert_eq!(candidates.len(), 103);
    assert_eq!(stats.pages, 2);
    first.assert_hits_async(1).await;
    second.assert_hits_async(1).await;
}

fn agent_in(countries: &[&str]) -> SearchAgent {
    let mut agent = SearchAgent::new(AgentId::new("marches"), "Marchés", &["ted"], &["iam"], &[]);
    agent.filters.countries = countries
        .iter()
        .map(|code| CountryCode::new(*code))
        .collect();
    agent
}

#[test]
fn agent_countries_become_alpha_three_buyer_countries() {
    assert_eq!(
        native_countries(&agent_in(&["FR", "BE"]), &[]),
        ["BEL", "FRA"]
    );
    assert_eq!(native_countries(&agent_in(&["FR", "FR"]), &[]), ["FRA"]);
}

#[test]
fn an_agent_without_countries_uses_the_configured_default() {
    let defaults = vec!["DEU".to_owned()];
    assert_eq!(native_countries(&agent_in(&[]), &defaults), ["DEU"]);
    assert_eq!(native_countries(&agent_in(&["FR"]), &defaults), ["FRA"]);
}

#[test]
fn a_country_without_an_alpha_three_code_is_left_to_the_post_filter() {
    assert!(native_countries(&agent_in(&["FR", "ZZ"]), &["DEU".to_owned()]).is_empty());
}
