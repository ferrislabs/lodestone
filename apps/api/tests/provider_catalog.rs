mod support;

use std::{
    collections::BTreeMap,
    num::{NonZeroU16, NonZeroU32},
    sync::Arc,
    time::Duration,
};

use api::{
    jobs::{Intervals, JobRouter, WiredSources},
    providers::{CatalogAccess, ProviderSeed, Provisioned, blocking_provider, provision_providers},
};
use chrono::{DateTime, Duration as Delta, TimeZone, Utc};
use common::Secret;
use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{
    Method::{GET, POST},
    Mock, MockServer,
};
use lodestone_core::{
    ActiveProfile, Job, JobId, JobKind, JobStatus, Profile, ProfileId, ProfileName,
    ProfileRevision, ProviderStatus, RevisionNumber, ValidatedProfile,
    infrastructure::contact_target::postgres::PgAtsTargets,
    jobs::{JobHandler, JobOutcome},
};
use serde_json::{Map, Value, json};
use sources::{
    ats::{AtsSettings, AtsSource},
    boamp::{BoampSettings, BoampSource},
    decp::{DecpSettings, DecpSource},
    france_travail::{FranceTravailSettings, FranceTravailSource},
    ted::{TedSettings, TedSource},
};
use sqlx::PgPool;
use support::Harness;
use url::Url;
use uuid::Uuid;

const CLIENT_ID: &str = "seed-client";
const CLIENT_SECRET: &str = "seed-secret-0123456789";

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn public_seed(server: &MockServer) -> ProviderSeed {
    let mut seed = ProviderSeed::default();
    seed.settings
        .insert("boamp", object(json!({"base_url": server.url("/boamp")})));
    seed.settings.insert(
        "decp",
        object(json!({"dataset_api_url": server.url("/decp")})),
    );
    seed.settings
        .insert("ted", object(json!({"base_url": server.url("/ted")})));
    seed.settings.insert(
        "certificate_transparency",
        object(json!({"log_list_url": server.url("/ct")})),
    );
    seed.settings.insert(
        "company_research",
        object(json!({"entreprises_url": server.url("/company")})),
    );
    seed
}

fn france_travail_seed(server: &MockServer, client_id: &str) -> ProviderSeed {
    let mut seed = public_seed(server);
    seed.settings.insert(
        "france_travail",
        object(json!({
            "client_id": client_id,
            "token_url": server.url("/token"),
            "api_base_url": server.url("/api"),
        })),
    );
    seed.secrets.insert(
        "france_travail",
        BTreeMap::from([("client_secret", Secret::new(CLIENT_SECRET))]),
    );
    seed
}

async fn public_upstreams(server: &MockServer) {
    for (path, status) in [
        ("/boamp", 200),
        ("/decp", 200),
        ("/ted", 405),
        ("/ct", 200),
        ("/company", 200),
    ] {
        server
            .mock_async(|when, then| {
                when.method(GET).path(path);
                then.status(status).body("{}");
            })
            .await;
    }
}

async fn france_travail_upstream(server: &MockServer) -> (Mock<'_>, Mock<'_>) {
    let token = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/token")
                .body_contains("grant_type=client_credentials")
                .body_contains(format!("client_id={CLIENT_ID}"))
                .body_contains(format!("client_secret={CLIENT_SECRET}"));
            then.status(200)
                .json_body(json!({"access_token": "catalog-token-1", "expires_in": 1499}));
        })
        .await;
    let search = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/api/offres/search")
                .header("authorization", "Bearer catalog-token-1");
            then.status(200).json_body(json!({"resultats": [{
                "id": "OFF1",
                "intitule": "Ingénieur IAM",
                "dateCreation": "2026-10-02T08:00:00.000Z",
                "entreprise": {"url": "https://acme-identity.fr"},
            }]}));
        })
        .await;
    (token, search)
}

fn status_of(report: &[Provisioned], key: &str) -> Option<Provisioned> {
    report.iter().copied().find(|entry| entry.key == key)
}

async fn provision(harness: &Harness, seed: &ProviderSeed) -> Vec<Provisioned> {
    provision_providers(
        &harness.state.usecase,
        &harness.state.providers,
        &harness.state.cipher,
        seed,
    )
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn public_providers_are_created_and_tested_on_first_start(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let server = MockServer::start_async().await;
    public_upstreams(&server).await;

    let report = provision(&harness, &public_seed(&server)).await;

    for key in [
        "boamp",
        "decp",
        "ted",
        "ats",
        "certificate_transparency",
        "dns_cname",
        "http_probe",
        "contacts",
        "company_research",
    ] {
        let entry = status_of(&report, key).unwrap();
        assert!(entry.created, "{key}");
        assert_eq!(entry.status, ProviderStatus::Ready, "{key}");
        assert!(
            harness.state.usecase.is_provider_usable(key).await.unwrap(),
            "{key}"
        );
    }
    assert!(status_of(&report, "france_travail").is_none());
    assert!(
        harness
            .state
            .usecase
            .get_provider_configuration("france_travail")
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failing_public_provider_is_not_ready(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let server = MockServer::start_async().await;
    public_upstreams(&server).await;
    let mut seed = public_seed(&server);
    seed.settings.insert(
        "decp",
        object(json!({"dataset_api_url": server.url("/down")})),
    );
    server
        .mock_async(|when, then| {
            when.method(GET).path("/down");
            then.status(503);
        })
        .await;

    let report = provision(&harness, &seed).await;

    assert_eq!(
        status_of(&report, "decp").unwrap().status,
        ProviderStatus::Failing
    );
    assert!(
        !harness
            .state
            .usecase
            .is_provider_usable("decp")
            .await
            .unwrap()
    );
    assert_eq!(
        blocking_provider(&harness.state.usecase, "decp")
            .await
            .unwrap(),
        Some("decp")
    );
    assert_eq!(
        blocking_provider(&harness.state.usecase, "boamp")
            .await
            .unwrap(),
        None
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_public_provider_is_retested_at_the_next_start(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let server = MockServer::start_async().await;
    public_upstreams(&server).await;
    let mut seed = public_seed(&server);
    seed.settings.insert(
        "decp",
        object(json!({"dataset_api_url": server.url("/late")})),
    );
    let first = provision(&harness, &seed).await;
    assert_eq!(
        status_of(&first, "decp").unwrap().status,
        ProviderStatus::Failing
    );
    server
        .mock_async(|when, then| {
            when.method(GET).path("/late");
            then.status(200).body("{}");
        })
        .await;

    let second = provision(&harness, &seed).await;

    let decp = status_of(&second, "decp").unwrap();
    assert!(!decp.created);
    assert_eq!(decp.status, ProviderStatus::Ready);
    assert!(status_of(&second, "boamp").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn environment_credentials_seed_france_travail_once(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let server = MockServer::start_async().await;
    public_upstreams(&server).await;
    let (token, _search) = france_travail_upstream(&server).await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/api/offres/search");
            then.status(206).body("{}");
        })
        .await;

    let first = provision(&harness, &france_travail_seed(&server, CLIENT_ID)).await;

    let entry = status_of(&first, "france_travail").unwrap();
    assert!(entry.created);
    assert_eq!(entry.status, ProviderStatus::Ready);
    let stored = harness
        .state
        .usecase
        .get_provider_configuration("france_travail")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.settings["client_id"], CLIENT_ID);
    assert!(stored.has_secret("client_secret"));
    assert!(!format!("{:?}", stored.secrets).contains(CLIENT_SECRET));
    token.assert_hits_async(1).await;

    let second = provision(&harness, &france_travail_seed(&server, "another-client")).await;

    assert!(status_of(&second, "france_travail").is_none());
    let unchanged = harness
        .state
        .usecase
        .get_provider_configuration("france_travail")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(unchanged.settings["client_id"], CLIENT_ID);
    assert_eq!(unchanged.updated_at, stored.updated_at);
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_environment_credentials_france_travail_stays_unconfigured(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let server = MockServer::start_async().await;
    public_upstreams(&server).await;

    provision(&harness, &public_seed(&server)).await;

    assert_eq!(
        blocking_provider(&harness.state.usecase, "france_travail")
            .await
            .unwrap(),
        Some("france_travail")
    );
}

fn polite(allow_private: bool) -> Arc<PoliteFetcher> {
    Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact: "bot@example.com".to_owned(),
            requests_per_host_per_minute: NonZeroU32::new(10_000).unwrap(),
            request_timeout: Duration::from_secs(5),
            max_body_bytes: 1024 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(60),
            source_quotas: Vec::new(),
            allow_private_targets: allow_private,
        })
        .unwrap(),
    )
}

fn iam_profile() -> ActiveProfile {
    let value = json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": {"france_travail": {"keywords": ["iam"]}},
        "scoring": {"factors": [{"signal": "saas_provider", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
    });
    let profile_id = ProfileId(Uuid::from_u128(1));
    let epoch = Utc.timestamp_opt(0, 0).unwrap();
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

#[sqlx::test(migrations = "../../migrations")]
async fn the_france_travail_source_takes_its_token_from_the_catalog_engine(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let server = MockServer::start_async().await;
    public_upstreams(&server).await;
    let (token, search) = france_travail_upstream(&server).await;
    provision(&harness, &france_travail_seed(&server, CLIENT_ID)).await;
    token.assert_hits_async(1).await;
    let probed = search.hits_async().await;
    let source = FranceTravailSource::new(
        polite(true),
        CatalogAccess::new(
            harness.state.usecase.clone(),
            Arc::clone(&harness.state.providers),
            Arc::clone(&harness.state.cipher),
        ),
        FranceTravailSettings {
            published_since_days: 7,
            max_pages: 2,
        },
    );

    let (candidates, stats) = source.collect_with_stats(&[iam_profile()]).await.unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(stats.requests, 1);
    search.assert_hits_async(probed + 1).await;
    token.assert_hits_async(1).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_france_travail_source_fails_cleanly_without_a_catalog_configuration(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let source = FranceTravailSource::new(
        polite(true),
        CatalogAccess::new(
            harness.state.usecase.clone(),
            Arc::clone(&harness.state.providers),
            Arc::clone(&harness.state.cipher),
        ),
        FranceTravailSettings {
            published_since_days: 7,
            max_pages: 2,
        },
    );

    let error = source
        .collect_with_stats(&[iam_profile()])
        .await
        .unwrap_err();

    assert!(error.to_string().contains("not configured"));
}

fn router(harness: &Harness, pool: &PgPool, upstream: &MockServer) -> JobRouter {
    let fetcher = polite(true);
    let url = |path: &str| Url::parse(&upstream.url(path)).unwrap();
    JobRouter::new(
        harness.state.usecase.clone(),
        WiredSources {
            boamp: BoampSource::new(
                Arc::clone(&fetcher),
                BoampSettings {
                    base_url: url("/boamp"),
                    lookback_days: 7,
                    max_pages: 1,
                },
            ),
            decp: DecpSource::new(
                Arc::clone(&fetcher),
                DecpSettings {
                    dataset_api_url: url("/decp"),
                    annuaire_url: url("/annuaire"),
                    entreprises_url: url("/company"),
                    lookback_days: 7,
                    notified_within_days: 7,
                    max_body_bytes: 1024 * 1024,
                    max_rate_limit_wait: Duration::from_secs(1),
                },
            ),
            ted: TedSource::new(
                Arc::clone(&fetcher),
                TedSettings {
                    base_url: url("/ted"),
                    lookback_days: 7,
                    max_pages: 1,
                    buyer_countries: Vec::new(),
                },
            ),
            france_travail: FranceTravailSource::new(
                Arc::clone(&fetcher),
                CatalogAccess::new(
                    harness.state.usecase.clone(),
                    Arc::clone(&harness.state.providers),
                    Arc::clone(&harness.state.cipher),
                ),
                FranceTravailSettings {
                    published_since_days: 7,
                    max_pages: 1,
                },
            ),
            ats: AtsSource::new(
                fetcher,
                PgAtsTargets::new(pool.clone()),
                AtsSettings {
                    batch_size: 1,
                    revisit_interval: Duration::from_secs(60),
                    max_career_paths: 1,
                    max_rate_limit_wait: Duration::from_secs(1),
                },
            ),
        },
        Intervals {
            other_sources: Duration::from_secs(6 * 3600),
            decp: Duration::from_secs(6 * 3600),
        },
    )
}

fn source_job(source: &str) -> Job {
    let now = Utc::now();
    Job {
        id: JobId(Uuid::new_v4()),
        kind: JobKind::parse("source.run").unwrap(),
        payload: json!({"source": source}),
        status: JobStatus::Running,
        attempts: 1,
        max_attempts: NonZeroU16::new(3).unwrap(),
        run_at: now,
        last_error: None,
        created_at: now,
        updated_at: now,
    }
}

fn rescheduled(outcome: JobOutcome) -> DateTime<Utc> {
    match outcome {
        JobOutcome::RescheduleAt(at) => at,
        other => panic!("expected a reschedule, got {other:?}"),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_source_whose_provider_is_not_ready_does_not_run(pool: PgPool) {
    let harness = Harness::new(pool.clone()).await;
    let upstream = MockServer::start_async().await;
    let records = upstream
        .mock_async(|when, then| {
            when.method(GET).path("/boamp");
            then.status(200).json_body(json!({"results": []}));
        })
        .await;
    let router = router(&harness, &pool, &upstream);
    let before = Utc::now();

    let outcome = router.handle(&source_job("boamp")).await.unwrap();

    let next = rescheduled(outcome);
    assert!(next >= before + Delta::minutes(4));
    assert!(next <= Utc::now() + Delta::minutes(6));
    records.assert_hits_async(0).await;
    assert!(
        harness
            .state
            .usecase
            .list_source_schedule()
            .await
            .unwrap()
            .runs
            .is_empty()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_source_whose_provider_is_ready_runs_on_its_own_interval(pool: PgPool) {
    let harness = Harness::new(pool.clone()).await;
    let upstream = MockServer::start_async().await;
    public_upstreams(&upstream).await;
    provision(&harness, &public_seed(&upstream)).await;
    let router = router(&harness, &pool, &upstream);
    let before = Utc::now();

    let outcome = router.handle(&source_job("boamp")).await.unwrap();

    let next = rescheduled(outcome);
    assert!(next >= before + Delta::hours(5));
}

#[sqlx::test(migrations = "../../migrations")]
async fn france_travail_does_not_run_without_a_ready_catalog_configuration(pool: PgPool) {
    let harness = Harness::new(pool.clone()).await;
    let upstream = MockServer::start_async().await;
    let router = router(&harness, &pool, &upstream);

    let outcome = router.handle(&source_job("france_travail")).await.unwrap();

    assert!(rescheduled(outcome) <= Utc::now() + Delta::minutes(6));
}
