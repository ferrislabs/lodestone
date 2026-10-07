mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, Clock, CreateProfileCommand, LeadRepository, LodestoneUseCase, Observation,
    ProfileId, ProfileName, Provenance, RunRepository, SignalKind, SignalRepository, Source,
    SourceError, SourceName, UnitInterval, ValidatedProfile,
    definition::{Agent, AgentId},
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
}

impl AgentSource {
    fn new() -> Self {
        Self {
            collects: AtomicUsize::new(0),
        }
    }
}

impl Source for AgentSource {
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
        Ok(vec!["sso.acme.com", "auth.beta.io"])
    }

    fn serves(&self, item: &Self::Item, agent: &Agent) -> bool {
        agent.id().as_str() != "probe-b" || item.contains("beta")
    }

    fn evaluate(
        &self,
        item: &Self::Item,
        _profile: &ActiveProfile,
        _agent: &Agent,
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
        "type": "signal",
        "id": id,
        "name": id,
        "enabled": enabled,
        "source": {
            "provider": "http_probe",
            "fingerprints": [{
                "id": "kc",
                "path": "/realms/master",
                "product": "keycloak",
                "matchers": [{"kind": "status", "equals": 200}]
            }]
        }
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
