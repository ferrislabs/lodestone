mod support;

use std::{
    num::{NonZeroU32, NonZeroUsize},
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, Utc};
use common::{CoreError, EncryptionKey, Secret};
use lodestone_core::{
    Clock, CreateProfileCommand, GenerationParams, LlmError, LlmProvider, LlmRequest, LlmResponse,
    LodestoneUseCase, NO_DECIDED_LEAD, NewTrial, Observation, ProfileId, ProfileName, Provenance,
    ProviderDefinition, ProviderId, ProviderKind, SampleSize, SignalKind, SourceName, Tier,
    TokenPricing, TokenUsage, TrialBudget, TrialId, TrialLabel, TrialStatus, UnitInterval,
    ValidatedProfile, Verdict,
    evaluation::{EvaluationSettings, ProviderBuilder},
    infrastructure::{crypto::AesGcmCipher, llm_cache::postgres::PgLlmCache},
    llm::CreateLlmProvider,
    trial::{PROFILE_TRIAL_JOB_KIND, trial_job, trial_of_job},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{at, domain};
use uuid::Uuid;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const CLASSIFIER: &str = "model-class";
const SYNTHESIS: &str = "model-synth";
const HOSTS: [&str; 4] = ["alpha.com", "bravo.com", "charlie.com", "delta.com"];
const VALIDATED: [&str; 2] = ["alpha.com", "bravo.com"];
const REJECTED: [&str; 2] = ["charlie.com", "delta.com"];
const DRAFT_REJECTS: [&str; 2] = ["charlie.com", "delta.com"];
const TABLES: [&str; 8] = [
    "leads",
    "signals",
    "qualification_results",
    "scoring_results",
    "lead_events",
    "lead_qualification_state",
    "lead_agent_retries",
    "decision_labels",
];

struct TestClock;

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        at(2_000)
    }
}

#[derive(Debug, Clone)]
struct Seen {
    draft: bool,
    scoring: bool,
}

struct Shared {
    seen: Mutex<Vec<Seen>>,
    failing: Mutex<Vec<String>>,
    input_tokens: u64,
}

struct Scripted {
    model: String,
    shared: Arc<Shared>,
}

struct FakeBuilder {
    shared: Arc<Shared>,
}

impl ProviderBuilder for FakeBuilder {
    type Provider = Scripted;

    fn build(&self, definition: &ProviderDefinition) -> Result<Scripted, LlmError> {
        Ok(Scripted {
            model: definition.model.clone(),
            shared: Arc::clone(&self.shared),
        })
    }
}

fn first_line(request: &LlmRequest<'_>) -> String {
    request
        .documents
        .first()
        .and_then(|document| document.content.lines().next())
        .unwrap_or_default()
        .to_owned()
}

fn qualification_answer(schema: &Value, quote: &str, worst: bool) -> Value {
    let mut answers = serde_json::Map::new();
    for (id, question) in schema["properties"]["answers"]["properties"]
        .as_object()
        .unwrap()
    {
        let all = question["properties"]["choice"]["enum"].as_array().unwrap();
        let known: Vec<&Value> = all
            .iter()
            .filter(|value| value.as_str() != Some("inconnu"))
            .collect();
        let chosen = if worst {
            known[known.len() - 1]
        } else {
            known[0]
        };
        let probabilities: serde_json::Map<String, Value> = all
            .iter()
            .map(|value| {
                (
                    value.as_str().unwrap().to_owned(),
                    json!(if value == chosen { 1.0 } else { 0.0 }),
                )
            })
            .collect();
        answers.insert(
            id.clone(),
            json!({
                "type": "choice",
                "choice": chosen,
                "evidence": quote,
                "confidence": 0.9,
                "probabilities": probabilities
            }),
        );
    }
    json!({"summary": "Observations de l'agent sur le lead.", "answers": answers})
}

impl LlmProvider for Scripted {
    async fn complete(&self, request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        let scoring = request.schema["properties"].get("criteria").is_some();
        let draft = request.system.contains("DRAFT");
        let quote = first_line(request);
        self.shared
            .seen
            .lock()
            .unwrap()
            .push(Seen { draft, scoring });
        if draft
            && self
                .shared
                .failing
                .lock()
                .unwrap()
                .iter()
                .any(|host| quote.contains(host.as_str()))
        {
            return Err(LlmError::Timeout);
        }
        let worst = draft && DRAFT_REJECTS.iter().any(|host| quote.contains(host));
        let output = if scoring {
            support::scoring_answer(request.schema, &quote, worst)
        } else {
            qualification_answer(request.schema, &quote, worst)
        };
        Ok(LlmResponse {
            output,
            model: self.model.clone(),
            usage: TokenUsage {
                input_tokens: self.shared.input_tokens,
                output_tokens: 0,
            },
        })
    }
}

fn settings() -> EvaluationSettings {
    EvaluationSettings {
        max_tokens: 512,
        budget_cents: NonZeroU32::new(100).unwrap(),
        batch_size: 10,
        base_delay: Duration::ZERO,
        concurrency: NonZeroUsize::MIN,
    }
}

fn question() -> Value {
    json!({
        "id": "q",
        "wording": "Pertinent ?",
        "weight": 5,
        "options": [
            {"value": "forte", "label": "Haut", "score": 1.0},
            {"value": "moyenne", "label": "Moyen", "score": 0.5},
            {"value": "faible", "label": "Bas", "score": 0.0},
            {"value": "inconnu", "label": "Inconnu", "score": 0.5, "is_unknown": true}
        ]
    })
}

fn qualification_agent(id: &str, prompt: &str) -> Value {
    json!({
        "id": id,
        "name": id,
        "description": format!("DESCRIPTION-OF-{id}"),
        "enabled": true,
        "provider_id": null,
        "system_prompt": prompt,
        "samples": 1,
        "questions": [question()]
    })
}

fn scoring_agent(id: &str, prompt: &str) -> Value {
    let mut criterion = question();
    criterion["id"] = json!("adequation");
    json!({
        "id": id,
        "name": id,
        "description": format!("DESCRIPTION-OF-{id}"),
        "enabled": true,
        "provider_id": null,
        "weight": 5,
        "system_prompt": prompt,
        "criteria": [criterion],
        "samples": 1,
        "sampling_temperature": 0.7
    })
}

fn definition(qualification: Vec<Value>, scoring: Vec<Value>) -> Value {
    let mut definition: Value = serde_json::from_str(IAM).unwrap();
    definition["qualification_agents"] = Value::Array(qualification);
    definition["scoring_agents"] = Value::Array(scoring);
    definition["aggregator"]["system_prompt"] = json!("AGGREGATOR-PROMPT");
    definition["aggregator"]["provider_id"] = Value::Null;
    definition
}

fn live() -> Value {
    definition(
        vec![qualification_agent("fit", "PROMPT-OF-fit")],
        vec![scoring_agent("global", "SCORING-PROMPT-OF-global")],
    )
}

fn draft() -> Value {
    definition(
        vec![qualification_agent("fit", "DRAFT-PROMPT-OF-fit")],
        vec![scoring_agent("global-v2", "DRAFT-SCORING-PROMPT")],
    )
}

fn cipher() -> AesGcmCipher {
    let encoded = format!("{}QkI=", "QkJC".repeat(10));
    AesGcmCipher::new(&EncryptionKey::from_base64(&Secret::new(encoded)).unwrap()).unwrap()
}

struct Harness {
    pool: PgPool,
    usecase: LodestoneUseCase,
    cipher: AesGcmCipher,
    shared: Arc<Shared>,
    profile: ProfileId,
}

impl Harness {
    async fn new(pool: PgPool, input_tokens: u64, price: u32) -> Self {
        let usecase = LodestoneUseCase::new(pool.clone());
        let cipher = cipher();
        for (model, tier) in [(CLASSIFIER, Tier::Triage), (SYNTHESIS, Tier::Qualification)] {
            let provider = provider(&usecase, &cipher, model, price).await;
            usecase.assign_llm_tier(tier, provider).await.unwrap();
        }
        let profile = usecase
            .create_profile(CreateProfileCommand {
                name: ProfileName::parse("iam").unwrap(),
                definition: ValidatedProfile::parse(live()).unwrap(),
                created_by: None,
            })
            .await
            .unwrap()
            .profile
            .id;
        usecase.set_profile_active(profile, true).await.unwrap();
        Self {
            usecase,
            pool,
            cipher,
            shared: Arc::new(Shared {
                seen: Mutex::new(Vec::new()),
                failing: Mutex::new(Vec::new()),
                input_tokens,
            }),
            profile,
        }
    }

    fn builder(&self) -> FakeBuilder {
        FakeBuilder {
            shared: Arc::clone(&self.shared),
        }
    }

    fn cache(&self) -> PgLlmCache {
        PgLlmCache::new(self.pool.clone())
    }

    async fn observe(&self, host: &str) {
        let active = self.usecase.list_active_profiles().await.unwrap().remove(0);
        self.usecase
            .record_observations(
                &active,
                SourceName::HttpProbe,
                &[Observation {
                    domain: domain(host),
                    kind: SignalKind::parse("product_detected").unwrap(),
                    value: Some("keycloak-http_probe".to_owned()),
                    raw: json!({"version": "21.1"}),
                    provenance: Provenance::parse(&format!("https://{host}/keycloak")).unwrap(),
                    strength: UnitInterval::new(0.9).unwrap(),
                    observed_at: at(10),
                    agent: None,
                }],
            )
            .await
            .unwrap();
    }

    async fn lead(&self, host: &str) -> Uuid {
        sqlx::query_scalar("SELECT id FROM leads WHERE profile_id = $1 AND domain = $2")
            .bind(self.profile.0)
            .bind(host)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn label(&self, host: &str, kind: &str, seconds: i64) {
        let lead = self.lead(host).await;
        sqlx::query(
            "INSERT INTO decision_labels (id, lead_id, profile_id, profile_revision, kind, decided_at, snapshot)
             VALUES ($1, $2, $3, 1, $4, $5, $6)",
        )
        .bind(Uuid::now_v7())
        .bind(lead)
        .bind(self.profile.0)
        .bind(kind)
        .bind(at(seconds))
        .bind(json!({
            "confidence": null, "opinion": null, "evidence": null,
            "verdict": null, "verdict_reason": null, "agents": {}
        }))
        .execute(&self.pool)
        .await
        .unwrap();
    }

    async fn decided(pool: PgPool, input_tokens: u64, price: u32) -> Self {
        let harness = Self::new(pool, input_tokens, price).await;
        for host in HOSTS {
            harness.observe(host).await;
        }
        harness
            .usecase
            .qualify_due_leads(
                &harness.builder(),
                &harness.cipher,
                &harness.cache(),
                settings(),
                &TestClock,
            )
            .await
            .unwrap();
        harness
            .usecase
            .score_due_leads(
                &harness.builder(),
                &harness.cipher,
                &harness.cache(),
                settings(),
                &TestClock,
            )
            .await
            .unwrap();
        for (position, host) in VALIDATED.into_iter().enumerate() {
            harness
                .label(host, "lead_validated", 100 + position as i64)
                .await;
        }
        for (position, host) in REJECTED.into_iter().enumerate() {
            harness
                .label(host, "lead_rejected", 200 + position as i64)
                .await;
        }
        harness.shared.seen.lock().unwrap().clear();
        harness
    }

    async fn create(&self, sample: u32, budget: u32) -> Result<TrialId, CoreError> {
        self.create_at(sample, budget, 1_500).await
    }

    async fn create_at(
        &self,
        sample: u32,
        budget: u32,
        seconds: i64,
    ) -> Result<TrialId, CoreError> {
        self.usecase
            .create_profile_trial(
                NewTrial {
                    profile_id: self.profile,
                    definition: ValidatedProfile::parse(draft()).unwrap(),
                    sample_size: SampleSize::new(sample).unwrap(),
                    budget: TrialBudget::new(budget).unwrap(),
                    created_by: None,
                },
                at(seconds),
            )
            .await
            .map(|trial| trial.id)
    }

    async fn run(&self, id: TrialId) -> TrialStatus {
        self.usecase
            .run_profile_trial(
                id,
                &self.builder(),
                &self.cipher,
                &self.cache(),
                settings(),
                &TestClock,
            )
            .await
            .unwrap()
    }

    fn calls(&self) -> Vec<Seen> {
        self.shared.seen.lock().unwrap().clone()
    }

    async fn snapshot(&self) -> Vec<(&'static str, Vec<Value>)> {
        let mut tables = Vec::new();
        for table in TABLES {
            let rows: Vec<Value> = sqlx::query_scalar(&format!(
                "SELECT to_jsonb(t) FROM {table} t ORDER BY to_jsonb(t)::text"
            ))
            .fetch_all(&self.pool)
            .await
            .unwrap();
            tables.push((table, rows));
        }
        tables
    }
}

async fn provider(
    usecase: &LodestoneUseCase,
    cipher: &AesGcmCipher,
    model: &str,
    price: u32,
) -> ProviderId {
    usecase
        .create_llm_provider(
            CreateLlmProvider {
                name: model.to_owned(),
                kind: ProviderKind::OpenAiCompatible,
                base_url: "https://llm.example/v1".parse().unwrap(),
                model: model.to_owned(),
                timeout: Duration::from_secs(30),
                max_retries: 0,
                max_concurrency: None,
                pricing: TokenPricing {
                    input_cents_per_mtok: price,
                    output_cents_per_mtok: 0,
                },
                api_key: None,
                reasoning_effort: None,
                generation: GenerationParams::default(),
            },
            cipher,
        )
        .await
        .unwrap()
        .id
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_trial_gives_the_draft_verdicts_and_compares_them_with_the_current_ones(pool: PgPool) {
    let harness = Harness::decided(pool, 10, 0).await;
    let id = harness.create(20, 200).await.unwrap();

    let status = harness.run(id).await;

    assert_eq!(status, TrialStatus::Done);
    let report = harness.usecase.profile_trial_report(id).await.unwrap();
    assert_eq!(report.trial.status, TrialStatus::Done);
    assert_eq!(report.trial.leads_tried, 4);
    assert_eq!(report.trial.spent_cents, 0.0);
    assert!(report.trial.started_at.is_some());
    assert!(report.trial.finished_at.is_some());
    assert_eq!(report.leads.len(), 4);
    for lead in &report.leads {
        let host = lead.domain.as_str();
        let rejected = REJECTED.contains(&host);
        assert_eq!(
            lead.label,
            if rejected {
                TrialLabel::Rejected
            } else {
                TrialLabel::Validated
            },
            "{host}"
        );
        assert_eq!(lead.current_verdict, Some(Verdict::Reliable), "{host}");
        assert_eq!(
            lead.trial_verdict,
            Some(if rejected {
                Verdict::Discarded
            } else {
                Verdict::Reliable
            }),
            "{host}"
        );
        assert!(lead.trial_confidence.is_some(), "{host}");
        assert_eq!(lead.error, None, "{host}");
    }
    let summary = report.summary;
    assert_eq!(summary.current.reliable, 4);
    assert_eq!(summary.current.reliable_validated, 2);
    assert_eq!(summary.current.precision(), Some(0.5));
    assert_eq!(summary.current.rejected, 2);
    assert_eq!(summary.current.rejected_discarded, 0);
    assert_eq!(summary.trial.reliable, 2);
    assert_eq!(summary.trial.reliable_validated, 2);
    assert_eq!(summary.trial.precision(), Some(1.0));
    assert_eq!(summary.trial.rejected, 2);
    assert_eq!(summary.trial.rejected_discarded, 2);
    assert_eq!(summary.changed, 2);
    assert_eq!(summary.errors, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_trial_calls_the_draft_agents_and_writes_nothing_to_the_leads(pool: PgPool) {
    let harness = Harness::decided(pool, 10, 0).await;
    let before = harness.snapshot().await;
    for (table, rows) in &before {
        assert!(
            *table == "lead_agent_retries" || !rows.is_empty(),
            "{table}"
        );
    }
    let id = harness.create(20, 200).await.unwrap();

    harness.run(id).await;

    let calls = harness.calls();
    assert_eq!(calls.len(), 8);
    assert!(calls.iter().all(|call| call.draft));
    assert_eq!(calls.iter().filter(|call| call.scoring).count(), 4);
    assert_eq!(harness.snapshot().await, before);
}

#[sqlx::test(migrations = "../../migrations")]
async fn creating_a_trial_queues_one_deduplicated_job(pool: PgPool) {
    let harness = Harness::decided(pool.clone(), 10, 0).await;

    let id = harness.create(20, 200).await.unwrap();

    let rows: Vec<(String, Value, Option<String>, i32, String)> = sqlx::query_as(
        "SELECT kind, payload, dedup_key, max_attempts, status FROM jobs WHERE kind = $1",
    )
    .bind(PROFILE_TRIAL_JOB_KIND)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, json!({ "trial_id": id.to_string() }));
    assert_eq!(rows[0].2, Some(format!("trial:{id}")));
    assert_eq!(rows[0].3, 3);
    assert_eq!(rows[0].4, "pending");
    let job = trial_job(id, at(0)).unwrap();
    assert_eq!(job.dedup_key, rows[0].2);
    let report = harness.usecase.profile_trial_report(id).await.unwrap();
    assert_eq!(report.trial.status, TrialStatus::Pending);
    assert_eq!(report.trial.base_revision.get(), 1);
    assert_eq!(report.trial.sample_size.get(), 20);
    assert_eq!(report.trial.budget.cents().get(), 200);
    assert!(report.leads.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_trial_job_carries_its_trial(pool: PgPool) {
    let harness = Harness::decided(pool.clone(), 10, 0).await;
    let id = harness.create(20, 200).await.unwrap();
    let payload: Value = sqlx::query_scalar("SELECT payload FROM jobs WHERE kind = $1")
        .bind(PROFILE_TRIAL_JOB_KIND)
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut job = lodestone_core::Job {
        id: lodestone_core::JobId(Uuid::now_v7()),
        kind: lodestone_core::JobKind::parse(PROFILE_TRIAL_JOB_KIND).unwrap(),
        payload,
        status: lodestone_core::JobStatus::Pending,
        attempts: 0,
        max_attempts: std::num::NonZeroU16::new(3).unwrap(),
        run_at: at(0),
        last_error: None,
        created_at: at(0),
        updated_at: at(0),
    };

    assert_eq!(trial_of_job(&job), Ok(id));
    job.payload = json!({});
    assert!(trial_of_job(&job).is_err());
    job.payload = json!({ "trial_id": "nope" });
    assert!(trial_of_job(&job).is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_second_live_trial_is_refused_until_the_first_ends(pool: PgPool) {
    let harness = Harness::decided(pool, 10, 0).await;
    let first = harness.create(20, 200).await.unwrap();

    let refused = harness.create(20, 200).await;

    assert!(
        matches!(refused, Err(CoreError::Conflict(_))),
        "{refused:?}"
    );
    harness.run(first).await;
    let second = harness.create(20, 200).await;
    assert!(second.is_ok(), "{second:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_trial_needs_a_decided_lead(pool: PgPool) {
    let harness = Harness::new(pool, 10, 0).await;
    harness.observe("alpha.com").await;

    let refused = harness.create(20, 200).await;

    match refused {
        Err(CoreError::Conflict(message)) => assert_eq!(message, NO_DECIDED_LEAD),
        other => panic!("{other:?}"),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_trial_of_an_unknown_profile_is_not_found(pool: PgPool) {
    let harness = Harness::decided(pool, 10, 0).await;

    let result = harness
        .usecase
        .create_profile_trial(
            NewTrial {
                profile_id: ProfileId(Uuid::now_v7()),
                definition: ValidatedProfile::parse(draft()).unwrap(),
                sample_size: SampleSize::DEFAULT,
                budget: TrialBudget::DEFAULT,
                created_by: None,
            },
            at(1_500),
        )
        .await;

    assert!(matches!(result, Err(CoreError::NotFound)), "{result:?}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_small_sample_keeps_the_balance_between_validated_and_rejected_leads(pool: PgPool) {
    let harness = Harness::decided(pool, 10, 0).await;
    let id = harness.create(2, 200).await.unwrap();

    harness.run(id).await;

    let report = harness.usecase.profile_trial_report(id).await.unwrap();
    let mut labels: Vec<TrialLabel> = report.leads.iter().map(|lead| lead.label).collect();
    labels.sort_by_key(|label| label.as_str());
    assert_eq!(labels, [TrialLabel::Rejected, TrialLabel::Validated]);
    let newest_validated = report
        .leads
        .iter()
        .find(|lead| lead.label == TrialLabel::Validated)
        .unwrap();
    assert_eq!(newest_validated.domain.as_str(), "bravo.com");
    let newest_rejected = report
        .leads
        .iter()
        .find(|lead| lead.label == TrialLabel::Rejected)
        .unwrap();
    assert_eq!(newest_rejected.domain.as_str(), "delta.com");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_trial_stops_at_its_budget_and_keeps_the_leads_it_tried(pool: PgPool) {
    let harness = Harness::decided(pool, 10_000, 100).await;
    let id = harness.create(20, 3).await.unwrap();

    let status = harness.run(id).await;

    assert_eq!(status, TrialStatus::BudgetExhausted);
    let report = harness.usecase.profile_trial_report(id).await.unwrap();
    assert_eq!(report.trial.status, TrialStatus::BudgetExhausted);
    assert!(report.trial.finished_at.is_some());
    assert_eq!(report.trial.leads_tried, 1);
    assert_eq!(report.leads.len(), 1);
    assert!(report.leads[0].trial_verdict.is_some());
    let calls = harness.calls().len();
    assert!(calls < 8);
    assert!(report.trial.spent_cents >= 3.0);
    assert!((report.trial.spent_cents - calls as f64).abs() < 1e-6);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_replay_adds_its_spend_to_what_the_trial_already_spent(pool: PgPool) {
    let harness = Harness::decided(pool.clone(), 10_000, 100).await;
    let id = harness.create(20, 200).await.unwrap();
    sqlx::query(
        "UPDATE profile_trials SET status = 'running', started_at = $2, spent_cents = 5.0 WHERE id = $1",
    )
    .bind(id.0)
    .bind(at(1_600))
    .execute(&pool)
    .await
    .unwrap();

    let status = harness.run(id).await;

    assert_eq!(status, TrialStatus::Done);
    let calls = harness.calls().len();
    assert_eq!(calls, 8);
    let report = harness.usecase.profile_trial_report(id).await.unwrap();
    assert!((report.trial.spent_cents - (5.0 + calls as f64)).abs() < 1e-6);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_replay_resumes_a_running_trial_and_skips_the_leads_already_tried(pool: PgPool) {
    let harness = Harness::decided(pool.clone(), 10, 0).await;
    let id = harness.create(20, 200).await.unwrap();
    let kept = harness.lead("alpha.com").await;
    sqlx::query(
        "UPDATE profile_trials SET status = 'running', started_at = $2, spent_cents = 1.5 WHERE id = $1",
    )
        .bind(id.0)
        .bind(at(1_600))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO profile_trial_leads (trial_id, lead_id, label, current_verdict, current_confidence, trial_verdict, trial_confidence, error, tried_at)
         VALUES ($1, $2, 'validated', 'fiable', 80.0, 'a_verifier', 55.5, NULL, $3)",
    )
    .bind(id.0)
    .bind(kept)
    .bind(at(1_601))
    .execute(&pool)
    .await
    .unwrap();

    let status = harness.run(id).await;

    assert_eq!(status, TrialStatus::Done);
    assert_eq!(harness.calls().len(), 6);
    let report = harness.usecase.profile_trial_report(id).await.unwrap();
    assert_eq!(report.leads.len(), 4);
    let skipped = report
        .leads
        .iter()
        .find(|lead| lead.lead_id.0 == kept)
        .unwrap();
    assert_eq!(skipped.trial_verdict, Some(Verdict::ToCheck));
    assert_eq!(skipped.trial_confidence, Some(55.5));
    assert_eq!(report.trial.started_at, Some(at(1_600)));
    assert_eq!(report.trial.spent_cents, 1.5);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_finished_trial_is_left_alone_when_its_job_runs_again(pool: PgPool) {
    let harness = Harness::decided(pool, 10, 0).await;
    let id = harness.create(20, 200).await.unwrap();
    harness.run(id).await;
    let calls = harness.calls().len();
    let before = harness.usecase.profile_trial_report(id).await.unwrap();

    let status = harness.run(id).await;

    assert_eq!(status, TrialStatus::Done);
    assert_eq!(harness.calls().len(), calls);
    assert_eq!(
        harness.usecase.profile_trial_report(id).await.unwrap(),
        before
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_model_call_marks_the_lead_and_the_trial_goes_on(pool: PgPool) {
    let harness = Harness::decided(pool, 10, 0).await;
    harness
        .shared
        .failing
        .lock()
        .unwrap()
        .push("delta.com".to_owned());
    let id = harness.create(20, 200).await.unwrap();

    let status = harness.run(id).await;

    assert_eq!(status, TrialStatus::Done);
    let report = harness.usecase.profile_trial_report(id).await.unwrap();
    assert_eq!(report.leads.len(), 4);
    let failed = report
        .leads
        .iter()
        .find(|lead| lead.domain.as_str() == "delta.com")
        .unwrap();
    assert!(failed.error.is_some());
    assert_eq!(failed.trial_verdict, None);
    assert_eq!(failed.current_verdict, Some(Verdict::Reliable));
    assert_eq!(report.summary.errors, 1);
    assert_eq!(report.summary.current.rejected, 1);
    assert_eq!(report.summary.trial.rejected, 1);
    assert_eq!(
        report
            .leads
            .iter()
            .filter(|lead| lead.error.is_none() && lead.trial_verdict.is_some())
            .count(),
        3
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_trial_can_be_failed_once_and_never_after_it_ended(pool: PgPool) {
    let harness = Harness::decided(pool, 10, 0).await;
    let pending = harness.create(20, 200).await.unwrap();

    harness
        .usecase
        .fail_profile_trial(pending, "no LLM provider is configured", at(1_700))
        .await
        .unwrap();

    let report = harness.usecase.profile_trial_report(pending).await.unwrap();
    assert_eq!(report.trial.status, TrialStatus::Failed);
    assert_eq!(
        report.trial.error.as_deref(),
        Some("no LLM provider is configured")
    );
    assert_eq!(report.trial.finished_at, Some(at(1_700)));
    assert_eq!(harness.run(pending).await, TrialStatus::Failed);
    assert!(harness.calls().is_empty());
    let finished = harness.create(20, 200).await.unwrap();
    harness.run(finished).await;
    harness
        .usecase
        .fail_profile_trial(finished, "late failure", at(1_800))
        .await
        .unwrap();
    let after = harness
        .usecase
        .profile_trial_report(finished)
        .await
        .unwrap();
    assert_eq!(after.trial.status, TrialStatus::Done);
    assert_eq!(after.trial.error, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_trials_of_a_profile_are_listed_newest_first(pool: PgPool) {
    let harness = Harness::decided(pool, 10, 0).await;
    let first = harness.create(20, 200).await.unwrap();
    harness.run(first).await;
    let second = harness.create_at(5, 100, 1_600).await.unwrap();

    let listed = harness
        .usecase
        .list_profile_trials(harness.profile)
        .await
        .unwrap();

    assert_eq!(
        listed.iter().map(|trial| trial.id).collect::<Vec<_>>(),
        [second, first]
    );
    assert_eq!(listed[0].status, TrialStatus::Pending);
    assert_eq!(listed[1].leads_tried, 4);
    let unknown = harness
        .usecase
        .list_profile_trials(ProfileId(Uuid::now_v7()))
        .await;
    assert!(matches!(unknown, Err(CoreError::NotFound)));
}
