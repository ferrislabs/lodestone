use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, TimeZone, Utc};
use lodestone_core::{
    ActiveProfile, FetchError, FetchPurpose, FetchRequest, FetchResponse, Fetcher, Profile,
    ProfileId, ProfileName, ProfileRevision, RevisionNumber, SourceName, ValidatedProfile,
    definition::{AgentId, SearchAgent},
};
use serde_json::{Value, json};
use url::Url;
use uuid::Uuid;

use super::*;

const DATASET: &str = include_str!("../../tests/fixtures/decp/dataset.json");
const APRIL: &str = include_str!("../../tests/fixtures/decp/decp-2026-04.json");
const MARCH: &str = include_str!("../../tests/fixtures/decp/decp-2026-03.json");

const ANNUAIRE: [(&str, &str); 4] = [
    (
        "20004217400090",
        include_str!("../../tests/fixtures/decp/annuaire-20004217400090.json"),
    ),
    (
        "21560121200016",
        include_str!("../../tests/fixtures/decp/annuaire-21560121200016.json"),
    ),
    (
        "22350001800013",
        include_str!("../../tests/fixtures/decp/annuaire-22350001800013.json"),
    ),
    (
        "25351449100047",
        include_str!("../../tests/fixtures/decp/annuaire-25351449100047.json"),
    ),
];

const ENTREPRISES: [(&str, &str); 6] = [
    (
        "20004217400090",
        r#"{"results":[{"nom_complet":"LORIENT AGGLOMERATION"}]}"#,
    ),
    (
        "21560121200016",
        include_str!("../../tests/fixtures/decp/entreprises-21560121200016.json"),
    ),
    (
        "22350001800013",
        include_str!("../../tests/fixtures/decp/entreprises-22350001800013.json"),
    ),
    (
        "83177947500027",
        include_str!("../../tests/fixtures/decp/entreprises-83177947500027.json"),
    ),
    (
        "34927581800061",
        include_str!("../../tests/fixtures/decp/entreprises-34927581800061.json"),
    ),
    (
        "81378068100025",
        include_str!("../../tests/fixtures/decp/entreprises-81378068100025.json"),
    ),
];

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 20, 12, 0, 0).unwrap()
}

fn settings() -> DecpSettings {
    DecpSettings {
        dataset_api_url: Url::parse(DATASET_API_URL).unwrap().into(),
        annuaire_url: Url::parse(ANNUAIRE_URL).unwrap(),
        entreprises_url: Url::parse(ENTREPRISES_URL).unwrap().into(),
        lookback_days: DEFAULT_LOOKBACK_DAYS,
        notified_within_days: 31,
        max_body_bytes: 1024 * 1024,
        max_rate_limit_wait: Duration::from_secs(60),
    }
}

fn profile(decp: Option<Value>) -> ActiveProfile {
    let mut sources = json!({
        "dns_cname": {"patterns": [{"provider": "auth0", "target_suffix": "auth0.com"}]}
    });
    if let Some(rules) = decp {
        sources["decp"] = rules;
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
    let epoch = now();
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
            definition: ValidatedProfile::parse(value).unwrap(),
            created_by: None,
            created_at: epoch,
        },
    }
}

fn iam() -> ActiveProfile {
    profile(Some(json!({
        "keywords": ["SSO", "annuaire", "authentification", "MFA"],
        "exclusions": ["annuaire téléphonique"]
    })))
}

struct FixtureFetcher {
    files: HashMap<String, String>,
    dataset: String,
    requests: Mutex<Vec<Url>>,
    rate_limited_once: Mutex<bool>,
}

impl FixtureFetcher {
    fn new() -> Self {
        let mut files = HashMap::new();
        files.insert("decp-2026-04.json".to_owned(), APRIL.to_owned());
        files.insert("decp-2026-03.json".to_owned(), MARCH.to_owned());
        Self {
            files,
            dataset: DATASET.to_owned(),
            requests: Mutex::new(Vec::new()),
            rate_limited_once: Mutex::new(false),
        }
    }

    fn count(&self, host: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|url| url.host_str() == Some(host))
            .count()
    }

    fn param(url: &Url, name: &str) -> String {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .unwrap_or_default()
    }
}

fn ok(body: &str) -> Result<FetchResponse, FetchError> {
    Ok(FetchResponse {
        status: 200,
        headers: Vec::new(),
        body: body.as_bytes().to_vec(),
        final_url: Url::parse("https://example.test/").unwrap(),
        truncated: false,
    })
}

impl Fetcher for FixtureFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        assert_eq!(request.purpose, FetchPurpose::Api);
        assert_eq!(request.source, SourceName::Decp);
        self.requests.lock().unwrap().push(request.url.clone());
        let url = &request.url;
        match url.host_str().unwrap() {
            "www.data.gouv.fr" => ok(&self.dataset),
            "static.data.gouv.fr" => {
                let name = url.path().rsplit('/').next().unwrap();
                match self.files.get(name) {
                    Some(body) => ok(body),
                    None => Ok(FetchResponse {
                        status: 404,
                        headers: Vec::new(),
                        body: Vec::new(),
                        final_url: url.clone(),
                        truncated: false,
                    }),
                }
            }
            "api-lannuaire.service-public.fr" => {
                let clause = Self::param(url, "where");
                let siret = clause.trim_start_matches("siret=\"").trim_end_matches('"');
                let body = ANNUAIRE
                    .iter()
                    .find(|(known, _)| *known == siret)
                    .map_or(r#"{"total_count":0,"results":[]}"#, |(_, body)| body);
                ok(body)
            }
            "recherche-entreprises.api.gouv.fr" => {
                let siret = Self::param(url, "q");
                let body = ENTREPRISES
                    .iter()
                    .find(|(known, _)| *known == siret)
                    .map_or(r#"{"results":[]}"#, |(_, body)| body);
                let mut limited = self.rate_limited_once.lock().unwrap();
                if siret == "83177947500027" && !*limited {
                    *limited = true;
                    return Err(FetchError::RateLimited {
                        retry_after: Duration::from_millis(1),
                    });
                }
                ok(body)
            }
            other => panic!("unexpected host {other}"),
        }
    }
}

fn source(fetcher: FixtureFetcher) -> (DecpSource<FixtureFetcher>, Arc<FixtureFetcher>) {
    let fetcher = Arc::new(fetcher);
    (DecpSource::new(Arc::clone(&fetcher), settings()), fetcher)
}

#[tokio::test]
async fn the_window_spans_both_months_and_lists_the_candidates() {
    let (source, _) = source(FixtureFetcher::new());
    let (candidates, stats) = source.collect_with_stats(&[iam()], now()).await.unwrap();
    assert_eq!(stats.files_used, ["decp-2026-04.json", "decp-2026-03.json"]);
    assert_eq!(stats.files, 2);
    assert_eq!(stats.contracts, 11);
    assert_eq!(stats.in_window, 8);
    assert_eq!(stats.matched, 5);
    assert_eq!(stats.duplicates, 1);
    assert_eq!(stats.skipped_malformed, 1);
    assert_eq!(stats.unresolved_buyers, 1);
    assert_eq!(stats.candidates, 3);
    let mut domains: Vec<String> = candidates
        .iter()
        .map(|candidate| candidate.domain.to_string())
        .collect();
    domains.sort();
    assert_eq!(
        domains,
        ["ille-et-vilaine.fr", "lorient-agglo.bzh", "lorient.bzh"]
    );
}

#[tokio::test]
async fn a_candidate_carries_the_contract_details() {
    let (source, _) = source(FixtureFetcher::new());
    let (candidates, _) = source.collect_with_stats(&[iam()], now()).await.unwrap();
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.domain.as_str() == "ille-et-vilaine.fr")
        .unwrap();
    assert_eq!(candidate.source, SourceName::Decp);
    assert_eq!(candidate.kind.as_str(), "awarded_contract");
    assert_eq!(
        candidate.title,
        "Mise en place d'un annuaire d'entreprise et d'une authentification unique SSO pour les agents"
    );
    assert!(candidate.text.contains("CPV: 72200000"));
    assert!(candidate.text.contains("Procédure: Procédure adaptée"));
    assert_eq!(
        candidate.provenance.as_str(),
        "https://www.data.gouv.fr/fr/datasets/r/res-2026-04#22350001800013-2026S09001"
    );
    assert_eq!(
        candidate.observed_at,
        Utc.with_ymd_and_hms(2026, 4, 2, 0, 0, 0).unwrap()
    );
    let details = &candidate.details;
    assert_eq!(details["contract_id"], "2026S09001");
    assert_eq!(details["buyer_siret"], "22350001800013");
    assert_eq!(details["buyer_name"], "DEPARTEMENT D ILLE ET VILAINE");
    assert_eq!(details["amount"], 85000.0);
    assert_eq!(details["notified_on"], "2026-04-02");
    assert_eq!(details["cpv"], "72200000");
    assert_eq!(
        details["holders"],
        json!([
            {"siret": "83177947500027", "name": "LIKSI RENNES"},
            {"siret": "34927581800061", "name": "AGYSOFT"}
        ])
    );
}

#[tokio::test]
async fn the_newest_file_wins_a_duplicated_contract() {
    let (source, _) = source(FixtureFetcher::new());
    let (candidates, _) = source.collect_with_stats(&[iam()], now()).await.unwrap();
    let department: Vec<_> = candidates
        .iter()
        .filter(|candidate| candidate.domain.as_str() == "ille-et-vilaine.fr")
        .collect();
    assert_eq!(department.len(), 1);
    assert_eq!(department[0].details["amount"], 85000.0);
}

#[tokio::test]
async fn contracts_outside_the_window_or_in_the_future_are_ignored() {
    let (source, _) = source(FixtureFetcher::new());
    let (candidates, _) = source.collect_with_stats(&[iam()], now()).await.unwrap();
    for candidate in &candidates {
        let contract = candidate.details["contract_id"].as_str().unwrap();
        assert_ne!(contract, "2026S09003");
        assert_ne!(contract, "2026S09006");
    }
    let awards = source.take_awards();
    assert!(awards.iter().all(|award| award.contract_id != "2026S09003"));
    assert!(awards.iter().all(|award| award.contract_id != "2026S09006"));
}

#[tokio::test]
async fn a_longer_notification_window_keeps_older_contracts_from_the_same_files() {
    let (_, fetcher) = source(FixtureFetcher::new());
    let mut settings = settings();
    settings.notified_within_days = 60;
    let wide = DecpSource::new(Arc::clone(&fetcher), settings);
    let (candidates, stats) = wide.collect_with_stats(&[iam()], now()).await.unwrap();
    assert_eq!(stats.files_used, ["decp-2026-04.json", "decp-2026-03.json"]);
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.details["contract_id"] == "2026S09006")
    );
}

#[tokio::test]
async fn an_unresolved_buyer_yields_no_candidate_but_keeps_the_awards() {
    let (source, _) = source(FixtureFetcher::new());
    let (candidates, stats) = source.collect_with_stats(&[iam()], now()).await.unwrap();
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.details["contract_id"] != "2026S04026")
    );
    assert_eq!(stats.unresolved_buyers, 1);
    let sso = source
        .take_awards()
        .into_iter()
        .find(|award| award.contract_id == "2026S04026")
        .unwrap();
    assert_eq!(sso.buyer.as_str(), "25351449100047");
    assert_eq!(sso.holders.len(), 1);
    assert_eq!(sso.holders[0].siret.as_str(), "83177947500027");
    assert_eq!(sso.holders[0].name.as_deref(), Some("LIKSI RENNES"));
    assert_eq!(sso.amount, Some(120000.0));
}

#[tokio::test]
async fn holders_of_other_kinds_are_ignored() {
    let (source, _) = source(FixtureFetcher::new());
    source.collect_with_stats(&[iam()], now()).await.unwrap();
    let awards = source.take_awards();
    let department = awards
        .iter()
        .find(|award| award.contract_id == "2026S09001")
        .unwrap();
    assert_eq!(department.holders.len(), 2);
}

#[tokio::test]
async fn lookups_are_cached_for_the_run() {
    let (source, fetcher) = source(FixtureFetcher::new());
    let (_, stats) = source.collect_with_stats(&[iam()], now()).await.unwrap();
    assert_eq!(fetcher.count("api-lannuaire.service-public.fr"), 4);
    assert_eq!(fetcher.count("recherche-entreprises.api.gouv.fr"), 7);
    assert_eq!(stats.lookups, 10);
}

#[tokio::test(start_paused = true)]
async fn a_rate_limited_lookup_is_retried() {
    let (source, _) = source(FixtureFetcher::new());
    source.collect_with_stats(&[iam()], now()).await.unwrap();
    let awards = source.take_awards();
    let sso = awards
        .iter()
        .find(|award| award.contract_id == "2026S04026")
        .unwrap();
    assert_eq!(sso.holders[0].name.as_deref(), Some("LIKSI RENNES"));
}

#[tokio::test]
async fn partner_awards_follow_the_routing_of_each_profile() {
    let (source, _) = source(FixtureFetcher::new());
    source.collect_with_stats(&[iam()], now()).await.unwrap();
    let contracts = source.take_awards();
    let rules = SearchAgent {
        accept_min_keywords: 1,
        ..SearchAgent::new(AgentId::new("mfa"), "MFA", &["decp"], &["MFA"], &[])
    };
    let awards = partner_awards(&rules, &contracts);
    assert_eq!(awards.len(), 1);
    assert_eq!(awards[0].holder.as_str(), "81378068100025");
    assert_eq!(awards[0].matched, ["MFA"]);
    assert_eq!(awards[0].contract_id, "20004217400090-2026S09005");
    assert_eq!(
        awards[0].holder_name.as_deref(),
        Some("SOCIETE PUBLIQUE LOCALE CONSTRUCTION PUBLIQUE D'ILLE ET VILAINE")
    );
}

#[tokio::test]
async fn ambiguous_contracts_still_produce_partner_awards() {
    let (source, _) = source(FixtureFetcher::new());
    source.collect_with_stats(&[iam()], now()).await.unwrap();
    let contracts = source.take_awards();
    let rules = iam()
        .revision
        .definition
        .definition()
        .enabled_search_agents(SourceName::Decp)
        .next()
        .cloned()
        .unwrap();
    let awards = partner_awards(&rules, &contracts);
    let standard: Vec<_> = awards
        .iter()
        .filter(|award| award.contract_id == "21560121200016-2026S09002")
        .collect();
    assert_eq!(standard.len(), 1);
    assert_eq!(awards.len(), 5);
}

#[tokio::test]
async fn take_awards_drains_the_previous_run() {
    let (source, _) = source(FixtureFetcher::new());
    source.collect_with_stats(&[iam()], now()).await.unwrap();
    assert_eq!(source.take_awards().len(), 4);
    assert!(source.take_awards().is_empty());
}

#[tokio::test]
async fn profiles_without_decp_rules_fetch_nothing() {
    let (source, fetcher) = source(FixtureFetcher::new());
    let (candidates, stats) = source
        .collect_with_stats(&[profile(None)], now())
        .await
        .unwrap();
    assert!(candidates.is_empty());
    assert_eq!(stats, DecpStats::default());
    assert!(fetcher.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_short_lookback_reads_only_the_current_month() {
    let (source, fetcher) = source(FixtureFetcher::new());
    let mut settings = settings();
    settings.lookback_days = 5;
    let short = DecpSource::new(Arc::clone(&fetcher), settings);
    let (_, stats) = short.collect_with_stats(&[iam()], now()).await.unwrap();
    assert_eq!(stats.files_used, ["decp-2026-04.json"]);
    drop(source);
}

#[tokio::test]
async fn a_missing_current_month_still_reads_the_previous_one() {
    let mut fetcher = FixtureFetcher::new();
    let mut dataset: Value = serde_json::from_str(DATASET).unwrap();
    dataset["resources"]
        .as_array_mut()
        .unwrap()
        .retain(|resource| resource["title"] != "decp-2026-04.json");
    fetcher.dataset = dataset.to_string();
    let (source, _) = source(fetcher);
    let (_, stats) = source.collect_with_stats(&[iam()], now()).await.unwrap();
    assert_eq!(stats.files_used, ["decp-2026-03.json"]);
}

#[tokio::test]
async fn no_published_file_is_an_error() {
    let mut fetcher = FixtureFetcher::new();
    fetcher.dataset = json!({"resources": []}).to_string();
    let (source, _) = source(fetcher);
    assert!(matches!(
        source.collect_with_stats(&[iam()], now()).await,
        Err(SourceError::Upstream(_))
    ));
}

#[tokio::test]
async fn an_oversized_file_is_refused_before_download() {
    let mut fetcher = FixtureFetcher::new();
    let mut dataset: Value = serde_json::from_str(DATASET).unwrap();
    dataset["resources"][0]["filesize"] = json!(10_u64 * 1024 * 1024);
    fetcher.dataset = dataset.to_string();
    let (source, fetcher) = source(fetcher);
    assert!(matches!(
        source.collect_with_stats(&[iam()], now()).await,
        Err(SourceError::Upstream(_))
    ));
    assert_eq!(fetcher.count("static.data.gouv.fr"), 0);
}

#[tokio::test]
async fn a_failed_download_is_an_error() {
    let mut fetcher = FixtureFetcher::new();
    fetcher.files.remove("decp-2026-04.json");
    let (source, _) = source(fetcher);
    assert!(matches!(
        source.collect_with_stats(&[iam()], now()).await,
        Err(SourceError::Upstream(_))
    ));
}

#[tokio::test]
async fn a_corrupt_file_is_an_error() {
    let mut fetcher = FixtureFetcher::new();
    fetcher
        .files
        .insert("decp-2026-04.json".to_owned(), "{not json".to_owned());
    let (source, _) = source(fetcher);
    assert!(matches!(
        source.collect_with_stats(&[iam()], now()).await,
        Err(SourceError::Upstream(_))
    ));
}

#[tokio::test]
async fn odd_field_types_do_not_break_the_file() {
    let mut fetcher = FixtureFetcher::new();
    let odd = json!({"marches": {"marche": [
        {
            "id": 42,
            "acheteur": {"id": "22350001800013"},
            "objet": "Mise en place d'un SSO et d'un annuaire",
            "dateNotification": "2026-04-03",
            "montant": "not a number",
            "titulaires": {"unexpected": true}
        },
        {"id": "x", "acheteur": null, "objet": null, "dateNotification": null, "titulaires": null}
    ]}});
    fetcher
        .files
        .insert("decp-2026-04.json".to_owned(), odd.to_string());
    fetcher.files.insert(
        "decp-2026-03.json".to_owned(),
        r#"{"marches":{}}"#.to_owned(),
    );
    let (source, _) = source(fetcher);
    let (candidates, stats) = source.collect_with_stats(&[iam()], now()).await.unwrap();
    assert_eq!(stats.candidates, 1);
    assert_eq!(stats.skipped_malformed, 1);
    assert_eq!(candidates[0].details["contract_id"], "42");
    assert_eq!(candidates[0].details["amount"], Value::Null);
    assert_eq!(candidates[0].details["holders"], json!([]));
}

#[test]
fn months_are_walked_newest_first_across_a_year_boundary() {
    let since = NaiveDate::from_ymd_opt(2025, 11, 20).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
    assert_eq!(
        months_newest_first(since, today),
        [(2026, 1), (2025, 12), (2025, 11)]
    );
    assert_eq!(resource_title(2026, 1), "decp-2026-01.json");
}

#[test]
fn the_candidate_provenance_parses() {
    assert!(Provenance::parse("https://www.data.gouv.fr/fr/datasets/r/abc#123-A 1/é").is_ok());
}
