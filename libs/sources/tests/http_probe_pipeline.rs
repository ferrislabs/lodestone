use std::{num::NonZeroU32, path::PathBuf, sync::Arc, time::Duration};

use chrono::{DateTime, TimeZone, Utc};
use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{Method::GET, MockServer};
use lodestone_core::{
    CandidateHost, CandidateHosts, Clock, CreateProfileCommand, HostOrigin, Hostname,
    LeadRepository, LodestoneUseCase, ProfileId, ProfileName, SignalRepository, SourceName,
    ValidatedProfile,
    infrastructure::{
        candidate_host::PgCandidateHosts, lead::postgres::PgLeadRepository, postgres::with_tx,
        signal::postgres::PgSignalRepository,
    },
};
use sources::http_probe::{HttpProbeSettings, HttpProbeSource, ProbeScheme};
use sqlx::PgPool;

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

async fn active_iam_profile(pool: &PgPool) -> ProfileId {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse("iam").unwrap(),
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
async fn probed_keycloak_becomes_a_signal_once_per_revisit_window(pool: PgPool) {
    let profile = active_iam_profile(&pool).await;
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
    assert_eq!(first.signals, 1);
    assert_eq!(mock.hits(), 1);
    assert_eq!(
        lead_signals(&pool, profile).await,
        vec![(
            "product_detected".to_owned(),
            Some("keycloak".to_owned()),
            "acme.com".to_owned()
        )]
    );

    let runs: Vec<(String, String, i64, i64)> =
        sqlx::query_as("SELECT source, status, volume, signals FROM runs")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        runs,
        vec![("http_probe".to_owned(), "succeeded".to_owned(), 1, 1)]
    );
    let checks: Vec<(String, String, DateTime<Utc>)> =
        sqlx::query_as("SELECT host, source, checked_at FROM host_source_checks")
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
    let visits: Vec<(String,)> = sqlx::query_as("SELECT host FROM host_visits")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(visits, vec![("sso.acme.com".to_owned(),)]);

    let second = usecase.run_source(&source, &FixedClock).await.unwrap();
    assert_eq!(second.items, 0);
    assert_eq!(second.signals, 0);
    assert_eq!(mock.hits(), 1);
}
