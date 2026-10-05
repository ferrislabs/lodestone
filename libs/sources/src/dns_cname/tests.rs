use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use chrono::TimeZone;
use common::CoreError;
use lodestone_core::{
    DnsError, HostOrigin, Profile, ProfileId, ProfileName, ProfileRevision, RevisionNumber,
    ValidatedProfile,
};
use uuid::Uuid;

use super::*;

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
        .find(|agent| agent.source.source_name() == kind)
        .cloned()
        .unwrap()
}

fn host(name: &str) -> Hostname {
    Hostname::parse(name).unwrap()
}

fn candidate(name: &str) -> CandidateHost {
    CandidateHost::discovered(host(name), HostOrigin::Import, at(0)).unwrap()
}

fn profile_with(patterns: serde_json::Value) -> ActiveProfile {
    let sources = if patterns.is_null() {
        json!({
            "http_probe": {
                "fingerprints": [{
                    "id": "keycloak-master-realm",
                    "path": "/realms/master",
                    "product": "keycloak",
                    "matchers": [{"kind": "status", "equals": 200}]
                }]
            }
        })
    } else {
        json!({ "dns_cname": { "patterns": patterns } })
    };
    let value = json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": sources,
        "scoring": {"factors": [{"signal": "saas_provider", "value": "auth0", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses a SaaS IAM"}
    });
    ActiveProfile {
        profile: Profile {
            id: ProfileId(Uuid::nil()),
            name: ProfileName::parse("p").unwrap(),
            active: true,
            created_at: at(0),
            updated_at: at(0),
        },
        revision: ProfileRevision {
            profile_id: ProfileId(Uuid::nil()),
            number: RevisionNumber::FIRST,
            definition: ValidatedProfile::parse(value).unwrap(),
            created_by: None,
            created_at: at(0),
        },
    }
}

fn auth0_profile() -> ActiveProfile {
    profile_with(json!([
        {"provider": "auth0", "target_suffix": "auth0.com"},
        {"provider": "okta", "target_suffix": "okta.com"},
        {"provider": "auth0", "target_suffix": "edge.auth0.com"}
    ]))
}

fn resolved(name: &str, chain: &[&str]) -> ResolvedHost {
    ResolvedHost {
        host: candidate(name),
        chain: chain.iter().map(|hop| host(hop)).collect(),
        resolved_at: at(7),
    }
}

#[derive(Default)]
struct FakeResolver {
    answers: HashMap<String, Result<Vec<String>, DnsError>>,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
}

impl FakeResolver {
    fn with(answers: &[(&str, Result<Vec<&str>, DnsError>)]) -> Self {
        Self {
            answers: answers
                .iter()
                .map(|(name, answer)| {
                    (
                        (*name).to_owned(),
                        answer
                            .clone()
                            .map(|chain| chain.into_iter().map(str::to_owned).collect()),
                    )
                })
                .collect(),
            ..Self::default()
        }
    }
}

impl DnsResolver for FakeResolver {
    async fn cname_chain(&self, host: &Hostname) -> Result<Vec<Hostname>, DnsError> {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_in_flight.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(20)).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        match self.answers.get(host.as_str()) {
            Some(Ok(chain)) => Ok(chain
                .iter()
                .map(|hop| Hostname::parse(hop).unwrap())
                .collect()),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(Vec::new()),
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

fn settings(concurrency: usize) -> DnsCnameSettings {
    DnsCnameSettings {
        batch_size: 50,
        concurrency,
        revisit_interval: Duration::from_secs(3600),
    }
}

fn source(
    resolver: FakeResolver,
    hosts: FakeHosts,
    concurrency: usize,
) -> (
    DnsCnameSource<FakeResolver, FakeHosts>,
    Arc<FakeResolver>,
    Arc<FakeHosts>,
) {
    let resolver = Arc::new(resolver);
    let hosts = Arc::new(hosts);
    (
        DnsCnameSource::new(
            Arc::clone(&resolver),
            Arc::clone(&hosts),
            settings(concurrency),
        ),
        resolver,
        hosts,
    )
}

fn evaluator() -> DnsCnameSource<FakeResolver, FakeHosts> {
    source(FakeResolver::default(), FakeHosts::default(), 1).0
}

#[test]
fn suffix_matches_on_label_boundary_only() {
    let evaluator = evaluator();
    let profile = auth0_profile();
    for chain in ["tenant.eu.auth0.com", "auth0.com", "TENANT.Auth0.com"] {
        let found = evaluator.evaluate(
            &resolved("sso.acme.com", &[chain]),
            &profile,
            &agent_of(&profile, SourceName::DnsCname),
        );
        assert_eq!(found.len(), 1, "{chain}");
        assert_eq!(found[0].value.as_deref(), Some("auth0"));
    }
    for chain in ["evilauth0.com", "auth0.com.evil.io", "auth0.co"] {
        let found = evaluator.evaluate(
            &resolved("sso.acme.com", &[chain]),
            &profile,
            &agent_of(&profile, SourceName::DnsCname),
        );
        assert!(found.is_empty(), "{chain}");
    }
}

#[test]
fn observation_carries_evidence() {
    let profile = auth0_profile();
    let found = evaluator().evaluate(
        &resolved("sso.acme.com", &["lb.example.net", "acme.eu.auth0.com"]),
        &profile,
        &agent_of(&profile, SourceName::DnsCname),
    );
    assert_eq!(found.len(), 1);
    let observation = &found[0];
    assert_eq!(observation.domain.as_str(), "acme.com");
    assert_eq!(observation.kind.as_str(), "saas_provider");
    assert_eq!(
        observation.provenance.as_str(),
        "dns:sso.acme.com?type=CNAME"
    );
    assert_eq!(observation.strength.get(), 0.9);
    assert_eq!(observation.observed_at, at(7));
    assert_eq!(
        observation.raw,
        json!({
            "host": "sso.acme.com",
            "chain": ["lb.example.net", "acme.eu.auth0.com"],
            "matched": "acme.eu.auth0.com"
        })
    );
}

#[test]
fn several_providers_match_and_are_deduplicated() {
    let profile = auth0_profile();
    let found = evaluator().evaluate(
        &resolved(
            "sso.acme.com",
            &["a.auth0.com", "b.edge.auth0.com", "c.okta.com"],
        ),
        &profile,
        &agent_of(&profile, SourceName::DnsCname),
    );
    let values: Vec<_> = found.iter().filter_map(|o| o.value.as_deref()).collect();
    assert_eq!(values, vec!["auth0", "okta"]);
}

#[test]
fn profile_without_dns_cname_yields_nothing() {
    let profile = profile_with(serde_json::Value::Null);
    let found = evaluator().evaluate(
        &resolved("sso.acme.com", &["a.auth0.com"]),
        &profile,
        &agent_of(&profile, SourceName::HttpProbe),
    );
    assert!(found.is_empty());
}

#[tokio::test]
async fn failed_lookups_are_skipped_but_marked_so_they_wait_for_the_revisit() {
    let resolver = FakeResolver::with(&[
        ("ok.acme.com", Ok(vec!["t.auth0.com"])),
        ("empty.acme.com", Ok(vec![])),
        ("slow.acme.com", Err(DnsError::Timeout)),
        (
            "bad.acme.com",
            Err(DnsError::Failure("servfail".to_owned())),
        ),
    ]);
    let hosts = FakeHosts {
        due: ["ok", "slow", "empty", "bad"]
            .iter()
            .map(|label| candidate(&format!("{label}.acme.com")))
            .collect(),
        ..FakeHosts::default()
    };
    let (source, _, hosts) = source(resolver, hosts, 4);

    let items = source.collect(&[], at(100)).await.unwrap();

    let names: Vec<_> = items.iter().map(|i| i.host.host.as_str()).collect();
    assert_eq!(names, vec!["ok.acme.com", "empty.acme.com"]);
    assert!(items[1].chain.is_empty());
    assert_eq!(items[0].resolved_at, at(100));
    let marked = hosts.marked.lock().unwrap();
    assert_eq!(marked.len(), 2);
    assert_eq!(
        marked[0].0,
        vec!["ok.acme.com", "empty.acme.com", "bad.acme.com"]
    );
    assert_eq!(marked[0].1, SourceName::DnsCname);
    assert_eq!(marked[0].2, at(100));
    assert_eq!(marked[1].0, vec!["slow.acme.com"]);
}

#[tokio::test]
async fn timeouts_retry_after_an_hour_while_other_failures_wait_the_full_revisit() {
    let resolver = Arc::new(FakeResolver::with(&[
        ("slow.acme.com", Err(DnsError::Timeout)),
        (
            "bad.acme.com",
            Err(DnsError::Failure("nxdomain".to_owned())),
        ),
    ]));
    let hosts = Arc::new(FakeHosts {
        due: vec![candidate("slow.acme.com"), candidate("bad.acme.com")],
        ..FakeHosts::default()
    });
    let revisit = Duration::from_secs(720 * 3600);
    let source = DnsCnameSource::new(
        resolver,
        Arc::clone(&hosts),
        DnsCnameSettings {
            batch_size: 10,
            concurrency: 2,
            revisit_interval: revisit,
        },
    );

    source.collect(&[], at(0)).await.unwrap();

    let marked = hosts.marked.lock().unwrap();
    assert_eq!(marked[0].0, vec!["bad.acme.com"]);
    assert_eq!(marked[0].2, at(0));
    assert_eq!(marked[1].0, vec!["slow.acme.com"]);
    assert_eq!(marked[1].2, at(0) - chrono::Duration::hours(719));
}

#[tokio::test]
async fn concurrency_bound_is_respected() {
    let hosts = FakeHosts {
        due: (0..12)
            .map(|n| candidate(&format!("h{n}.acme.com")))
            .collect(),
        ..FakeHosts::default()
    };
    let (source, resolver, _) = source(FakeResolver::default(), hosts, 3);

    let items = source.collect(&[], at(0)).await.unwrap();

    assert_eq!(items.len(), 12);
    let max = resolver.max_in_flight.load(Ordering::SeqCst);
    assert!((2..=3).contains(&max), "max in flight {max}");
}

#[tokio::test]
async fn revisit_window_and_batch_size_are_passed_to_list_due() {
    let (source, _, hosts) = source(FakeResolver::default(), FakeHosts::default(), 2);

    source.collect(&[], at(10_000)).await.unwrap();

    assert_eq!(
        *hosts.list_args.lock().unwrap(),
        vec![(SourceName::DnsCname, at(10_000 - 3600), 50)]
    );
}

#[tokio::test]
async fn store_failures_become_upstream_errors() {
    let failing_list = FakeHosts {
        fail_list: true,
        ..FakeHosts::default()
    };
    let (source_a, _, _) = source(FakeResolver::default(), failing_list, 2);
    assert!(matches!(
        source_a.collect(&[], at(0)).await,
        Err(SourceError::Upstream(_))
    ));

    let failing_mark = FakeHosts {
        due: vec![candidate("a.acme.com")],
        fail_mark: true,
        ..FakeHosts::default()
    };
    let (source_b, _, _) = source(FakeResolver::default(), failing_mark, 2);
    assert!(matches!(
        source_b.collect(&[], at(0)).await,
        Err(SourceError::Upstream(_))
    ));
}
