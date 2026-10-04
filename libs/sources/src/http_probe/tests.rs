use std::sync::atomic::AtomicUsize;

use chrono::TimeZone;
use lodestone_core::{HostOrigin, Profile, ProfileName, ProfileRevision, ValidatedProfile};
use serde_json::Value;
use uuid::Uuid;

use super::*;

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

fn host(name: &str) -> Hostname {
    Hostname::parse(name).unwrap()
}

fn candidate(name: &str) -> CandidateHost {
    CandidateHost::discovered(host(name), HostOrigin::Import, at(0)).unwrap()
}

fn profile(id: u128, revision: u32, probe: Value) -> ActiveProfile {
    let value = json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": { "http_probe": probe },
        "scoring": {"factors": [{"signal": "product_detected", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
    });
    let profile_id = ProfileId(Uuid::from_u128(id));
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
            number: RevisionNumber::new(revision).unwrap(),
            definition: ValidatedProfile::parse(value).unwrap(),
            created_by: None,
            created_at: at(0),
        },
    }
}

fn plain() -> Value {
    json!({"fingerprints": [{
        "id": "plain",
        "path": "/plain",
        "product": "plain",
        "matchers": [{"kind": "status", "equals": 200}]
    }]})
}

fn keycloak_profile() -> ActiveProfile {
    profile(
        1,
        1,
        json!({
            "fingerprints": [{
                "id": "keycloak-realm",
                "path": "/realms/master",
                "product": "keycloak",
                "matchers": [
                    {"kind": "status", "equals": 200},
                    {"kind": "body", "pattern": "\"realm\""}
                ]
            }]
        }),
    )
}

fn versioned_profile() -> ActiveProfile {
    profile(
        2,
        1,
        json!({
            "fingerprints": [{
                "id": "app",
                "path": "/version",
                "product": "app",
                "matchers": [{"kind": "status", "equals": 200}],
                "version": {"from": "header", "name": "X-Version", "pattern": "^v?([0-9.]+)$"},
                "debt_markers": [
                    {"id": "old", "rule": {"kind": "version_below", "version": "22.0"}},
                    {"id": "powered", "rule": {"kind": "matches", "matcher": {"kind": "header", "name": "x-powered-by", "pattern": "php"}}}
                ]
            }]
        }),
    )
}

fn response(status: u16, headers: &[(&str, &str)], body: &str) -> ProbedResponse {
    ProbedResponse {
        status,
        headers: headers
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
        body: body.to_owned(),
    }
}

fn probed_item(name: &str, path: &str, answer: Option<ProbedResponse>) -> ProbedHost {
    ProbedHost {
        host: candidate(name),
        responses: vec![ProbedPath {
            path: path.to_owned(),
            response: answer,
        }],
        probed_at: at(9),
    }
}

type Answer<'a> = Result<(u16, &'a str), FetchError>;

type Script = HashMap<String, Result<(u16, String), FetchError>>;

#[derive(Default)]
struct FakeFetcher {
    script: Script,
    requests: Mutex<Vec<(String, FetchPurpose, SourceName)>>,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
}

impl FakeFetcher {
    fn scripted(entries: &[(&str, Answer)]) -> Self {
        Self {
            script: entries
                .iter()
                .map(|(url, answer)| {
                    (
                        (*url).to_owned(),
                        answer
                            .clone()
                            .map(|(status, body)| (status, body.to_owned())),
                    )
                })
                .collect(),
            ..Self::default()
        }
    }
}

impl Fetcher for FakeFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_in_flight.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(20)).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push((
            request.url.to_string(),
            request.purpose,
            request.source,
        ));
        match self.script.get(request.url.as_str()) {
            Some(Ok((status, body))) => Ok(FetchResponse {
                status: *status,
                headers: Vec::new(),
                body: body.clone().into_bytes(),
                final_url: request.url.clone(),
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

type Marked = (Vec<String>, SourceName, DateTime<Utc>);

#[derive(Default)]
struct FakeHosts {
    due: Vec<CandidateHost>,
    marked: Mutex<Vec<Marked>>,
    list_args: Mutex<Vec<(SourceName, DateTime<Utc>, u32)>>,
    fail_list: bool,
    fail_mark: bool,
}

impl CandidateHosts for FakeHosts {
    async fn upsert(&self, hosts: &[CandidateHost]) -> Result<u64, CoreError> {
        Ok(hosts.len() as u64)
    }

    async fn list_due(
        &self,
        source: SourceName,
        checked_before: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<CandidateHost>, CoreError> {
        self.list_args
            .lock()
            .unwrap()
            .push((source, checked_before, limit));
        if self.fail_list {
            return Err(CoreError::Database("down".to_owned()));
        }
        Ok(self.due.clone())
    }

    async fn mark_checked(
        &self,
        hosts: &[Hostname],
        source: SourceName,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        if self.fail_mark {
            return Err(CoreError::Database("down".to_owned()));
        }
        self.marked.lock().unwrap().push((
            hosts.iter().map(|h| h.as_str().to_owned()).collect(),
            source,
            at,
        ));
        Ok(())
    }
}

#[derive(Default)]
struct FakeGuard {
    refused: Vec<String>,
    fail: bool,
    reserved: Mutex<Vec<(String, DateTime<Utc>, Duration)>>,
}

impl ProbeGuard for FakeGuard {
    async fn reserve(
        &self,
        host: &Hostname,
        now: DateTime<Utc>,
        revisit: Duration,
    ) -> Result<bool, CoreError> {
        if self.fail {
            return Err(CoreError::Database("down".to_owned()));
        }
        self.reserved
            .lock()
            .unwrap()
            .push((host.as_str().to_owned(), now, revisit));
        Ok(!self.refused.iter().any(|name| name == host.as_str()))
    }
}

fn settings(concurrency: usize) -> HttpProbeSettings {
    HttpProbeSettings {
        batch_size: 50,
        concurrency,
        revisit_interval: Duration::from_secs(3600),
        scheme: ProbeScheme::Https,
        port: None,
        host_override: None,
    }
}

type TestSource = HttpProbeSource<FakeFetcher, FakeHosts, FakeGuard>;

fn source(
    fetcher: FakeFetcher,
    hosts: FakeHosts,
    guard: FakeGuard,
    concurrency: usize,
) -> (TestSource, Arc<FakeFetcher>, Arc<FakeHosts>, Arc<FakeGuard>) {
    let fetcher = Arc::new(fetcher);
    let hosts = Arc::new(hosts);
    let guard = Arc::new(guard);
    (
        HttpProbeSource::new(
            Arc::clone(&fetcher),
            Arc::clone(&hosts),
            Arc::clone(&guard),
            settings(concurrency),
        ),
        fetcher,
        hosts,
        guard,
    )
}

fn evaluator() -> TestSource {
    source(
        FakeFetcher::default(),
        FakeHosts::default(),
        FakeGuard::default(),
        1,
    )
    .0
}

fn hosts_due(names: &[&str]) -> FakeHosts {
    FakeHosts {
        due: names.iter().map(|name| candidate(name)).collect(),
        ..FakeHosts::default()
    }
}

fn second_profile() -> ActiveProfile {
    profile(
        3,
        1,
        json!({
            "fingerprints": [
                {
                    "id": "keycloak-realm-2",
                    "path": "/realms/master",
                    "product": "keycloak",
                    "matchers": [{"kind": "status", "equals": 200}]
                },
                {
                    "id": "zitadel",
                    "path": "/ui/console",
                    "product": "zitadel",
                    "matchers": [{"kind": "status", "equals": 200}]
                }
            ]
        }),
    )
}

#[tokio::test]
async fn paths_are_the_sorted_union_fetched_once_per_host() {
    let (source, fetcher, hosts, _) = source(
        FakeFetcher::default(),
        hosts_due(&["sso.acme.com", "id.acme.org"]),
        FakeGuard::default(),
        1,
    );
    let items = source
        .collect(&[keycloak_profile(), second_profile()], at(100))
        .await
        .unwrap();
    assert_eq!(items.len(), 2);
    let requests = fetcher.requests.lock().unwrap();
    let urls: Vec<&str> = requests.iter().map(|(url, _, _)| url.as_str()).collect();
    assert_eq!(
        urls,
        vec![
            "https://sso.acme.com/realms/master",
            "https://sso.acme.com/ui/console",
            "https://id.acme.org/realms/master",
            "https://id.acme.org/ui/console",
        ]
    );
    assert!(requests.iter().all(
        |(_, purpose, name)| *purpose == FetchPurpose::Probe && *name == SourceName::HttpProbe
    ));
    let marked = hosts.marked.lock().unwrap();
    assert_eq!(marked.len(), 1);
    assert_eq!(marked[0].0, vec!["sso.acme.com", "id.acme.org"]);
    assert_eq!(marked[0].1, SourceName::HttpProbe);
    assert_eq!(marked[0].2, at(100));
    let args = hosts.list_args.lock().unwrap();
    assert_eq!(args[0], (SourceName::HttpProbe, at(100 - 3600), 50));
}

#[tokio::test]
async fn no_paths_means_no_listing() {
    let (source, _, hosts, _) = source(
        FakeFetcher::default(),
        hosts_due(&["sso.acme.com"]),
        FakeGuard::default(),
        1,
    );
    let empty = without_probe();
    assert!(source.collect(&[empty], at(0)).await.unwrap().is_empty());
    assert!(source.collect(&[], at(0)).await.unwrap().is_empty());
    assert!(hosts.list_args.lock().unwrap().is_empty());
}

#[tokio::test]
async fn guard_refusal_skips_the_host_unmarked() {
    let guard = FakeGuard {
        refused: vec!["sso.acme.com".to_owned()],
        ..FakeGuard::default()
    };
    let (source, fetcher, hosts, guard) = source(
        FakeFetcher::default(),
        hosts_due(&["sso.acme.com", "id.acme.org"]),
        guard,
        2,
    );
    let items = source
        .collect(&[keycloak_profile()], at(100))
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].host.host.as_str(), "id.acme.org");
    assert_eq!(fetcher.requests.lock().unwrap().len(), 1);
    assert_eq!(
        hosts.marked.lock().unwrap()[0].0,
        vec!["id.acme.org".to_owned()]
    );
    let reserved = guard.reserved.lock().unwrap();
    assert_eq!(reserved.len(), 2);
    assert!(
        reserved
            .iter()
            .all(|(_, now, revisit)| *now == at(100) && *revisit == Duration::from_secs(3600))
    );
}

#[tokio::test]
async fn rate_limit_drops_the_host_unmarked() {
    let limited = FakeFetcher::scripted(&[(
        "https://sso.acme.com/ui/console",
        Err(FetchError::RateLimited {
            retry_after: Duration::from_secs(5),
        }),
    )]);
    let (source, fetcher, hosts, _) = source(
        limited,
        hosts_due(&["sso.acme.com", "id.acme.org"]),
        FakeGuard::default(),
        1,
    );
    let items = source
        .collect(&[keycloak_profile(), second_profile()], at(100))
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].host.host.as_str(), "id.acme.org");
    assert_eq!(
        hosts.marked.lock().unwrap()[0].0,
        vec!["id.acme.org".to_owned()]
    );
    assert_eq!(fetcher.requests.lock().unwrap().len(), 4);
}

#[tokio::test]
async fn fetch_error_is_none_and_other_paths_still_evaluated() {
    let flaky = FakeFetcher::scripted(&[
        (
            "https://sso.acme.com/realms/master",
            Err(FetchError::Timeout),
        ),
        ("https://sso.acme.com/ui/console", Ok((200, "console"))),
    ]);
    let (source, _, _, _) = source(flaky, hosts_due(&["sso.acme.com"]), FakeGuard::default(), 1);
    let profiles = [keycloak_profile(), second_profile()];
    let items = source.collect(&profiles, at(100)).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].responses[0].path, "/realms/master");
    assert!(items[0].responses[0].response.is_none());
    assert_eq!(items[0].responses[1].response.as_ref().unwrap().status, 200);
    let found = source.evaluate(&items[0], &profiles[1]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].value.as_deref(), Some("zitadel"));
}

#[tokio::test]
async fn store_and_guard_errors_are_upstream() {
    let (source_a, _, _, _) = source(
        FakeFetcher::default(),
        FakeHosts {
            fail_list: true,
            ..hosts_due(&["sso.acme.com"])
        },
        FakeGuard::default(),
        1,
    );
    assert!(matches!(
        source_a.collect(&[keycloak_profile()], at(0)).await,
        Err(SourceError::Upstream(_))
    ));
    let (source_b, _, _, _) = source(
        FakeFetcher::default(),
        FakeHosts {
            fail_mark: true,
            ..hosts_due(&["sso.acme.com"])
        },
        FakeGuard::default(),
        1,
    );
    assert!(matches!(
        source_b.collect(&[keycloak_profile()], at(0)).await,
        Err(SourceError::Upstream(_))
    ));
    let guard = FakeGuard {
        fail: true,
        ..FakeGuard::default()
    };
    let (source_c, _, _, _) = source(
        FakeFetcher::default(),
        hosts_due(&["sso.acme.com"]),
        guard,
        1,
    );
    assert!(matches!(
        source_c.collect(&[keycloak_profile()], at(0)).await,
        Err(SourceError::Upstream(_))
    ));
}

#[tokio::test]
async fn concurrency_is_bounded() {
    let (source, fetcher, _, _) = source(
        FakeFetcher::default(),
        hosts_due(&[
            "a.acme.com",
            "b.acme.com",
            "c.acme.com",
            "d.acme.com",
            "e.acme.com",
            "f.acme.com",
        ]),
        FakeGuard::default(),
        2,
    );
    let items = source.collect(&[keycloak_profile()], at(0)).await.unwrap();
    assert_eq!(items.len(), 6);
    assert_eq!(fetcher.max_in_flight.load(Ordering::SeqCst), 2);
}

#[test]
fn body_is_truncated_lossily() {
    let mut body = vec![b'a'; MAX_BODY_BYTES + 10];
    body[0] = 0xff;
    let converted = super::probed(&FetchResponse {
        status: 200,
        headers: vec![("A".to_owned(), "b".to_owned())],
        body,
        final_url: Url::parse("https://x.example.com/").unwrap(),
        truncated: false,
    });
    assert_eq!(converted.body.chars().count(), MAX_BODY_BYTES);
    assert!(converted.body.starts_with('\u{fffd}'));
    assert_eq!(converted.headers, vec![("A".to_owned(), "b".to_owned())]);
}

#[test]
fn urls_use_scheme_port_and_override() {
    let context = |port, over: Option<&str>, scheme| ProbeContext {
        fetcher: Arc::new(FakeFetcher::default()),
        guard: Arc::new(FakeGuard::default()),
        settings: HttpProbeSettings {
            port,
            host_override: over.map(str::to_owned),
            scheme,
            ..settings(1)
        },
        paths: Arc::new(Vec::new()),
        permits: Arc::new(Semaphore::new(1)),
        now: at(0),
        revisit: Duration::from_secs(1),
    };
    let name = host("sso.acme.com");
    assert_eq!(
        context(None, None, ProbeScheme::Https)
            .url(&name, "/a")
            .unwrap()
            .as_str(),
        "https://sso.acme.com/a"
    );
    assert_eq!(
        context(Some(8080), None, ProbeScheme::Http)
            .url(&name, "/a")
            .unwrap()
            .as_str(),
        "http://sso.acme.com:8080/a"
    );
    assert_eq!(
        context(Some(1), Some("127.0.0.1:9"), ProbeScheme::Http)
            .url(&name, "/a")
            .unwrap()
            .as_str(),
        "http://127.0.0.1:9/a"
    );
}

#[test]
fn matchers_require_all_conditions() {
    let evaluator = evaluator();
    let profile = keycloak_profile();
    let hit = probed_item(
        "sso.acme.com",
        "/realms/master",
        Some(response(200, &[], "{\"realm\":\"m\"}")),
    );
    assert_eq!(evaluator.evaluate(&hit, &profile).len(), 1);
    for answer in [
        Some(response(404, &[], "{\"realm\":\"m\"}")),
        Some(response(200, &[], "{}")),
        None,
    ] {
        let miss = probed_item("sso.acme.com", "/realms/master", answer);
        assert!(evaluator.evaluate(&miss, &profile).is_empty());
    }
    let other_path = probed_item(
        "sso.acme.com",
        "/other",
        Some(response(200, &[], "{\"realm\":1}")),
    );
    assert!(evaluator.evaluate(&other_path, &profile).is_empty());
}

#[test]
fn observation_carries_evidence() {
    let found = evaluator().evaluate(
        &probed_item(
            "sso.acme.com",
            "/realms/master",
            Some(response(200, &[], "{\"realm\":\"m\"}")),
        ),
        &keycloak_profile(),
    );
    assert_eq!(found.len(), 1);
    let observation = &found[0];
    assert_eq!(observation.domain.as_str(), "acme.com");
    assert_eq!(observation.kind.as_str(), "product_detected");
    assert_eq!(observation.value.as_deref(), Some("keycloak"));
    assert_eq!(observation.strength.get(), 0.9);
    assert_eq!(observation.observed_at, at(9));
    assert_eq!(
        observation.provenance.as_str(),
        "https://sso.acme.com/realms/master"
    );
    assert_eq!(
        observation.raw,
        json!({
            "host": "sso.acme.com",
            "fingerprint": "keycloak-realm",
            "path": "/realms/master",
            "status": 200,
            "version": null
        })
    );
}

#[test]
fn header_matching_ignores_name_case() {
    let profile = profile(
        5,
        1,
        json!({"fingerprints": [{
            "id": "ak",
            "path": "/live",
            "product": "authentik",
            "matchers": [{"kind": "header", "name": "X-Powered-By", "pattern": "^authentik$"}]
        }]}),
    );
    let evaluator = evaluator();
    let hit = probed_item(
        "sso.acme.com",
        "/live",
        Some(response(200, &[("x-powered-by", "authentik")], "")),
    );
    assert_eq!(evaluator.evaluate(&hit, &profile).len(), 1);
    let miss = probed_item(
        "sso.acme.com",
        "/live",
        Some(response(200, &[("server", "authentik")], "")),
    );
    assert!(evaluator.evaluate(&miss, &profile).is_empty());
}

#[test]
fn version_comparison_pads_missing_parts() {
    assert!(version_below(&[21, 1, 2], &[22, 0]));
    assert!(!version_below(&[22, 0], &[21, 1, 2]));
    assert!(!version_below(&[22], &[22, 0, 0]));
    assert!(!version_below(&[22, 0, 0], &[22]));
    assert!(version_below(&[22], &[22, 0, 1]));
    assert_eq!(parse_version("1.x"), None);
    assert_eq!(parse_version(""), None);
}

#[test]
fn version_below_marks_old_versions_only() {
    let evaluator = evaluator();
    let profile = versioned_profile();
    let run = |version: &str| {
        evaluator.evaluate(
            &probed_item(
                "sso.acme.com",
                "/version",
                Some(response(200, &[("x-version", version)], "")),
            ),
            &profile,
        )
    };
    let old = run("v21.1.2");
    assert_eq!(old.len(), 2);
    assert_eq!(old[0].raw["version"], json!("21.1.2"));
    assert_eq!(old[1].kind.as_str(), "debt_marker");
    assert_eq!(old[1].value.as_deref(), Some("old"));
    assert_eq!(old[1].strength.get(), 0.6);
    assert_eq!(old[1].raw["version"], json!("21.1.2"));
    assert_eq!(run("22").len(), 1);
    assert_eq!(run("22.0.0").len(), 1);
    assert_eq!(run("23.1").len(), 1);
}

#[test]
fn unparsable_or_missing_version_yields_no_marker() {
    let profile = profile(
        6,
        1,
        json!({"fingerprints": [{
            "id": "app",
            "path": "/version",
            "product": "app",
            "matchers": [{"kind": "status", "equals": 200}],
            "version": {"from": "body", "pattern": "version=(\\S+)"},
            "debt_markers": [{"id": "old", "rule": {"kind": "version_below", "version": "22.0"}}]
        }]}),
    );
    let evaluator = evaluator();
    for body in ["version=1.2-beta", "nothing here"] {
        let found = evaluator.evaluate(
            &probed_item("sso.acme.com", "/version", Some(response(200, &[], body))),
            &profile,
        );
        assert_eq!(found.len(), 1, "{body}");
        assert_eq!(found[0].kind.as_str(), "product_detected");
    }
    let found = evaluator.evaluate(
        &probed_item(
            "sso.acme.com",
            "/version",
            Some(response(200, &[], "version=3.1")),
        ),
        &profile,
    );
    assert_eq!(found.len(), 2);
}

#[test]
fn matches_marker_uses_the_same_response() {
    let evaluator = evaluator();
    let profile = versioned_profile();
    let found = evaluator.evaluate(
        &probed_item(
            "sso.acme.com",
            "/version",
            Some(response(
                200,
                &[("x-version", "30"), ("X-Powered-By", "php/8")],
                "",
            )),
        ),
        &profile,
    );
    assert_eq!(found.len(), 2);
    assert_eq!(found[1].value.as_deref(), Some("powered"));
}

#[test]
fn excluded_product_suppresses_everything() {
    let profile = profile(
        7,
        1,
        json!({
            "fingerprints": [
                {"id": "kc", "path": "/realms/master", "product": "keycloak",
                 "matchers": [{"kind": "status", "equals": 200}]},
                {"id": "fk", "path": "/realms/master", "product": "ferriskey",
                 "matchers": [{"kind": "status", "equals": 200}]}
            ],
            "excluded_products": ["ferriskey"]
        }),
    );
    let found = evaluator().evaluate(
        &probed_item(
            "sso.acme.com",
            "/realms/master",
            Some(response(200, &[], "")),
        ),
        &profile,
    );
    assert!(found.is_empty());
}

#[test]
fn product_is_reported_once() {
    let found = evaluator().evaluate(
        &probed_item(
            "sso.acme.com",
            "/realms/master",
            Some(response(200, &[], "{\"realm\":1}")),
        ),
        &profile(
            8,
            1,
            json!({"fingerprints": [
                {"id": "one", "path": "/realms/master", "product": "keycloak",
                 "matchers": [{"kind": "status", "equals": 200}]},
                {"id": "two", "path": "/realms/master", "product": "keycloak",
                 "matchers": [{"kind": "body", "pattern": "realm"}]}
            ]}),
        ),
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].raw["fingerprint"], json!("one"));
}

fn without_probe() -> ActiveProfile {
    let value = json!({
        "schema_version": 1,
        "identity": {"offer": "x", "target_customer": "y"},
        "sources": {"dns_cname": {"patterns": [{"provider": "auth0", "target_suffix": "auth0.com"}]}},
        "scoring": {"factors": [{"signal": "saas_provider", "weight": 1.0}]},
        "contact_ranking": {"preferred_local_parts": ["it"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "z"}
    });
    let mut other = keycloak_profile();
    other.revision.definition = ValidatedProfile::parse(value).unwrap();
    other
}

#[test]
fn profile_without_http_probe_yields_nothing() {
    let other = without_probe();
    let found = evaluator().evaluate(
        &probed_item(
            "sso.acme.com",
            "/realms/master",
            Some(response(200, &[], "")),
        ),
        &other,
    );
    assert!(found.is_empty());
}

#[test]
fn regexes_are_compiled_once_per_profile_revision() {
    let evaluator = evaluator();
    let item = probed_item(
        "sso.acme.com",
        "/realms/master",
        Some(response(200, &[], "{\"realm\":1}")),
    );
    let first = keycloak_profile();
    for _ in 0..3 {
        evaluator.evaluate(&item, &first);
    }
    assert_eq!(evaluator.compilations(), 1);
    let revised = profile(1, 2, plain());
    evaluator.evaluate(&item, &revised);
    evaluator.evaluate(&item, &revised);
    assert_eq!(evaluator.compilations(), 2);
    evaluator.evaluate(&item, &first);
    assert_eq!(evaluator.compilations(), 2);
}

#[test]
fn cache_is_bounded() {
    let evaluator = evaluator();
    let item = probed_item("sso.acme.com", "/realms/master", None);
    for id in 0..200u128 {
        evaluator.evaluate(&item, &profile(1000 + id, 1, plain()));
    }
    assert!(evaluator.compiled.lock().unwrap().len() <= MAX_CACHED_PROFILES);
    assert_eq!(evaluator.compilations(), 200);
}
