mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, Clock, LeadRepository, LodestoneUseCase, NormalizedDomain, Observation,
    ProfileId, Provenance, RunRepository, RunStatus, SignalKind, SignalRepository, Source,
    SourceError, SourceName, UnitInterval,
    definition::SourceTarget,
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
        SourceName::DnsCname
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

    fn evaluate(
        &self,
        item: &Self::Item,
        profile: &ActiveProfile,
        _target: &SourceTarget<'_>,
    ) -> Vec<Observation> {
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
            agent: None,
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
    assert_eq!(signals[0].source, SourceName::DnsCname);
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

async fn signal_count(pool: &PgPool, profile: ProfileId) -> usize {
    let mut total = 0;
    for domain_name in leads(pool, profile).await {
        let found = with_tx(pool, async |shared| {
            let lead = PgLeadRepository::new(&shared)
                .find_by_domain(profile, &NormalizedDomain::from_host(&domain_name).unwrap())
                .await?
                .unwrap();
            PgSignalRepository::new(&shared)
                .list_for_lead(lead.id)
                .await
        })
        .await
        .unwrap();
        total += found.len();
    }
    total
}

#[sqlx::test(migrations = "../../migrations")]
async fn replaying_a_run_creates_no_duplicate_and_keeps_profiles_apart(pool: PgPool) {
    let alpha = active_profile(&pool, "alpha").await;
    let beta = active_profile(&pool, "beta").await;
    let source = FakeSource::new(vec!["sso.acme.com", "auth.beta.io"]);
    let usecase = LodestoneUseCase::new(pool.clone());

    usecase.run_source(&source, &FixedClock).await.unwrap();
    let after_first = (
        signal_count(&pool, alpha).await,
        signal_count(&pool, beta).await,
    );
    usecase.run_source(&source, &FixedClock).await.unwrap();
    let after_replay = (
        signal_count(&pool, alpha).await,
        signal_count(&pool, beta).await,
    );

    assert_eq!(after_first, (2, 1));
    assert_eq!(after_replay, after_first);
    assert_eq!(leads(&pool, alpha).await.len(), 2);
    assert_eq!(leads(&pool, beta).await, vec!["beta.io"]);
}

async fn lead_score(pool: &PgPool, profile: ProfileId, host: &str) -> lodestone_core::Lead {
    let domain = NormalizedDomain::from_host(host).unwrap();
    with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain)
            .await
    })
    .await
    .unwrap()
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_input_gets_separate_leads_and_the_strength_of_its_signals_per_profile(pool: PgPool) {
    let alpha = active_profile(&pool, "alpha").await;
    let beta = active_profile(&pool, "beta").await;
    let source = FakeSource::new(vec!["auth.beta.io"]);

    LodestoneUseCase::new(pool.clone())
        .run_source(&source, &FixedClock)
        .await
        .unwrap();

    let in_alpha = lead_score(&pool, alpha, "beta.io").await;
    let in_beta = lead_score(&pool, beta, "beta.io").await;
    assert_ne!(in_alpha.id, in_beta.id);
    assert_eq!(in_alpha.score.get(), 80.0);
    assert_eq!(in_beta.score.get(), 80.0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_lead_score_is_the_mean_of_its_three_strongest_decayed_signals(pool: PgPool) {
    let alpha = active_profile(&pool, "alpha").await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let active = usecase
        .list_active_profiles()
        .await
        .unwrap()
        .into_iter()
        .find(|active| active.profile.id == alpha)
        .unwrap();
    let observation = |value: &str, strength: f64| Observation {
        domain: domain("sso.acme.com"),
        kind: SignalKind::parse("product_detected").unwrap(),
        value: Some(value.to_owned()),
        raw: json!({ "host": "sso.acme.com" }),
        provenance: Provenance::parse("https://sso.acme.com/").unwrap(),
        strength: UnitInterval::new(strength).unwrap(),
        observed_at: at(100),
        agent: None,
    };

    usecase
        .record_observations_at(
            &active,
            SourceName::HttpProbe,
            &[observation("a", 0.9)],
            at(100),
        )
        .await
        .unwrap();
    assert_eq!(lead_score(&pool, alpha, "acme.com").await.score.get(), 90.0);

    usecase
        .record_observations_at(
            &active,
            SourceName::HttpProbe,
            &[
                observation("b", 0.6),
                observation("c", 0.3),
                observation("d", 0.1),
            ],
            at(100),
        )
        .await
        .unwrap();
    assert_eq!(lead_score(&pool, alpha, "acme.com").await.score.get(), 60.0);

    let later = at(100) + chrono::Duration::days(365);
    usecase
        .record_observations_at(
            &active,
            SourceName::HttpProbe,
            &[observation("a", 0.9)],
            later,
        )
        .await
        .unwrap();
    assert_eq!(lead_score(&pool, alpha, "acme.com").await.score.get(), 30.0);
}
