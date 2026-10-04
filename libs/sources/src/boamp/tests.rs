use std::{
    io::Write,
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, TimeZone, Utc};
use fetch::{FetchSettings, PoliteFetcher};
use flate2::{Compression, write::GzEncoder};
use httpmock::{Method::GET, MockServer};
use lodestone_core::{
    ActiveProfile, FetchError, FetchPurpose, FetchRequest, FetchResponse, Fetcher, Profile,
    ProfileId, ProfileName, ProfileRevision, RevisionNumber, SourceName, ValidatedProfile,
};
use serde_json::{Value, json};
use url::Url;
use uuid::Uuid;

use super::*;

const PAGE: &str = include_str!("../../tests/fixtures/boamp/page.json");

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

fn profile(boamp_keywords: Option<&[&str]>) -> ActiveProfile {
    let mut sources = json!({
        "dns_cname": {"patterns": [{"provider": "auth0", "target_suffix": "auth0.com"}]}
    });
    if let Some(keywords) = boamp_keywords {
        sources["boamp"] = json!({ "keywords": keywords });
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

fn fixture_records() -> Vec<Value> {
    let page: Value = serde_json::from_str(PAGE).unwrap();
    page["results"].as_array().unwrap().clone()
}

fn fixture(idweb: &str) -> Value {
    fixture_records()
        .into_iter()
        .find(|record| record["idweb"] == idweb)
        .unwrap()
}

fn domain_of(idweb: &str) -> Option<String> {
    parse_record(&fixture(idweb))
        .ok()
        .map(|candidate| candidate.domain.to_string())
}

#[test]
fn eforms_buyer_website_wins_over_other_organizations() {
    assert_eq!(
        domain_of("26-95330").as_deref(),
        Some("agglo-rochefortocean.fr")
    );
}

#[test]
fn eforms_without_a_website_falls_back_to_the_buyer_email() {
    assert_eq!(domain_of("26-95410").as_deref(), Some("andour.bzh"));
}

#[test]
fn eforms_platform_website_falls_back_to_the_buyer_email() {
    assert_eq!(domain_of("26-95224").as_deref(), Some("cnosf.org"));
}

#[test]
fn eforms_with_only_platform_contacts_is_skipped() {
    assert_eq!(parse_record(&fixture("26-95504")), Err(Skip::NoDomain));
}

#[test]
fn fnsimple_uses_the_contact_email_when_nothing_else_is_set() {
    assert_eq!(domain_of("26-95027").as_deref(), Some("ehpad-mer.fr"));
}

#[test]
fn fnsimple_without_any_contact_is_skipped() {
    assert_eq!(parse_record(&fixture("26-95288")), Err(Skip::NoDomain));
}

#[test]
fn mapa_uses_the_buyer_coord_url() {
    assert_eq!(
        domain_of("26-95272").as_deref(),
        Some("ville-st-andre-eure.fr")
    );
}

#[test]
fn mapa_with_a_platform_profile_and_a_webmail_is_skipped() {
    assert_eq!(parse_record(&fixture("26-95269")), Err(Skip::NoDomain));
}

#[test]
fn platforms_and_webmails_are_rejected_on_the_registrable_domain() {
    for url in [
        "https://www.marches-publics.info/x",
        "https://agysoft.marches-publics.info",
        "https://marches.megalis.bretagne.bzh/",
        "place.marches-publics.gouv.fr",
    ] {
        assert!(domain_of_url(url).is_none(), "{url}");
    }
    for email in [
        "a@gmail.com",
        "a@orange.fr",
        "a@hotmail.fr",
        "a@outlook.com",
        "a@yahoo.fr",
        "a@aws-france.com",
        "a@free.fr",
    ] {
        assert!(domain_of_email(email).is_none(), "{email}");
    }
    assert_eq!(
        domain_of_url("http://www.limoges.fr/").unwrap().as_str(),
        "limoges.fr"
    );
    assert_eq!(
        domain_of_email("x@mairie.bretagne.bzh").unwrap().as_str(),
        "bretagne.bzh"
    );
}

#[test]
fn the_iam_notice_becomes_a_complete_candidate() {
    let candidate = parse_record(&fixture("26-94122")).unwrap();
    assert_eq!(candidate.source, SourceName::Boamp);
    assert_eq!(candidate.kind.as_str(), "public_tender");
    assert_eq!(
        candidate.title,
        "Achat et mise en oeuvre d'une solution IAM en mode SAAS"
    );
    assert!(candidate.text.chars().count() <= TEXT_MAX_CHARS);
    assert!(candidate.text.contains("IAM"));
    assert_eq!(
        candidate.provenance.as_str(),
        "https://www.boamp.fr/pages/avis/?q=idweb:26-94122"
    );
    assert_eq!(candidate.details["idweb"], "26-94122");
    assert_eq!(candidate.details["published"], "2026-10-01");
    assert_eq!(
        candidate.details["notice_url"],
        "https://www.boamp.fr/pages/avis/?q=idweb:26-94122"
    );
    assert!(candidate.details["buyer"].is_string());
    assert!(candidate.details["deadline"].is_string());
    assert_eq!(
        candidate.observed_at,
        Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()
    );
}

#[test]
fn fnsimple_and_mapa_text_carries_the_object_description() {
    let fnsimple = parse_record(&fixture("26-95027")).unwrap();
    assert!(fnsimple.text.contains("Assistance à maîtrise d'ouvrage"));
    let mapa = parse_record(&fixture("26-95272")).unwrap();
    assert!(mapa.text.contains("Assurance"));
}

#[test]
fn text_is_bounded() {
    let mut record = fixture("26-94122");
    let huge = "x ".repeat(10_000);
    let mut donnees: Value = serde_json::from_str(record["donnees"].as_str().unwrap()).unwrap();
    donnees["EFORMS"]["ContractNotice"]["cac:ProcurementProject"]["cbc:Description"] =
        json!({"@languageID": "FRA", "#text": huge});
    record["donnees"] = Value::String(donnees.to_string());
    let candidate = parse_record(&record).unwrap();
    assert_eq!(candidate.text.chars().count(), TEXT_MAX_CHARS);
}

#[test]
fn malformed_records_are_flagged() {
    assert_eq!(parse_record(&json!({"objet": "x"})), Err(Skip::Malformed));
    let mut record = fixture("26-94122");
    record["dateparution"] = json!("not a date");
    assert_eq!(parse_record(&record), Err(Skip::Malformed));
}

#[test]
fn the_keyword_union_is_deduplicated_ignoring_case() {
    let profiles = [
        profile(Some(&["SSO", "gestion des identités"])),
        profile(Some(&["sso", "MFA"])),
        profile(None),
    ];
    assert_eq!(
        keyword_union(&profiles),
        ["SSO", "gestion des identités", "MFA"]
    );
}

#[test]
fn the_where_clause_unions_keywords_and_bounds_dates() {
    let now = Utc.with_ymd_and_hms(2026, 10, 5, 12, 0, 0).unwrap();
    let keywords = vec!["SSO".to_owned(), "gestion des identités".to_owned()];
    assert_eq!(
        where_clause(&keywords, now, 7),
        "(\"SSO\" or \"gestion des identités\") and nature in (\"APPEL_OFFRE\") and datelimitereponse>=now() and dateparution>=date'2026-09-28'"
    );
}

#[test]
fn keywords_are_escaped_in_the_where_clause() {
    let now = Utc.with_ymd_and_hms(2026, 1, 3, 0, 0, 0).unwrap();
    let keywords = vec![r#"a "quoted" word"#.to_owned(), r"back\slash".to_owned()];
    let clause = where_clause(&keywords, now, 5);
    assert!(clause.starts_with(r#"("a \"quoted\" word" or "back\\slash")"#));
    assert!(clause.ends_with("dateparution>=date'2025-12-29'"));
}

#[test]
fn the_page_url_carries_the_paging_parameters() {
    let base = Url::parse("https://example.test/records").unwrap();
    let url = page_url(&base, "(\"SSO\")", 100);
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert!(pairs.contains(&("where".to_owned(), "(\"SSO\")".to_owned())));
    assert!(pairs.contains(&("order_by".to_owned(), "dateparution desc".to_owned())));
    assert!(pairs.contains(&("limit".to_owned(), "50".to_owned())));
    assert!(pairs.contains(&("offset".to_owned(), "100".to_owned())));
}

fn synthetic(count: usize, from: usize) -> Vec<Value> {
    let template = fixture("26-95330");
    (from..from + count)
        .map(|index| {
            let mut record = template.clone();
            record["idweb"] = json!(format!("26-{index}"));
            record
        })
        .collect()
}

fn body(records: &[Value]) -> Vec<u8> {
    json!({"total_count": records.len(), "results": records})
        .to_string()
        .into_bytes()
}

struct PagedFetcher {
    total: usize,
    status: u16,
    truncated: bool,
    fixed: Option<Vec<u8>>,
    requests: Mutex<Vec<Url>>,
}

impl PagedFetcher {
    fn new(total: usize) -> Self {
        Self {
            total,
            status: 200,
            truncated: false,
            fixed: None,
            requests: Mutex::new(Vec::new()),
        }
    }

    fn offsets(&self) -> Vec<usize> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|url| {
                url.query_pairs()
                    .find(|(key, _)| key == "offset")
                    .unwrap()
                    .1
                    .parse()
                    .unwrap()
            })
            .collect()
    }
}

impl Fetcher for PagedFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        assert_eq!(request.purpose, FetchPurpose::Api);
        assert_eq!(request.source, SourceName::Boamp);
        self.requests.lock().unwrap().push(request.url.clone());
        let offset: usize = request
            .url
            .query_pairs()
            .find(|(key, _)| key == "offset")
            .unwrap()
            .1
            .parse()
            .unwrap();
        let count = self.total.saturating_sub(offset).min(PAGE_SIZE);
        Ok(FetchResponse {
            status: self.status,
            headers: Vec::new(),
            body: self
                .fixed
                .clone()
                .unwrap_or_else(|| body(&synthetic(count, offset))),
            final_url: request.url.clone(),
            truncated: self.truncated,
        })
    }
}

fn settings(max_pages: u32) -> BoampSettings {
    BoampSettings {
        base_url: Url::parse("https://example.test/records").unwrap(),
        lookback_days: 7,
        max_pages,
    }
}

fn source(fetcher: PagedFetcher, max_pages: u32) -> BoampSource<PagedFetcher> {
    BoampSource::new(Arc::new(fetcher), settings(max_pages))
}

#[tokio::test]
async fn pagination_stops_on_a_short_page() {
    let source = source(PagedFetcher::new(110), 20);
    let (candidates, stats) = source
        .collect_with_stats(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    assert_eq!(source.fetcher.offsets(), [0, 50, 100]);
    assert_eq!(candidates.len(), 110);
    assert_eq!(stats.pages, 3);
    assert_eq!(stats.records, 110);
    assert_eq!(stats.skipped_no_domain, 0);
}

#[tokio::test]
async fn pagination_stops_at_max_pages() {
    let source = source(PagedFetcher::new(10_000), 2);
    let candidates = source
        .collect(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    assert_eq!(source.fetcher.offsets(), [0, 50]);
    assert_eq!(candidates.len(), 100);
}

#[tokio::test]
async fn pagination_stops_at_the_offset_cap() {
    let source = source(PagedFetcher::new(1_000_000), 1_000);
    source
        .collect(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    let offsets = source.fetcher.offsets();
    assert_eq!(offsets.len(), OFFSET_CAP / PAGE_SIZE);
    assert_eq!(offsets.last().copied(), Some(OFFSET_CAP - PAGE_SIZE));
}

#[tokio::test]
async fn no_boamp_profile_means_no_request() {
    let source = source(PagedFetcher::new(10), 20);
    let candidates = source.collect(&[profile(None)], at(0)).await.unwrap();
    assert!(candidates.is_empty());
    assert!(source.fetcher.offsets().is_empty());
}

#[tokio::test]
async fn skipped_records_are_counted_by_reason() {
    let mut fetcher = PagedFetcher::new(0);
    fetcher.fixed = Some(PAGE.as_bytes().to_vec());
    let source = source(fetcher, 20);
    let (candidates, stats) = source
        .collect_with_stats(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    assert_eq!(stats.pages, 1);
    assert_eq!(stats.records, 9);
    assert_eq!(stats.candidates, 6);
    assert_eq!(stats.skipped_no_domain, 3);
    assert_eq!(stats.skipped_malformed, 0);
    assert_eq!(candidates.len(), 6);
}

#[tokio::test]
async fn an_error_status_or_a_truncated_body_is_an_upstream_error() {
    let mut failing = PagedFetcher::new(10);
    failing.status = 503;
    let error = source(failing, 20)
        .collect(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap_err();
    assert!(matches!(error, SourceError::Upstream(_)));
    let mut cut = PagedFetcher::new(10);
    cut.truncated = true;
    let error = source(cut, 20)
        .collect(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap_err();
    assert!(matches!(error, SourceError::Upstream(_)));
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
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
async fn gzip_pages_are_read_through_the_polite_fetcher_without_robots() {
    let server = MockServer::start_async().await;
    let robots = server
        .mock_async(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(200).body("User-agent: *\nDisallow: /api/\n");
        })
        .await;
    let first = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/api/records")
                .query_param("offset", "0")
                .query_param("limit", "50")
                .query_param("order_by", "dateparution desc")
                .query_param_exists("where");
            then.status(200)
                .header("content-encoding", "gzip")
                .header("content-type", "application/json")
                .body(gzip(&body(&synthetic(50, 0))));
        })
        .await;
    let second = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/api/records")
                .query_param("offset", "50");
            then.status(200)
                .header("content-encoding", "gzip")
                .header("content-type", "application/json")
                .body(gzip(&body(&synthetic(3, 50))));
        })
        .await;
    let fetcher = Arc::new(PoliteFetcher::new(fetch_settings()).unwrap());
    let source = BoampSource::new(
        fetcher,
        BoampSettings {
            base_url: Url::parse(&server.url("/api/records")).unwrap(),
            lookback_days: 7,
            max_pages: 20,
        },
    );
    let (candidates, stats) = source
        .collect_with_stats(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    assert_eq!(candidates.len(), 53);
    assert_eq!(stats.pages, 2);
    first.assert_hits_async(1).await;
    second.assert_hits_async(1).await;
    robots.assert_hits_async(0).await;
}
