mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, Clock, CreateProfileCommand, LeadRepository, LodestoneUseCase, Observation,
    ProfileId, ProfileName, Provenance, RunRepository, SignalKind, SignalRepository, Source,
    SourceError, SourceName, UnitInterval, ValidatedProfile,
    definition::{AgentId, SourceTarget},
    infrastructure::{
        lead::postgres::PgLeadRepository, postgres::with_tx, run::postgres::PgRunRepository,
        signal::postgres::PgSignalRepository,
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{at, domain};

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

struct AgentSource {
    collects: AtomicUsize,
    name: SourceName,
}

impl AgentSource {
    fn new() -> Self {
        Self::named(SourceName::CertificateTransparency)
    }

    fn named(name: SourceName) -> Self {
        Self {
            collects: AtomicUsize::new(0),
            name,
        }
    }
}

impl Source for AgentSource {
    type Item = &'static str;

    fn name(&self) -> SourceName {
        self.name
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Self::Item>, SourceError> {
        self.collects.fetch_add(1, Ordering::SeqCst);
        Ok(vec!["sso.acme.com", "auth.beta.io"])
    }

    fn serves(&self, item: &Self::Item, target: &SourceTarget<'_>) -> bool {
        target.agent_id().map(AgentId::as_str) != Some("probe-b") || item.contains("beta")
    }

    fn evaluate(
        &self,
        item: &Self::Item,
        _profile: &ActiveProfile,
        _target: &SourceTarget<'_>,
    ) -> Vec<Observation> {
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

fn probe_agent(id: &str, enabled: bool) -> Value {
    json!({
        "type": "certificates",
        "id": id,
        "name": id,
        "enabled": enabled,
        "labels": ["sso", "auth"]
    })
}

async fn activate(pool: &PgPool, name: &str, agents: Vec<Value>) -> ProfileId {
    let mut definition: Value = serde_json::from_str(IAM).unwrap();
    definition["agents"] = Value::Array(agents);
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse(name).unwrap(),
            definition: ValidatedProfile::parse(definition).unwrap(),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    usecase.set_profile_active(id, true).await.unwrap();
    id
}

async fn signals_of(pool: &PgPool, profile: ProfileId, host: &str) -> Vec<lodestone_core::Signal> {
    with_tx(pool, async |shared| {
        let Some(lead) = PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain(host))
            .await?
        else {
            return Ok(Vec::new());
        };
        PgSignalRepository::new(&shared)
            .list_for_lead(lead.id)
            .await
    })
    .await
    .unwrap()
}

async fn runs(pool: &PgPool) -> Vec<lodestone_core::Run> {
    with_tx(pool, async |shared| {
        PgRunRepository::new(&shared).list_recent(10).await
    })
    .await
    .unwrap()
}

fn agent(id: &str) -> Option<AgentId> {
    Some(AgentId::new(id))
}

#[sqlx::test(migrations = "../../migrations")]
async fn each_enabled_agent_gets_its_own_run_with_its_volume(pool: PgPool) {
    let profile = activate(
        &pool,
        "iam",
        vec![probe_agent("probe-a", true), probe_agent("probe-b", true)],
    )
    .await;
    let source = AgentSource::new();

    let report = LodestoneUseCase::new(pool.clone())
        .run_source(&source, &FixedClock)
        .await
        .unwrap();

    assert_eq!(source.collects.load(Ordering::SeqCst), 1);
    assert_eq!(report.runs.len(), 2);
    assert_eq!(report.items, 2);
    let mut recorded = runs(&pool).await;
    recorded.sort_by(|a, b| {
        a.agent
            .as_ref()
            .map(AgentId::as_str)
            .cmp(&b.agent.as_ref().map(AgentId::as_str))
    });
    assert_eq!(recorded[0].agent, agent("probe-a"));
    assert_eq!(recorded[0].profile_id, profile);
    assert_eq!(recorded[0].volume, 2);
    assert_eq!(recorded[0].signals, 2);
    assert_eq!(recorded[1].agent, agent("probe-b"));
    assert_eq!(recorded[1].volume, 1);
    assert_eq!(recorded[1].signals, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_first_agent_to_record_evidence_keeps_the_signal(pool: PgPool) {
    let profile = activate(
        &pool,
        "iam",
        vec![probe_agent("probe-a", true), probe_agent("probe-b", true)],
    )
    .await;

    LodestoneUseCase::new(pool.clone())
        .run_source(&AgentSource::new(), &FixedClock)
        .await
        .unwrap();

    let shared = signals_of(&pool, profile, "beta.io").await;
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].agent, agent("probe-a"));
    let only_a = signals_of(&pool, profile, "acme.com").await;
    assert_eq!(only_a.len(), 1);
    assert_eq!(only_a[0].agent, agent("probe-a"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_disabled_agent_produces_no_run_and_no_signal(pool: PgPool) {
    let profile = activate(
        &pool,
        "iam",
        vec![probe_agent("probe-a", false), probe_agent("probe-b", true)],
    )
    .await;

    LodestoneUseCase::new(pool.clone())
        .run_source(&AgentSource::new(), &FixedClock)
        .await
        .unwrap();

    let recorded = runs(&pool).await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].agent, agent("probe-b"));
    assert!(signals_of(&pool, profile, "acme.com").await.is_empty());
    let beta = signals_of(&pool, profile, "beta.io").await;
    assert_eq!(beta.len(), 1);
    assert_eq!(beta[0].agent, agent("probe-b"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_profile_with_only_disabled_agents_of_the_kind_is_not_run(pool: PgPool) {
    activate(
        &pool,
        "iam",
        vec![probe_agent("probe-a", false), probe_agent("probe-b", false)],
    )
    .await;
    let source = AgentSource::new();

    let report = LodestoneUseCase::new(pool.clone())
        .run_source(&source, &FixedClock)
        .await
        .unwrap();

    assert!(report.runs.is_empty());
    assert_eq!(source.collects.load(Ordering::SeqCst), 0);
    assert!(runs(&pool).await.is_empty());
}

async fn has_lead(pool: &PgPool, profile: ProfileId, host: &str) -> bool {
    with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain(host))
            .await
    })
    .await
    .unwrap()
    .is_some()
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_http_probe_enrichment_only_completes_leads_of_its_own_profile(pool: PgPool) {
    let first = activate(&pool, "iam-a", vec![probe_agent("probe-a", true)]).await;
    let second = activate(&pool, "iam-b", vec![probe_agent("probe-a", true)]).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let active = usecase.list_active_profiles().await.unwrap();
    let owner = active
        .iter()
        .find(|candidate| candidate.profile.id == first)
        .unwrap();
    usecase
        .record_observations_at(
            owner,
            SourceName::CertificateTransparency,
            &[Observation {
                domain: domain("sso.acme.com"),
                kind: SignalKind::parse("certificate_seen").unwrap(),
                value: Some("sso.acme.com".to_owned()),
                raw: json!({ "host": "sso.acme.com" }),
                provenance: Provenance::parse("https://crt.sh/?q=sso.acme.com").unwrap(),
                strength: UnitInterval::new(0.3).unwrap(),
                observed_at: at(10),
                agent: None,
            }],
            at(10),
        )
        .await
        .unwrap();
    let source = AgentSource::named(SourceName::HttpProbe);

    let report = usecase.run_source(&source, &FixedClock).await.unwrap();

    assert_eq!(report.runs.len(), 2);
    assert_eq!(report.signals, 1);
    assert_eq!(report.skipped_no_lead, 3);
    let recorded = runs(&pool).await;
    assert!(recorded.iter().all(|run| run.agent.is_none()));
    assert!(
        recorded
            .iter()
            .all(|run| run.source == SourceName::HttpProbe && run.volume == 2)
    );
    let found = signals_of(&pool, first, "acme.com").await;
    let kinds: Vec<(&str, Option<&AgentId>)> = found
        .iter()
        .map(|signal| (signal.kind.as_str(), signal.agent.as_ref()))
        .collect();
    assert_eq!(kinds.len(), 2);
    assert!(kinds.contains(&("product_detected", None)));
    assert!(!has_lead(&pool, first, "beta.io").await);
    assert!(!has_lead(&pool, second, "acme.com").await);
    assert!(!has_lead(&pool, second, "beta.io").await);
}
