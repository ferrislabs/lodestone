use std::{num::NonZeroU32, path::PathBuf, sync::Arc, time::Duration};

use chrono::{DateTime, TimeZone, Utc};
use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{Method::GET, MockServer};
use lodestone_core::{
    CandidateHost, CandidateHosts, Clock, CreateProfileCommand, HostOrigin, Hostname,
    LeadRepository, LodestoneUseCase, NormalizedDomain, Observation, ProfileId, ProfileName,
    Provenance, SignalKind, SignalRepository, SourceName, UnitInterval, ValidatedProfile,
    infrastructure::{
        candidate_host::PgCandidateHosts, lead::postgres::PgLeadRepository, postgres::with_tx,
        signal::postgres::PgSignalRepository,
    },
};
use serde_json::json;
use sources::http_probe::{HttpProbeSettings, HttpProbeSource, ProbeScheme};
use sqlx::{PgPool, query_as};

mod fixtures_support;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

fn iam_definition() -> ValidatedProfile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/examples/profiles/iam-ferriskey.json");
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    ValidatedProfile::parse(value).unwrap()
}

async fn active_iam_profile(pool: &PgPool, name: &str) -> ProfileId {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse(name).unwrap(),
            definition: iam_definition(),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    usecase.set_profile_active(id, true).await.unwrap();
    id
}

async fn lead_signals(pool: &PgPool, profile: ProfileId) -> Vec<(String, Option<String>, String)> {
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
            found.push((
                signal.kind.as_str().to_owned(),
                signal.value,
                lead.domain.as_str().to_owned(),
            ));
        }
    }
    found
}

#[sqlx::test(migrations = "../../migrations")]
async fn probed_keycloak_only_enriches_existing_leads_of_each_profile(pool: PgPool) {
    let profile = active_iam_profile(&pool, "iam-a").await;
    let other = active_iam_profile(&pool, "iam-b").await;
    let realm = fixtures_support::load("keycloak/kc26-realms-master.json");
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/realms/master");
        then.status(realm.status)
            .header("content-type", "application/json")
            .body(&realm.body);
    });

    let hosts = Arc::new(PgCandidateHosts::new(pool.clone()));
    hosts
        .upsert(&[CandidateHost::discovered(
            Hostname::parse("sso.acme.com").unwrap(),
            HostOrigin::Import,
            at(0),
        )
        .unwrap()])
        .await
        .unwrap();

    let fetcher = Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact: "bot@example.com".to_owned(),
            requests_per_host_per_minute: NonZeroU32::new(600).unwrap(),
            request_timeout: Duration::from_secs(5),
            max_body_bytes: 2 * 1024 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(60),
            source_quotas: Vec::new(),
            allow_private_targets: true,
        })
        .unwrap(),
    );
    let usecase = Arc::new(LodestoneUseCase::new(pool.clone()));
    let source = HttpProbeSource::new(
        fetcher,
        Arc::clone(&hosts),
        Arc::clone(&usecase),
        HttpProbeSettings {
            batch_size: 100,
            concurrency: 4,
            revisit_interval: Duration::from_secs(3600),
            scheme: ProbeScheme::Http,
            port: None,
            host_override: Some(format!("127.0.0.1:{}", server.port())),
        },
    );

    let first = usecase.run_source(&source, &FixedClock).await.unwrap();
    assert_eq!(first.items, 1);
    assert_eq!(first.signals, 0);
    assert_eq!(first.skipped_no_lead, 2);
    assert_eq!(mock.hits(), 1);
    assert!(lead_signals(&pool, profile).await.is_empty());
    assert!(lead_signals(&pool, other).await.is_empty());
    let runs: Vec<(String, String, i64, i64)> =
        query_as("SELECT source, status, volume, signals FROM runs")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        runs,
        vec![
            ("http_probe".to_owned(), "succeeded".to_owned(), 1, 0),
            ("http_probe".to_owned(), "succeeded".to_owned(), 1, 0)
        ]
    );
    let checks: Vec<(String, String, DateTime<Utc>)> =
        query_as("SELECT host, source, checked_at FROM host_source_checks")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        checks,
        vec![(
            "sso.acme.com".to_owned(),
            SourceName::HttpProbe.as_str().to_owned(),
            at(100)
        )]
    );

    let active = usecase.list_active_profiles().await.unwrap();
    let owner = active
        .iter()
        .find(|candidate| candidate.profile.id == profile)
        .unwrap();
    usecase
        .record_observations_at(
            owner,
            SourceName::CertificateTransparency,
            &[Observation {
                domain: NormalizedDomain::from_host("acme.com").unwrap(),
                kind: SignalKind::parse("certificate_seen").unwrap(),
                value: Some("sso.acme.com".to_owned()),
                raw: json!({"host": "sso.acme.com", "labels": ["sso"]}),
                provenance: Provenance::parse("https://crt.sh/?q=sso.acme.com").unwrap(),
                strength: UnitInterval::new(0.3).unwrap(),
                observed_at: at(50),
                agent: None,
            }],
            at(50),
        )
        .await
        .unwrap();

    let second = usecase.run_source(&source, &FixedClock).await.unwrap();
    assert_eq!(second.items, 6);
    assert_eq!(second.signals, 6);
    assert_eq!(second.skipped_no_lead, 6);
    assert_eq!(mock.hits(), 7);
    let found = lead_signals(&pool, profile).await;
    assert!(found.contains(&(
        "product_detected".to_owned(),
        Some("keycloak".to_owned()),
        "acme.com".to_owned()
    )));
    assert!(lead_signals(&pool, other).await.is_empty());
    let seeded: Vec<(String,)> =
        query_as("SELECT host FROM candidate_hosts WHERE origin = 'lead' ORDER BY host")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        seeded,
        [
            "accounts.acme.com",
            "acme.com",
            "auth.acme.com",
            "id.acme.com",
            "idp.acme.com",
            "login.acme.com"
        ]
        .map(|host| (host.to_owned(),))
    );
    let ran: Vec<(String, Option<DateTime<Utc>>)> =
        query_as("SELECT step, ran_at FROM lead_enrichments")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(ran, [("http_probe".to_owned(), Some(at(100)))]);

    let third = usecase.run_source(&source, &FixedClock).await.unwrap();
    assert_eq!(third.items, 0);
    assert_eq!(third.signals, 0);
    assert_eq!(mock.hits(), 7);
}
