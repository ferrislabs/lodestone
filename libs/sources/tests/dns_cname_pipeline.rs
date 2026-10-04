use std::{sync::Arc, time::Duration};

use chrono::{DateTime, TimeZone, Utc};
use lodestone_core::{
    CandidateHost, CandidateHosts, Clock, CreateProfileCommand, DnsError, DnsResolver, HostOrigin,
    Hostname, LeadRepository, LodestoneUseCase, ProfileId, ProfileName, SignalRepository,
    ValidatedProfile,
    infrastructure::{
        candidate_host::PgCandidateHosts, lead::postgres::PgLeadRepository, postgres::with_tx,
        signal::postgres::PgSignalRepository,
    },
};
use serde_json::json;
use sources::dns_cname::{DnsCnameSettings, DnsCnameSource};
use sqlx::PgPool;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

struct FakeResolver;

impl DnsResolver for FakeResolver {
    async fn cname_chain(&self, host: &Hostname) -> Result<Vec<Hostname>, DnsError> {
        let target = match host.as_str() {
            "sso.acme.com" => "acme.eu.auth0.com",
            "www.acme.com" => "cdn.example.net",
            _ => return Ok(Vec::new()),
        };
        Ok(vec![Hostname::parse(target).unwrap()])
    }
}

fn definition(sources: serde_json::Value) -> ValidatedProfile {
    ValidatedProfile::parse(json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": sources,
        "scoring": {"factors": [{"signal": "saas_provider", "value": "auth0", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses a SaaS IAM"}
    }))
    .unwrap()
}

async fn active_profile(pool: &PgPool, name: &str, sources: serde_json::Value) -> ProfileId {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse(name).unwrap(),
            definition: definition(sources),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    usecase.set_profile_active(id, true).await.unwrap();
    id
}

async fn lead_signals(pool: &PgPool, profile: ProfileId) -> Vec<(String, Option<String>)> {
    let leads = with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .list_by_profile(profile)
            .await
    })
    .await
    .unwrap();
    let mut found = Vec::new();
    for lead in leads {
        let signals = with_tx(pool, async |shared| {
            PgSignalRepository::new(&shared)
                .list_for_lead(lead.id)
                .await
        })
        .await
        .unwrap();
        for signal in signals {
            found.push((signal.kind.as_str().to_owned(), signal.value));
        }
    }
    found
}

#[sqlx::test(migrations = "../../migrations")]
async fn cname_to_a_vendor_becomes_a_signal_for_matching_profiles_only(pool: PgPool) {
    let with_patterns = active_profile(
        &pool,
        "alpha",
        json!({"dns_cname": {"patterns": [{"provider": "auth0", "target_suffix": "auth0.com"}]}}),
    )
    .await;
    let without_patterns = active_profile(
        &pool,
        "beta",
        json!({"http_probe": {"fingerprints": [{
            "id": "keycloak-master-realm",
            "path": "/realms/master",
            "product": "keycloak",
            "matchers": [{"kind": "status", "equals": 200}]
        }]}}),
    )
    .await;

    let hosts = Arc::new(PgCandidateHosts::new(pool.clone()));
    let candidates: Vec<CandidateHost> = ["sso.acme.com", "www.acme.com", "app.other.org"]
        .iter()
        .map(|name| {
            CandidateHost::discovered(Hostname::parse(name).unwrap(), HostOrigin::Import, at(0))
                .unwrap()
        })
        .collect();
    hosts.upsert(&candidates).await.unwrap();

    let source = DnsCnameSource::new(
        Arc::new(FakeResolver),
        Arc::clone(&hosts),
        DnsCnameSettings {
            batch_size: 100,
            concurrency: 4,
            revisit_interval: Duration::from_secs(3600),
        },
    );
    let usecase = LodestoneUseCase::new(pool.clone());

    let first = usecase.run_source(&source, &FixedClock).await.unwrap();
    assert_eq!(first.items, 3);
    assert_eq!(first.signals, 1);
    assert_eq!(
        lead_signals(&pool, with_patterns).await,
        vec![("saas_provider".to_owned(), Some("auth0".to_owned()))]
    );
    assert!(lead_signals(&pool, without_patterns).await.is_empty());

    let second = usecase.run_source(&source, &FixedClock).await.unwrap();
    assert_eq!(second.items, 0);
    assert_eq!(second.signals, 0);
}
