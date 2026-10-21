use std::{collections::VecDeque, sync::Mutex};

use chrono::{TimeZone, Utc};
use lodestone_core::{
    FetchError, Profile, ProfileId, ProfileName, ProfileRevision, RevisionNumber, ValidatedProfile,
};
use serde_json::{Value, json};
use uuid::Uuid;

use super::*;

const SEARCH: &str =
    include_str!("../../tests/fixtures/france_travail/offres-search.synthetic.json");
const PAGE_TWO: &str =
    include_str!("../../tests/fixtures/france_travail/offres-search-page2.synthetic.json");
const FIXTURE_TOKEN: &str = "synthetic-access-token-0001";

fn profile(seed: u128, keywords: Option<&[&str]>) -> ActiveProfile {
    let mut sources = json!({
        "dns_cname": {"patterns": [{"provider": "auth0", "target_suffix": "auth0.com"}]}
    });
    if let Some(keywords) = keywords {
        sources["france_travail"] = json!({ "keywords": keywords });
    }
    let value = json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": sources,
        "scoring": {"factors": [{"signal": "saas_provider", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
    });
    let profile_id = ProfileId(Uuid::from_u128(seed));
    let epoch = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
    ActiveProfile {
        profile: Profile {
            id: profile_id,
            name: ProfileName::parse(&format!("p{seed}")).unwrap(),
            active: true,
            created_at: epoch,
            updated_at: epoch,
        },
        revision: ProfileRevision {
            profile_id,
            number: RevisionNumber::new(1).unwrap(),
            definition: ValidatedProfile::parse(value).unwrap(),
            created_by: None,
            created_at: epoch,
        },
    }
}

fn response(status: u16, headers: &[(&str, &str)], body: &str) -> FetchResponse {
    FetchResponse {
        status,
        headers: headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        body: body.as_bytes().to_vec(),
        final_url: Url::parse("https://example.test/").unwrap(),
        truncated: false,
    }
}

struct Recorded {
    url: Url,
    bearer: Option<String>,
}

struct Fake<S: Fn(&Url, usize) -> FetchResponse + Send + Sync> {
    search: S,
    recorded: Mutex<Vec<Recorded>>,
}

impl<S: Fn(&Url, usize) -> FetchResponse + Send + Sync> Fake<S> {
    fn searches(&self) -> Vec<(Url, Option<String>)> {
        self.recorded
            .lock()
            .unwrap()
            .iter()
            .map(|call| (call.url.clone(), call.bearer.clone()))
            .collect()
    }

    fn bearers(&self) -> Vec<Option<String>> {
        self.searches()
            .into_iter()
            .map(|(_, bearer)| bearer)
            .collect()
    }
}

impl<S: Fn(&Url, usize) -> FetchResponse + Send + Sync> ApiFetcher for Fake<S> {
    async fn call(&self, request: &ApiRequest) -> Result<FetchResponse, FetchError> {
        assert_eq!(request.source, SourceName::FranceTravail);
        assert!(request.form.is_none());
        let index = {
            let mut recorded = self.recorded.lock().unwrap();
            recorded.push(Recorded {
                url: request.url.clone(),
                bearer: request
                    .bearer
                    .as_ref()
                    .map(|token| token.expose().to_owned()),
            });
            recorded.len()
        };
        Ok((self.search)(&request.url, index))
    }
}

#[derive(Default)]
struct FakeAccess {
    queue: Mutex<VecDeque<String>>,
    current: Mutex<Option<String>>,
    requested: Mutex<usize>,
    invalidated: Mutex<usize>,
}

impl FakeAccess {
    fn with(tokens: &[&str]) -> Self {
        Self {
            queue: Mutex::new(tokens.iter().map(|token| (*token).to_owned()).collect()),
            ..Self::default()
        }
    }

    fn requested(&self) -> usize {
        *self.requested.lock().unwrap()
    }

    fn invalidated(&self) -> usize {
        *self.invalidated.lock().unwrap()
    }
}

impl AccessProvider for FakeAccess {
    async fn access(&self) -> Result<FranceTravailAccess, SourceError> {
        *self.requested.lock().unwrap() += 1;
        let mut current = self.current.lock().unwrap();
        if current.is_none() {
            *current = self.queue.lock().unwrap().pop_front();
        }
        let token = current
            .clone()
            .ok_or_else(|| SourceError::Upstream("france travail is not ready".to_owned()))?;
        Ok(FranceTravailAccess {
            token: Secret::new(token),
            api_base_url: Url::parse("https://api.example.test/partenaire/offresdemploi/v2")
                .unwrap(),
        })
    }

    fn invalidate(&self) {
        *self.invalidated.lock().unwrap() += 1;
        *self.current.lock().unwrap() = None;
    }
}

fn settings(max_pages: u32) -> FranceTravailSettings {
    FranceTravailSettings {
        published_since_days: 7,
        max_pages,
    }
}

fn source_with<S: Fn(&Url, usize) -> FetchResponse + Send + Sync>(
    tokens: &[&str],
    search: S,
    max_pages: u32,
) -> FranceTravailSource<Fake<S>, FakeAccess> {
    let fetcher = Fake {
        search,
        recorded: Mutex::new(Vec::new()),
    };
    FranceTravailSource::new(
        Arc::new(fetcher),
        FakeAccess::with(tokens),
        settings(max_pages),
    )
}

fn query(url: &Url, name: &str) -> String {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default()
}

fn offers(count: usize, from: usize) -> String {
    let list: Vec<Value> = (from..from + count)
        .map(|n| {
            json!({
                "id": format!("OFF{n:06}"),
                "intitule": format!("Offre {n}"),
                "dateCreation": "2026-10-02T08:00:00.000Z",
                "entreprise": {"url": format!("https://company{n}.fr")},
            })
        })
        .collect();
    json!({"resultats": list}).to_string()
}

fn ok_tokens() -> [&'static str; 1] {
    [FIXTURE_TOKEN]
}

fn one_profile() -> [ActiveProfile; 1] {
    [profile(1, Some(&["iam"]))]
}

#[test]
fn published_since_maps_to_the_closest_allowed_value() {
    assert_eq!(closest_published_since(7), 7);
    assert_eq!(closest_published_since(5), 7);
    assert_eq!(closest_published_since(10), 7);
    assert_eq!(closest_published_since(11), 14);
    assert_eq!(closest_published_since(0), 1);
    assert_eq!(closest_published_since(2), 3);
    assert_eq!(closest_published_since(20), 14);
    assert_eq!(closest_published_since(365), 31);
}

#[test]
fn content_range_total_reads_the_total_after_the_slash() {
    assert_eq!(content_range_total("offres 0-149/1234"), Some(1234));
    assert_eq!(content_range_total("offres 0-149/*"), None);
    assert_eq!(content_range_total("garbage"), None);
}

#[tokio::test]
async fn search_request_carries_bearer_keyword_range_and_window() {
    let source = source_with(&ok_tokens(), |_, _| response(204, &[], ""), 5);
    source
        .collect(&[profile(1, Some(&["ing\u{e9}nieur IAM"]))], Utc::now())
        .await
        .unwrap();
    let searches = source.fetcher.searches();
    assert_eq!(searches.len(), 1);
    let (url, bearer) = &searches[0];
    assert_eq!(url.path(), "/partenaire/offresdemploi/v2/offres/search");
    assert_eq!(bearer.as_deref(), Some(FIXTURE_TOKEN));
    assert_eq!(query(url, "motsCles"), "ing\u{e9}nieur IAM");
    assert_eq!(query(url, "range"), "0-149");
    assert_eq!(query(url, "publieeDepuis"), "7");
}

#[tokio::test]
async fn the_access_is_asked_for_every_request_and_never_invalidated_on_success() {
    let source = source_with(&ok_tokens(), |_, _| response(204, &[], ""), 5);
    source
        .collect_with_stats(&[profile(1, Some(&["iam", "sso", "keycloak"]))])
        .await
        .unwrap();
    assert_eq!(source.fetcher.searches().len(), 3);
    assert_eq!(source.access.requested(), 3);
    assert_eq!(source.access.invalidated(), 0);
}

#[tokio::test]
async fn a_401_refreshes_the_token_once_and_retries() {
    let source = source_with(
        &["stale", "fresh"],
        |_, index| {
            if index == 1 {
                response(401, &[("www-authenticate", "Bearer")], "")
            } else {
                response(200, &[], &offers(2, 0))
            }
        },
        5,
    );
    let (candidates, _) = source.collect_with_stats(&one_profile()).await.unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(
        source.fetcher.bearers(),
        [Some("stale".to_owned()), Some("fresh".to_owned())]
    );
    assert_eq!(source.access.invalidated(), 1);
}

#[tokio::test]
async fn a_second_401_fails_instead_of_looping() {
    let source = source_with(&["one", "two"], |_, _| response(401, &[], ""), 5);
    let error = source.collect_with_stats(&one_profile()).await.unwrap_err();
    assert!(matches!(error, SourceError::Upstream(message) if message.contains("401")));
    assert_eq!(source.fetcher.searches().len(), 2);
}

#[tokio::test]
async fn an_unavailable_access_stops_the_run_before_any_search() {
    let source = source_with(&[], |_, _| response(204, &[], ""), 5);
    let error = source.collect_with_stats(&one_profile()).await.unwrap_err();
    assert!(matches!(error, SourceError::Upstream(message) if message.contains("not ready")));
    assert!(source.fetcher.searches().is_empty());
}

#[tokio::test]
async fn pagination_follows_206_and_content_range_until_the_total() {
    let source = source_with(
        &ok_tokens(),
        |url, _| match query(url, "range").as_str() {
            "0-149" => response(
                206,
                &[("content-range", "offres 0-149/320")],
                &offers(150, 0),
            ),
            "150-299" => response(
                206,
                &[("content-range", "offres 150-299/320")],
                &offers(150, 150),
            ),
            "300-449" => response(
                206,
                &[("content-range", "offres 300-319/320")],
                &offers(20, 300),
            ),
            _ => response(204, &[], ""),
        },
        20,
    );
    let (candidates, stats) = source.collect_with_stats(&one_profile()).await.unwrap();
    let ranges: Vec<String> = source
        .fetcher
        .searches()
        .iter()
        .map(|(url, _)| query(url, "range"))
        .collect();
    assert_eq!(ranges, ["0-149", "150-299", "300-449"]);
    assert_eq!(candidates.len(), 320);
    assert_eq!(stats.total_matching, 320);
    assert_eq!(stats.requests, 3);
}

#[tokio::test]
async fn a_complete_200_stops_pagination() {
    let source = source_with(&ok_tokens(), |_, _| response(200, &[], &offers(40, 0)), 20);
    let (candidates, stats) = source.collect_with_stats(&one_profile()).await.unwrap();
    assert_eq!(candidates.len(), 40);
    assert_eq!(stats.requests, 1);
    assert_eq!(stats.total_matching, 40);
}

fn endless(_: &Url, index: usize) -> FetchResponse {
    response(
        206,
        &[("content-range", "offres 0-149/100000")],
        &offers(150, index * 150),
    )
}

#[tokio::test]
async fn pagination_is_bounded_by_max_pages() {
    let source = source_with(&ok_tokens(), endless, 3);
    source.collect_with_stats(&one_profile()).await.unwrap();
    assert_eq!(source.fetcher.searches().len(), 3);
}

#[tokio::test]
async fn pagination_is_bounded_by_the_last_index_cap() {
    let source = source_with(&ok_tokens(), endless, 500);
    source.collect_with_stats(&one_profile()).await.unwrap();
    let searches = source.fetcher.searches();
    assert_eq!(searches.len(), 21);
    assert_eq!(query(&searches[20].0, "range"), "3000-3149");
}

#[tokio::test]
async fn a_204_means_no_offers_and_no_candidates() {
    let source = source_with(&ok_tokens(), |_, _| response(204, &[], ""), 5);
    let (candidates, stats) = source.collect_with_stats(&one_profile()).await.unwrap();
    assert!(candidates.is_empty());
    assert_eq!(stats.requests, 1);
    assert_eq!(stats.records, 0);
}

#[tokio::test(start_paused = true)]
async fn a_429_with_a_short_retry_after_waits_and_retries() {
    let source = source_with(
        &ok_tokens(),
        |_, index| {
            if index == 1 {
                response(429, &[("retry-after", "2")], "")
            } else {
                response(200, &[], &offers(1, 0))
            }
        },
        5,
    );
    let started = tokio::time::Instant::now();
    let (candidates, _) = source.collect_with_stats(&one_profile()).await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert!(started.elapsed() >= StdDuration::from_secs(2));
    assert_eq!(source.fetcher.searches().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn a_429_with_a_long_retry_after_is_reported_as_rate_limited() {
    let source = source_with(
        &ok_tokens(),
        |_, _| response(429, &[("retry-after", "600")], ""),
        5,
    );
    let error = source.collect_with_stats(&one_profile()).await.unwrap_err();
    assert_eq!(
        error,
        SourceError::RateLimited {
            retry_after: StdDuration::from_secs(600)
        }
    );
    assert_eq!(source.fetcher.searches().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_persistent_429_gives_up_after_bounded_retries() {
    let source = source_with(
        &ok_tokens(),
        |_, _| response(429, &[("retry-after", "1")], ""),
        5,
    );
    let error = source.collect_with_stats(&one_profile()).await.unwrap_err();
    assert!(matches!(error, SourceError::RateLimited { .. }));
    assert_eq!(source.fetcher.searches().len(), 3);
}

#[tokio::test]
async fn other_statuses_are_upstream_errors() {
    let source = source_with(&ok_tokens(), |_, _| response(400, &[], "bad"), 5);
    let error = source.collect_with_stats(&one_profile()).await.unwrap_err();
    assert!(matches!(error, SourceError::Upstream(message) if message.contains("400")));
}

#[tokio::test]
async fn offers_are_deduplicated_across_profiles_and_keywords() {
    let first_two = {
        let page: Value = serde_json::from_str(SEARCH).unwrap();
        json!({"resultats": page["resultats"].as_array().unwrap()[..2].to_vec()}).to_string()
    };
    let source = source_with(
        &ok_tokens(),
        |url, _| match query(url, "motsCles").as_str() {
            "keycloak" => response(200, &[], SEARCH),
            "sso" => response(200, &[], &first_two),
            _ => response(200, &[], PAGE_TWO),
        },
        5,
    );
    let profiles = [
        profile(1, Some(&["keycloak", "sso"])),
        profile(2, Some(&["SSO", "iam"])),
        profile(3, None),
    ];
    let (candidates, stats) = source.collect_with_stats(&profiles).await.unwrap();
    assert_eq!(stats.queries, 3);
    let ids: Vec<&str> = candidates
        .iter()
        .map(|candidate| candidate.details["offer_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["184TKRB", "184TKRC", "184TKRF", "184TKRG"]);
    assert_eq!(stats.duplicates, 2);
    assert_eq!(stats.candidates, 4);
}

#[tokio::test]
async fn an_offer_found_by_two_queries_is_one_candidate() {
    let source = source_with(&ok_tokens(), |_, _| response(200, &[], SEARCH), 5);
    let (candidates, stats) = source
        .collect_with_stats(&[profile(1, Some(&["iam"])), profile(2, Some(&["sso"]))])
        .await
        .unwrap();
    assert_eq!(candidates.len(), 3);
    assert_eq!(stats.queries, 2);
    assert_eq!(stats.duplicates, 5);
}

#[tokio::test]
async fn no_france_travail_keywords_means_no_request() {
    let source = source_with(&ok_tokens(), |_, _| response(204, &[], ""), 5);
    let candidates = source
        .collect(&[profile(1, None)], Utc::now())
        .await
        .unwrap();
    assert!(candidates.is_empty());
    assert!(source.fetcher.searches().is_empty());
    assert_eq!(source.access.requested(), 0);
}

#[tokio::test]
async fn the_recorded_page_resolves_domains_and_counts_skips() {
    let source = source_with(&ok_tokens(), |_, _| response(200, &[], SEARCH), 5);
    let (candidates, stats) = source.collect_with_stats(&one_profile()).await.unwrap();
    assert_eq!(stats.records, 6);
    assert_eq!(stats.candidates, 3);
    assert_eq!(stats.skipped_no_domain, 2);
    assert_eq!(stats.skipped_malformed, 1);
    let domains: Vec<String> = candidates.iter().map(|c| c.domain.to_string()).collect();
    assert_eq!(
        domains,
        ["acme-identity.fr", "nordsecure.fr", "aero-systems.fr"]
    );
}

#[test]
fn an_offer_becomes_a_job_offer_candidate() {
    let page: Value = serde_json::from_str(SEARCH).unwrap();
    let candidate = parse_offer(&page["resultats"][0]).unwrap();
    assert_eq!(candidate.source, SourceName::FranceTravail);
    assert_eq!(candidate.kind.as_str(), "job_offer");
    assert_eq!(candidate.title, "Ing\u{e9}nieur IAM Keycloak H/F");
    assert_eq!(
        candidate.provenance.as_str(),
        "https://candidat.francetravail.fr/offres/recherche/detail/184TKRB"
    );
    assert_eq!(
        candidate.observed_at,
        Utc.with_ymd_and_hms(2026, 10, 2, 8, 12, 33).unwrap()
    );
    assert!(
        candidate
            .text
            .contains("migration de notre annuaire vers Keycloak")
    );
    assert!(
        candidate
            .text
            .contains("\u{c9}tudes et d\u{e9}veloppement informatique")
    );
    assert!(candidate.text.contains("paiement en ligne"));
    assert_eq!(
        candidate.details,
        json!({
            "offer_id": "184TKRB",
            "company": "ACME IDENTITY",
            "contract": "CDI",
            "location": "69 - LYON 03",
            "published": "2026-10-02T08:12:33.000Z",
            "rome": "M1805",
        })
    );
}

#[test]
fn the_employer_url_wins_then_the_contact_email() {
    let base = |company: Value, contact: Value| {
        json!({
            "id": "X1", "intitule": "t", "dateCreation": "2026-10-02T08:00:00Z",
            "entreprise": company, "contact": contact
        })
    };
    let both = base(
        json!({"url": "https://www.acme.fr"}),
        json!({"courriel": "rh@other.fr"}),
    );
    assert_eq!(parse_offer(&both).unwrap().domain.as_str(), "acme.fr");
    let board_then_mail = base(
        json!({"url": "https://fr.linkedin.com/company/acme"}),
        json!({"courriel": "rh@other.fr"}),
    );
    assert_eq!(
        parse_offer(&board_then_mail).unwrap().domain.as_str(),
        "other.fr"
    );
    let webmail = base(json!({}), json!({"courriel": "x@outlook.fr"}));
    assert_eq!(parse_offer(&webmail).unwrap_err(), Skip::NoDomain);
    let agency = base(
        json!({"url": "https://www.adecco.fr"}),
        json!({"courriel": "x@manpower.fr"}),
    );
    assert_eq!(parse_offer(&agency).unwrap_err(), Skip::NoDomain);
    let blank = base(json!({"url": ""}), json!({}));
    assert_eq!(parse_offer(&blank).unwrap_err(), Skip::NoDomain);
}

#[test]
fn malformed_offers_are_rejected() {
    let missing_date = json!({"id": "X", "intitule": "t", "entreprise": {"url": "https://a.fr"}});
    assert_eq!(parse_offer(&missing_date).unwrap_err(), Skip::Malformed);
    let bad_date = json!({"id": "X", "intitule": "t", "dateCreation": "yesterday"});
    assert_eq!(parse_offer(&bad_date).unwrap_err(), Skip::Malformed);
    let no_title = json!({"id": "X", "dateCreation": "2026-10-02T08:00:00Z"});
    assert_eq!(parse_offer(&no_title).unwrap_err(), Skip::Malformed);
}

#[test]
fn the_text_is_bounded() {
    let offer = json!({
        "id": "X", "intitule": "t", "dateCreation": "2026-10-02T08:00:00Z",
        "description": "d".repeat(10_000),
        "entreprise": {"url": "https://acme.fr", "description": "e".repeat(10_000)}
    });
    assert_eq!(
        parse_offer(&offer).unwrap().text.chars().count(),
        TEXT_MAX_CHARS
    );
}
