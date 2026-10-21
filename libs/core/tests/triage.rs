mod support;

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, Candidate, Clock, CreateProfileCommand, LeadRepository, LodestoneUseCase,
    NormalizedDomain, ProfileId, ProfileName, Provenance, RunRepository, RunStatus, SignalKind,
    SignalRepository, SourceError, SourceName, SuppressionTarget, ValidatedProfile,
    definition::AgentId,
    infrastructure::{
        lead::postgres::PgLeadRepository, postgres::with_tx, run::postgres::PgRunRepository,
        signal::postgres::PgSignalRepository,
    },
    triage::CandidateSource,
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

struct FakeSource {
    candidates: Vec<Candidate>,
}

impl CandidateSource for FakeSource {
    fn name(&self) -> SourceName {
        SourceName::Boamp
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        Ok(self.candidates.clone())
    }
}

fn candidate(host: &str, title: &str) -> Candidate {
    Candidate {
        domain: domain(host),
        source: SourceName::Boamp,
        kind: SignalKind::parse("public_tender").unwrap(),
        title: title.to_owned(),
        text: "Avis de marche public".to_owned(),
        provenance: Provenance::parse(&format!("https://marches.example/avis/{host}")).unwrap(),
        observed_at: at(50),
        details: json!({"idweb": host}),
    }
}

async fn activate(pool: &PgPool, name: &str, definition: Value) -> ProfileId {
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

async fn iam_profile(pool: &PgPool) -> ProfileId {
    activate(pool, "iam", serde_json::from_str(IAM).unwrap()).await
}

async fn run_triage(pool: &PgPool, source: &FakeSource) -> lodestone_core::triage::TriageReport {
    LodestoneUseCase::new(pool.clone())
        .run_triage(source, &FixedClock)
        .await
        .unwrap()
}

async fn signals(pool: &PgPool, profile: ProfileId, host: &str) -> Vec<lodestone_core::Signal> {
    with_tx(pool, async |shared| {
        let Some(lead) = PgLeadRepository::new(&shared)
            .find_by_domain(profile, &NormalizedDomain::from_host(host).unwrap())
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

#[sqlx::test(migrations = "../../migrations")]
async fn ambiguous_candidates_become_leads_without_any_llm_call(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let source = FakeSource {
        candidates: vec![
            candidate("mairie.fr", "Mise en place d'un SSO avec MFA"),
            candidate("meubles.fr", "Fourniture de mobilier de bureau"),
            candidate("acme.com", "Badge d'acces et SSO"),
            candidate("region.fr", "Projet IAM pour la region"),
        ],
    };

    let report = run_triage(&pool, &source).await;

    assert_eq!(report.candidates, 4);
    assert_eq!(report.accepted_by_rules, 1);
    assert_eq!(report.accepted_ambiguous, 2);
    assert_eq!(report.rejected_by_rules, 1);
    assert_eq!(report.accepted(), 3);

    let accepted = signals(&pool, profile, "mairie.fr").await;
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].source, SourceName::Boamp);
    assert_eq!(accepted[0].raw["route"], "keywords");
    assert_eq!(accepted[0].raw["details"]["idweb"], "mairie.fr");
    assert_eq!(accepted[0].strength.get(), 0.7);
    assert!(signals(&pool, profile, "meubles.fr").await.is_empty());

    let ambiguous = signals(&pool, profile, "acme.com").await;
    assert_eq!(ambiguous.len(), 1);
    assert_eq!(ambiguous[0].raw["route"], "keywords_ambiguous");
    assert_eq!(ambiguous[0].raw["matched"], json!(["SSO"]));
    assert_eq!(ambiguous[0].raw["excluded"], json!(["badge"]));
    assert_eq!(ambiguous[0].raw["details"]["idweb"], "acme.com");
    assert_eq!(ambiguous[0].strength.get(), 0.5);
    assert!(ambiguous[0].strength.get() < accepted[0].strength.get());

    let below_threshold = signals(&pool, profile, "region.fr").await;
    assert_eq!(below_threshold.len(), 1);
    assert_eq!(below_threshold[0].raw["route"], "keywords_ambiguous");
    assert_eq!(below_threshold[0].raw["excluded"], json!([]));

    let recorded = runs(&pool).await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].status, RunStatus::Succeeded);
    assert_eq!(recorded[0].volume, 4);
    assert_eq!(recorded[0].signals, 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_second_run_does_not_duplicate_signals(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let source = FakeSource {
        candidates: vec![candidate("acme.com", "Badge d'acces et SSO")],
    };

    run_triage(&pool, &source).await;
    run_triage(&pool, &source).await;

    assert_eq!(signals(&pool, profile, "acme.com").await.len(), 1);
    assert_eq!(runs(&pool).await.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn suppressed_domains_record_nothing(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    usecase
        .add_suppression(SuppressionTarget::Domain(domain("bad.fr")), None, None)
        .await
        .unwrap();
    let source = FakeSource {
        candidates: vec![
            candidate("bad.fr", "Badge et SSO pour bad"),
            candidate("good.fr", "Badge et SSO pour good"),
        ],
    };

    run_triage(&pool, &source).await;

    assert!(signals(&pool, profile, "bad.fr").await.is_empty());
    assert_eq!(signals(&pool, profile, "good.fr").await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn profiles_route_the_same_candidate_with_their_own_rules(pool: PgPool) {
    let strict = iam_profile(&pool).await;
    let mut lenient_definition: Value = serde_json::from_str(IAM).unwrap();
    let boamp = lenient_definition["agents"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|agent| agent["source"]["kind"] == "boamp")
        .unwrap();
    boamp["source"] = json!({"kind": "boamp", "keywords": ["SSO"], "accept_min_keywords": 1});
    let lenient = activate(&pool, "lenient", lenient_definition).await;
    let source = FakeSource {
        candidates: vec![candidate("ville.fr", "Projet SSO pour la ville")],
    };

    let report = run_triage(&pool, &source).await;

    assert_eq!(report.accepted_by_rules, 1);
    assert_eq!(report.accepted_ambiguous, 1);
    let ambiguous = signals(&pool, strict, "ville.fr").await;
    assert_eq!(ambiguous.len(), 1);
    assert_eq!(ambiguous[0].raw["route"], "keywords_ambiguous");
    let accepted = signals(&pool, lenient, "ville.fr").await;
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].raw["route"], "keywords");
    assert_eq!(runs(&pool).await.len(), 2);
}

fn boamp_agent(id: &str, enabled: bool, keywords: &[&str], min: u8) -> Value {
    json!({
        "id": id,
        "name": id,
        "enabled": enabled,
        "source": {"kind": "boamp", "keywords": keywords, "accept_min_keywords": min},
    })
}

async fn boamp_profile(pool: &PgPool, agents: Vec<Value>) -> ProfileId {
    let mut definition: Value = serde_json::from_str(IAM).unwrap();
    definition["agents"] = Value::Array(agents);
    activate(pool, "boamp-agents", definition).await
}

fn agent_ids(runs: &[lodestone_core::Run]) -> Vec<String> {
    let mut ids: Vec<String> = runs
        .iter()
        .filter_map(|run| run.agent.as_ref().map(|agent| agent.as_str().to_owned()))
        .collect();
    ids.sort();
    ids
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_of_the_same_kind_route_the_same_candidate_differently(pool: PgPool) {
    let profile = boamp_profile(
        &pool,
        vec![
            boamp_agent("boamp-fr", true, &["SSO"], 1),
            boamp_agent("boamp-en", true, &["single sign-on"], 1),
        ],
    )
    .await;
    let source = FakeSource {
        candidates: vec![candidate("ville.fr", "Projet SSO pour la ville")],
    };

    let report = run_triage(&pool, &source).await;

    assert_eq!(report.candidates, 1);
    assert_eq!(report.accepted_by_rules, 1);
    assert_eq!(report.rejected_by_rules, 1);
    let recorded = signals(&pool, profile, "ville.fr").await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].agent, Some(AgentId::new("boamp-fr")));
    let runs = runs(&pool).await;
    assert_eq!(agent_ids(&runs), ["boamp-en", "boamp-fr"]);
    for run in &runs {
        let accepted = run.agent == Some(AgentId::new("boamp-fr"));
        assert_eq!(run.signals, u64::from(accepted));
        assert_eq!(run.volume, 1);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_disabled_agent_is_neither_run_nor_recorded(pool: PgPool) {
    let profile = boamp_profile(
        &pool,
        vec![
            boamp_agent("boamp-fr", false, &["SSO"], 1),
            boamp_agent("boamp-en", true, &["single sign-on"], 1),
        ],
    )
    .await;
    let source = FakeSource {
        candidates: vec![candidate("ville.fr", "Projet SSO pour la ville")],
    };

    run_triage(&pool, &source).await;

    assert!(signals(&pool, profile, "ville.fr").await.is_empty());
    assert_eq!(agent_ids(&runs(&pool).await), ["boamp-en"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_profile_without_an_enabled_agent_of_the_kind_gets_no_run(pool: PgPool) {
    boamp_profile(&pool, vec![boamp_agent("boamp-fr", false, &["SSO"], 1)]).await;
    let source = FakeSource {
        candidates: vec![candidate("ville.fr", "Projet SSO pour la ville")],
    };

    let report = run_triage(&pool, &source).await;

    assert_eq!(report.candidates, 0);
    assert!(runs(&pool).await.is_empty());
}
