mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, Clock, LeadRepository, LodestoneUseCase, NormalizedDomain, Observation,
    ProfileId, Provenance, RunRepository, RunStatus, SignalKind, SignalRepository, Source,
    SourceError, SourceName, UnitInterval,
    infrastructure::{
        lead::postgres::PgLeadRepository, postgres::with_tx, run::postgres::PgRunRepository,
        signal::postgres::PgSignalRepository,
    },
    sources::{SourceRunError, source_job, source_of_job},
};
use serde_json::json;
use sqlx::PgPool;
use support::{at, create_profile, domain};

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

struct FakeSource {
    hosts: Vec<&'static str>,
    fail: bool,
    collects: AtomicUsize,
}

impl FakeSource {
    fn new(hosts: Vec<&'static str>) -> Self {
        Self {
            hosts,
            fail: false,
            collects: AtomicUsize::new(0),
        }
    }
}

impl Source for FakeSource {
    type Item = &'static str;

    fn name(&self) -> SourceName {
        SourceName::HttpProbe
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Self::Item>, SourceError> {
        self.collects.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(SourceError::Upstream("upstream down".to_owned()));
        }
        Ok(self.hosts.clone())
    }

    fn evaluate(&self, item: &Self::Item, profile: &ActiveProfile) -> Vec<Observation> {
        let wanted = profile.profile.name.as_str() == "alpha" || item.contains("beta");
        if !wanted {
            return Vec::new();
        }
        vec![Observation {
            domain: domain(item),
            kind: SignalKind::parse("product_detected").unwrap(),
            value: Some("keycloak".to_owned()),
            raw: json!({ "host": item }),
            provenance: Provenance::parse(&format!("https://{item}/realms/master")).unwrap(),
            strength: UnitInterval::new(0.8).unwrap(),
            observed_at: at(50),
        }]
    }
}

async fn active_profile(pool: &PgPool, name: &str) -> ProfileId {
    let id = create_profile(pool, name).await;
    LodestoneUseCase::new(pool.clone())
        .set_profile_active(id, true)
        .await
        .unwrap();
    id
}

async fn leads(pool: &PgPool, profile: ProfileId) -> Vec<String> {
    with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .list_by_profile(profile)
            .await
    })
    .await
    .unwrap()
    .into_iter()
    .map(|lead| lead.domain.to_string())
    .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_fetch_feeds_every_profile_separately(pool: PgPool) {
    let alpha = active_profile(&pool, "alpha").await;
    let beta = active_profile(&pool, "beta").await;
    let source = FakeSource::new(vec!["sso.acme.com", "auth.beta.io"]);
    let usecase = LodestoneUseCase::new(pool.clone());

    let report = usecase.run_source(&source, &FixedClock).await.unwrap();

    assert_eq!(source.collects.load(Ordering::SeqCst), 1);
    assert_eq!(report.items, 2);
    assert_eq!(report.signals, 3);
    assert_eq!(report.runs.len(), 2);

    let mut alpha_leads = leads(&pool, alpha).await;
    alpha_leads.sort();
    assert_eq!(alpha_leads, vec!["acme.com", "beta.io"]);
    assert_eq!(leads(&pool, beta).await, vec!["beta.io"]);

    let beta_lead = with_tx(&pool, async |shared| {
        PgLeadRepository::new(&shared)
            .find_by_domain(beta, &NormalizedDomain::from_host("beta.io").unwrap())
            .await
    })
    .await
    .unwrap()
    .unwrap();
    let signals = with_tx(&pool, async |shared| {
        PgSignalRepository::new(&shared)
            .list_for_lead(beta_lead.id)
            .await
    })
    .await
    .unwrap();
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].source, SourceName::HttpProbe);
    assert_eq!(
        signals[0].provenance.as_str(),
        "https://auth.beta.io/realms/master"
    );

    let runs = with_tx(&pool, async |shared| {
        PgRunRepository::new(&shared).list_recent(10).await
    })
    .await
    .unwrap();
    assert_eq!(runs.len(), 2);
    for run in &runs {
        assert_eq!(run.status, RunStatus::Succeeded);
        assert_eq!(run.volume, 2);
        assert_eq!(run.finished_at, Some(at(100)));
        let expected_signals = if run.profile_id == alpha { 2 } else { 1 };
        assert_eq!(run.signals, expected_signals);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_collection_is_recorded_on_every_run(pool: PgPool) {
    active_profile(&pool, "alpha").await;
    active_profile(&pool, "beta").await;
    let source = FakeSource {
        fail: true,
        ..FakeSource::new(vec!["sso.acme.com"])
    };

    let error = LodestoneUseCase::new(pool.clone())
        .run_source(&source, &FixedClock)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        SourceRunError::Source(SourceError::Upstream(_))
    ));

    let runs = with_tx(&pool, async |shared| {
        PgRunRepository::new(&shared).list_recent(10).await
    })
    .await
    .unwrap();
    assert_eq!(runs.len(), 2);
    for run in runs {
        assert_eq!(run.status, RunStatus::Failed);
        assert_eq!(run.error.as_deref(), Some("upstream error: upstream down"));
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn inactive_profiles_and_profiles_without_the_source_are_skipped(pool: PgPool) {
    create_profile(&pool, "dormant").await;
    let source = FakeSource::new(vec!["sso.acme.com"]);

    let report = LodestoneUseCase::new(pool.clone())
        .run_source(&source, &FixedClock)
        .await
        .unwrap();

    assert!(report.runs.is_empty());
    assert_eq!(source.collects.load(Ordering::SeqCst), 0);
}

#[test]
fn source_job_round_trips_its_source() {
    let new_job = source_job(SourceName::DnsCname, at(0)).unwrap();
    assert_eq!(new_job.kind.as_str(), "source.run");
    assert_eq!(new_job.dedup_key.as_deref(), Some("source:dns_cname"));

    let now = at(0);
    let job = lodestone_core::Job {
        id: lodestone_core::JobId(uuid::Uuid::nil()),
        kind: new_job.kind,
        payload: new_job.payload,
        status: lodestone_core::JobStatus::Running,
        attempts: 1,
        max_attempts: new_job.max_attempts,
        run_at: now,
        last_error: None,
        created_at: now,
        updated_at: now,
    };
    assert_eq!(source_of_job(&job), Ok(SourceName::DnsCname));
}
