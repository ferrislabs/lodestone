use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, TimeZone, Utc};
use common::CoreError;
use lodestone_core::{
    ActiveProfile, CandidateHost, CandidateHosts, CtCursors, FetchError, FetchRequest,
    FetchResponse, Fetcher, Hostname, Profile, ProfileId, ProfileName, ProfileRevision,
    RevisionNumber, Source, SourceName, ValidatedProfile,
};
use serde_json::json;
use url::Url;
use uuid::Uuid;

use super::*;

const TILE: &[u8] = include_bytes!("../../../ct/tests/fixtures/le-sycamore2026h2-tile-3874719.bin");
const MONITORING: &str = "https://mon.example.test/2026h2/";

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

fn agent_of(profile: &ActiveProfile, kind: SourceName) -> Agent {
    profile
        .revision
        .definition
        .definition()
        .agents
        .iter()
        .find(|agent| agent.serves(kind))
        .cloned()
        .unwrap()
}

fn labels(values: &[&str]) -> HashSet<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn profile(ct_labels: Option<&[&str]>) -> ActiveProfile {
    let mut sources = json!({
        "dns_cname": {"patterns": [{"provider": "auth0", "target_suffix": "auth0.com"}]}
    });
    if let Some(ct_labels) = ct_labels {
        sources["certificate_transparency"] = json!({ "labels": ct_labels });
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

fn log_list() -> Vec<u8> {
    json!({
        "version": "1",
        "operators": [{
            "name": "Test",
            "email": [],
            "logs": [],
            "tiled_logs": [{
                "description": "Test Tiled 2026h2",
                "log_id": "dGVzdA==",
                "key": "AAAA",
                "submission_url": "https://sub.example.test/",
                "monitoring_url": MONITORING,
                "mmd": 60,
                "state": {"usable": {"timestamp": "2020-01-01T00:00:00Z"}},
                "temporal_interval": {
                    "start_inclusive": "2000-01-01T00:00:00Z",
                    "end_exclusive": "2100-01-01T00:00:00Z"
                }
            }]
        }]
    })
    .to_string()
    .into_bytes()
}

struct FakeFetcher {
    tree_size: u64,
    requests: Mutex<Vec<String>>,
}

impl FakeFetcher {
    fn new(tree_size: u64) -> Self {
        Self {
            tree_size,
            requests: Mutex::new(Vec::new()),
        }
    }

    fn tile_requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|url| url.contains("/tile/data/"))
            .cloned()
            .collect()
    }
}

impl Fetcher for FakeFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let url = request.url.to_string();
        self.requests.lock().unwrap().push(url.clone());
        let body = if url.ends_with("log_list.json") {
            log_list()
        } else if url.ends_with("/checkpoint") {
            format!("mon.example.test/2026h2\n{}\nroot\n", self.tree_size).into_bytes()
        } else if url.contains("/tile/data/") {
            TILE.to_vec()
        } else {
            return Ok(FetchResponse {
                status: 404,
                headers: Vec::new(),
                body: Vec::new(),
                final_url: request.url.clone(),
                truncated: false,
            });
        };
        Ok(FetchResponse {
            status: 200,
            headers: Vec::new(),
            body,
            final_url: request.url.clone(),
            truncated: false,
        })
    }
}

#[derive(Default)]
struct FakeHosts {
    upserted: Mutex<Vec<CandidateHost>>,
}

impl CandidateHosts for FakeHosts {
    async fn upsert(&self, hosts: &[CandidateHost]) -> Result<u64, CoreError> {
        self.upserted.lock().unwrap().extend_from_slice(hosts);
        Ok(hosts.len() as u64)
    }

    async fn list_due(
        &self,
        _source: SourceName,
        _checked_before: DateTime<Utc>,
        _limit: u32,
    ) -> Result<Vec<CandidateHost>, CoreError> {
        Ok(Vec::new())
    }

    async fn mark_checked(
        &self,
        _hosts: &[Hostname],
        _source: SourceName,
        _at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        Ok(())
    }
}

#[derive(Default)]
struct FakeCursors {
    cursors: Mutex<HashMap<String, u64>>,
}

impl CtCursors for FakeCursors {
    async fn next_index(&self, log_id: &str) -> Result<Option<u64>, CoreError> {
        Ok(self.cursors.lock().unwrap().get(log_id).copied())
    }

    async fn advance(
        &self,
        log_id: &str,
        next_index: u64,
        _at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut cursors = self.cursors.lock().unwrap();
        let entry = cursors.entry(log_id.to_owned()).or_insert(0);
        *entry = (*entry).max(next_index);
        Ok(())
    }
}

fn settings(tiles_per_log: u64, max_lag_tiles: u64) -> CtSettings {
    CtSettings {
        log_list_url: Url::parse("https://lists.example.test/log_list.json")
            .unwrap()
            .into(),
        log_list_ttl: Duration::from_secs(3600),
        max_logs: 4,
        tiles_per_log,
        max_lag_tiles,
    }
}

type TestSource = CtSource<FakeFetcher, FakeHosts, FakeCursors>;

fn source(tree_size: u64, cursor: Option<u64>, tiles_per_log: u64, max_lag: u64) -> TestSource {
    let cursors = FakeCursors::default();
    if let Some(cursor) = cursor {
        cursors
            .cursors
            .lock()
            .unwrap()
            .insert("dGVzdA==".to_owned(), cursor);
    }
    CtSource::new(
        Arc::new(FakeFetcher::new(tree_size)),
        Arc::new(FakeHosts::default()),
        Arc::new(cursors),
        settings(tiles_per_log, max_lag),
    )
}

fn cursor(source: &TestSource) -> Option<u64> {
    source
        .cursors
        .cursors
        .lock()
        .unwrap()
        .get("dGVzdA==")
        .copied()
}

#[test]
fn only_subdomain_labels_match() {
    let wanted = labels(&["sso", "auth", "id"]);
    let host = |name: &str| matching_host(name, &wanted).map(|h| h.to_string());
    assert_eq!(host("sso.example.com").as_deref(), Some("sso.example.com"));
    assert_eq!(
        host("login.auth.example.co.uk").as_deref(),
        Some("login.auth.example.co.uk")
    );
    assert_eq!(
        host("*.auth.example.com").as_deref(),
        Some("auth.example.com")
    );
    assert_eq!(host("id.example.fr").as_deref(), Some("id.example.fr"));
    assert_eq!(host("example.com"), None);
    assert_eq!(host("shop.example.id"), None);
    assert_eq!(host("notsso.example.com"), None);
    assert_eq!(host("*.example.com"), None);
    assert_eq!(host("sso.corp.internal"), None);
}

#[test]
fn label_filter_is_the_union_of_profile_labels() {
    let filter = label_filter(&[
        profile(Some(&["sso", "auth"])),
        profile(Some(&["auth", "idp"])),
        profile(None),
    ]);
    assert_eq!(filter, labels(&["sso", "auth", "idp"]));
}

#[tokio::test]
async fn profiles_without_ct_labels_fetch_nothing() {
    let source = source(10 * 256, None, 4, 64);
    let hosts = source.collect(&[profile(None)], at(0)).await.unwrap();
    assert!(hosts.is_empty());
    assert!(source.fetcher.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn first_run_starts_at_the_current_tree_size() {
    let source = source(10 * 256 + 17, None, 4, 64);
    let hosts = source
        .collect(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    assert!(hosts.is_empty());
    assert!(source.fetcher.tile_requests().is_empty());
    assert_eq!(cursor(&source), Some(10 * 256));
}

#[tokio::test]
async fn later_runs_read_full_tiles_within_budget_and_store_matches() {
    let wanted = ["sso", "auth", "login", "id", "www", "mail", "api"];
    let source = source(10 * 256 + 17, Some(4 * 256), 3, 64);
    let hosts = source
        .collect(&[profile(Some(&wanted))], at(0))
        .await
        .unwrap();

    assert_eq!(source.fetcher.tile_requests().len(), 3);
    assert_eq!(cursor(&source), Some(7 * 256));
    assert_eq!(
        source.last_pass(),
        crate::ct::CtPass {
            tiles_read: 3,
            lag_tiles: 3
        }
    );

    let filter = labels(&wanted);
    let entries = ct::parse_data_tile(TILE, 0).unwrap();
    let expected: BTreeSet<String> = entries
        .iter()
        .flat_map(|entry| entry.dns_names.iter())
        .filter_map(|name| matching_host(name, &filter))
        .map(|host| host.to_string())
        .collect();
    let found: BTreeSet<String> = hosts.iter().map(|host| host.host.to_string()).collect();
    assert_eq!(found, expected);
    assert!(
        hosts
            .iter()
            .all(|host| host.origin == HostOrigin::CertificateTransparency)
    );
    assert_eq!(source.hosts.upserted.lock().unwrap().len(), hosts.len());
}

#[tokio::test]
async fn cursor_stops_at_the_last_full_tile() {
    let source = source(10 * 256 + 17, Some(9 * 256), 8, 64);
    source
        .collect(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    assert_eq!(source.fetcher.tile_requests().len(), 1);
    assert_eq!(cursor(&source), Some(10 * 256));
}

#[tokio::test]
async fn a_log_too_far_behind_skips_ahead() {
    let source = source(1000 * 256, Some(0), 2, 10);
    source
        .collect(&[profile(Some(&["sso"]))], at(0))
        .await
        .unwrap();
    let requests = source.fetcher.tile_requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].ends_with("/tile/data/990"), "{}", requests[0]);
    assert_eq!(cursor(&source), Some(992 * 256));
    assert_eq!(source.last_pass().lag_tiles, 8);
}

#[tokio::test]
async fn evaluate_never_creates_signals() {
    let source = source(256, None, 1, 1);
    let host = CandidateHost::discovered(
        Hostname::parse("sso.example.com").unwrap(),
        HostOrigin::CertificateTransparency,
        at(0),
    )
    .unwrap();
    let profile = profile(Some(&["sso"]));
    let agent = agent_of(&profile, SourceName::CertificateTransparency);
    assert!(source.evaluate(&host, &profile, &agent).is_empty());
}

fn ct_agents_profile(agents: &[(&str, &[&str])]) -> ActiveProfile {
    let mut active = profile(None);
    let value = json!({
        "schema_version": 2,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "agents": agents
            .iter()
            .map(|(id, labels)| json!({
                "id": id,
                "name": id,
                "source": {"kind": "certificate_transparency", "labels": labels}
            }))
            .collect::<Vec<_>>(),
        "scoring": {"factors": [{"signal": "saas_provider", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
    });
    active.revision.definition = ValidatedProfile::parse(value).unwrap();
    active
}

#[test]
fn label_filter_unions_the_labels_of_every_enabled_agent() {
    let profile = ct_agents_profile(&[("ct-sso", &["sso"]), ("ct-auth", &["auth", "login"])]);
    assert_eq!(label_filter(&[profile]), labels(&["sso", "auth", "login"]));
}

#[test]
fn a_matched_host_is_attributed_to_the_agents_whose_labels_match() {
    let source = source(256, None, 1, 1);
    let profile = ct_agents_profile(&[("ct-sso", &["sso"]), ("ct-auth", &["auth"])]);
    let agents = &profile.revision.definition.definition().agents;
    let sso_host = CandidateHost::discovered(
        Hostname::parse("sso.example.com").unwrap(),
        HostOrigin::CertificateTransparency,
        at(0),
    )
    .unwrap();
    let auth_host = CandidateHost::discovered(
        Hostname::parse("auth.example.com").unwrap(),
        HostOrigin::CertificateTransparency,
        at(0),
    )
    .unwrap();
    assert!(source.serves(&sso_host, &agents[0]));
    assert!(!source.serves(&sso_host, &agents[1]));
    assert!(source.serves(&auth_host, &agents[1]));
    assert!(!source.serves(&auth_host, &agents[0]));
}

#[test]
fn a_disabled_ct_agent_adds_no_label() {
    let mut profile = ct_agents_profile(&[("ct-sso", &["sso"]), ("ct-auth", &["auth"])]);
    let mut value = serde_json::to_value(profile.revision.definition.definition()).unwrap();
    value["agents"][1]["enabled"] = json!(false);
    profile.revision.definition = ValidatedProfile::parse(value).unwrap();
    assert_eq!(label_filter(&[profile]), labels(&["sso"]));
}
