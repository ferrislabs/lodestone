mod support;

use std::{
    collections::{HashMap, VecDeque},
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, Utc};
use common::{EncryptionKey, Secret};
use lodestone_core::{
    ActiveProfile, Clock, CreateProfileCommand, Lead, LeadRepository, LlmError, LlmProvider,
    LlmRequest, LlmResponse, LodestoneUseCase, Observation, ProfileId, ProfileName, Provenance,
    ProviderDefinition, ProviderId, ProviderKind, QualificationResult, ResultStatus,
    ReviseProfileCommand, SignalKind, SourceName, SuppressionTarget, Tier, TokenPricing,
    TokenUsage, UnitInterval, ValidatedProfile, Verdict,
    evaluation::{EvaluationReport, EvaluationSettings, ProviderBuilder},
    infrastructure::{
        crypto::AesGcmCipher, lead::postgres::PgLeadRepository, llm_cache::postgres::PgLlmCache,
        postgres::with_tx,
    },
    llm::CreateLlmProvider,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{at, domain};

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(1_000)
    }
}

#[derive(Debug, Clone)]
enum Reply {
    Best,
    Worst,
    Raw(Value),
    Fail(LlmError),
}

#[derive(Debug, Clone)]
struct Seen {
    system: String,
    instruction: String,
    documents: Vec<String>,
    schema: Value,
}

struct Shared {
    replies: Mutex<HashMap<String, VecDeque<Reply>>>,
    seen: Mutex<Vec<Seen>>,
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

fn answers_from(schema: &Value, last: bool) -> Value {
    let mut answers = serde_json::Map::new();
    let questions = schema["properties"]["answers"]["properties"]
        .as_object()
        .unwrap();
    for (id, question) in questions {
        let values = question["properties"]["choice"]["enum"].as_array().unwrap();
        let index = if last { values.len() - 1 } else { 0 };
        let probabilities: serde_json::Map<String, Value> = values
            .iter()
            .enumerate()
            .map(|(position, value)| {
                (
                    value.as_str().unwrap().to_owned(),
                    json!(if position == index { 1.0 } else { 0.0 }),
                )
            })
            .collect();
        answers.insert(
            id.clone(),
            json!({
                "type": "choice",
                "choice": values[index],
                "confidence": 0.9,
                "probabilities": probabilities
            }),
        );
    }
    json!({"answers": answers})
}

impl LlmProvider for Scripted {
    async fn complete(&self, request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        self.shared.seen.lock().unwrap().push(Seen {
            system: request.system.to_owned(),
            instruction: request.instruction.to_owned(),
            documents: request
                .documents
                .iter()
                .map(|document| document.content.to_owned())
                .collect(),
            schema: request.schema.clone(),
        });
        let reply = self
            .shared
            .replies
            .lock()
            .unwrap()
            .get_mut(&self.model)
            .and_then(VecDeque::pop_front)
            .unwrap_or(Reply::Best);
        let output = match reply {
            Reply::Best => answers_from(request.schema, false),
            Reply::Worst => answers_from(request.schema, true),
            Reply::Raw(value) => value,
            Reply::Fail(error) => return Err(error),
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

struct Harness {
    pool: PgPool,
    usecase: LodestoneUseCase,
    cipher: AesGcmCipher,
    shared: Arc<Shared>,
}

fn settings(budget_cents: u32, batch_size: u32) -> EvaluationSettings {
    EvaluationSettings {
        max_tokens: 512,
        budget_cents: NonZeroU32::new(budget_cents).unwrap(),
        batch_size,
        base_delay: Duration::ZERO,
    }
}

fn agent(id: &str, weight: u8, provider: Option<ProviderId>, question: &str) -> Value {
    json!({
        "id": id,
        "name": id,
        "enabled": true,
        "provider_id": provider.map(|provider| provider.0.to_string()),
        "system_prompt": format!("PROMPT-OF-{id}"),
        "weight": weight,
        "questions": [{
            "id": question,
            "wording": "Pertinent ?",
            "weight": 5,
            "options": [
                {"value": "forte", "label": "Forte", "score": 1.0},
                {"value": "moyenne", "label": "Moyenne", "score": 0.5},
                {"value": "faible", "label": "Faible", "score": 0.0}
            ]
        }]
    })
}

fn definition(agents: Vec<Value>) -> Value {
    let mut definition: Value = serde_json::from_str(IAM).unwrap();
    definition["qualification_agents"] = Value::Array(agents);
    definition
}

fn cipher() -> AesGcmCipher {
    let encoded = format!("{}QkI=", "QkJC".repeat(10));
    AesGcmCipher::new(&EncryptionKey::from_base64(&Secret::new(encoded)).unwrap()).unwrap()
}

impl Harness {
    fn new(pool: PgPool, input_tokens: u64) -> Self {
        Self {
            usecase: LodestoneUseCase::new(pool.clone()),
            pool,
            cipher: cipher(),
            shared: Arc::new(Shared {
                replies: Mutex::new(HashMap::new()),
                seen: Mutex::new(Vec::new()),
                input_tokens,
            }),
        }
    }

    async fn provider(&self, model: &str, input_cents_per_mtok: u32) -> ProviderId {
        self.usecase
            .create_llm_provider(
                CreateLlmProvider {
                    name: model.to_owned(),
                    kind: ProviderKind::OpenAiCompatible,
                    base_url: "https://llm.example/v1".parse().unwrap(),
                    model: model.to_owned(),
                    timeout: Duration::from_secs(30),
                    max_retries: 0,
                    pricing: TokenPricing {
                        input_cents_per_mtok,
                        output_cents_per_mtok: 0,
                    },
                    api_key: None,
                    reasoning_effort: None,
                },
                &self.cipher,
            )
            .await
            .unwrap()
            .id
    }

    async fn default_provider(&self, model: &str) -> ProviderId {
        let id = self.provider(model, 0).await;
        self.usecase
            .assign_llm_tier(Tier::Triage, id)
            .await
            .unwrap();
        id
    }

    async fn profile(&self, agents: Vec<Value>) -> ProfileId {
        let id = self
            .usecase
            .create_profile(CreateProfileCommand {
                name: ProfileName::parse("iam").unwrap(),
                definition: ValidatedProfile::parse(definition(agents)).unwrap(),
                created_by: None,
            })
            .await
            .unwrap()
            .profile
            .id;
        self.usecase.set_profile_active(id, true).await.unwrap();
        id
    }

    async fn revise(&self, profile: ProfileId, agents: Vec<Value>) {
        self.usecase
            .revise_profile(ReviseProfileCommand {
                profile_id: profile,
                definition: ValidatedProfile::parse(definition(agents)).unwrap(),
                created_by: None,
            })
            .await
            .unwrap();
    }

    fn script(&self, model: &str, replies: Vec<Reply>) {
        self.shared
            .replies
            .lock()
            .unwrap()
            .insert(model.to_owned(), replies.into());
    }

    fn seen(&self) -> Vec<Seen> {
        self.shared.seen.lock().unwrap().clone()
    }

    fn calls(&self) -> usize {
        self.shared.seen.lock().unwrap().len()
    }

    async fn active(&self) -> ActiveProfile {
        self.usecase.list_active_profiles().await.unwrap().remove(0)
    }

    async fn observe(&self, host: &str, source: SourceName, seconds: i64) {
        let active = self.active().await;
        self.usecase
            .record_observations(
                &active,
                source,
                &[Observation {
                    domain: domain(host),
                    kind: SignalKind::parse("product_detected").unwrap(),
                    value: Some(format!("keycloak-{}", source.as_str())),
                    raw: json!({"version": "21.1"}),
                    provenance: Provenance::parse(&format!("https://{host}/{}", source.as_str()))
                        .unwrap(),
                    strength: UnitInterval::new(0.9).unwrap(),
                    observed_at: at(seconds),
                    agent: None,
                }],
            )
            .await
            .unwrap();
    }

    async fn step(&self, settings: EvaluationSettings) -> EvaluationReport {
        self.usecase
            .qualify_due_leads(
                &FakeBuilder {
                    shared: Arc::clone(&self.shared),
                },
                &self.cipher,
                &PgLlmCache::new(self.pool.clone()),
                settings,
                &FixedClock,
            )
            .await
            .unwrap()
    }

    async fn lead(&self, profile: ProfileId, host: &str) -> Lead {
        with_tx(&self.pool, async |shared| {
            PgLeadRepository::new(&shared)
                .find_by_domain(profile, &domain(host))
                .await
        })
        .await
        .unwrap()
        .unwrap()
    }

    async fn results(&self, profile: ProfileId, host: &str) -> Vec<QualificationResult> {
        let lead = self.lead(profile, host).await;
        self.usecase
            .lead_qualification_results(lead.id)
            .await
            .unwrap()
    }
}

fn result_of<'a>(results: &'a [QualificationResult], agent: &str) -> &'a QualificationResult {
    results
        .iter()
        .find(|result| result.agent_id.as_str() == agent)
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn valid_answers_are_stored_scored_and_judged(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    harness.default_provider("model-default").await;
    let profile = harness
        .profile(vec![agent("pertinence", 5, None, "pertinence")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.leads, 1);
    assert_eq!(report.valid, 1);
    assert_eq!(report.invalid, 0);
    assert_eq!(report.backlog, 0);
    assert!(!report.budget_exhausted);
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results.len(), 1);
    let stored = &results[0];
    assert_eq!(stored.status, ResultStatus::Valid);
    assert_eq!(stored.agent_score, Some(1.0));
    assert_eq!(stored.model, "model-default");
    assert_eq!(stored.answers["pertinence"]["choice"], "forte");
    assert_eq!(stored.answers["pertinence"]["type"], "choice");
    assert_eq!(stored.prompt_hash.len(), 64);
    assert!(stored.error.is_none());
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.confidence, Some(100.0));
    assert_eq!(lead.verdict, Some(Verdict::Reliable));
    assert_eq!(lead.qualified_at, Some(at(1_000)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_prompt_carries_the_agent_the_profile_context_the_contract_and_the_lead(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    harness.default_provider("model-default").await;
    let profile = harness
        .profile(vec![agent("pertinence", 5, None, "pertinence")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let seen = harness.seen();
    assert_eq!(seen.len(), 1);
    let call = &seen[0];
    assert!(call.system.starts_with("PROMPT-OF-pertinence"));
    assert!(call.system.contains("Offer: "));
    assert!(call.system.contains("Write any free text in French."));
    assert!(call.system.contains("Contract JSON schema:"));
    assert!(call.instruction.contains("Question `pertinence`"));
    assert_eq!(call.documents.len(), 2);
    assert!(call.documents[0].starts_with("lead: acme.com"));
    assert!(call.documents[1].contains("signal: product_detected"));
    let active = harness.active().await;
    let expected = lodestone_core::contract_schema(
        &active.revision.definition.definition().qualification_agents[0],
    );
    assert_eq!(call.schema, expected);
    assert!(harness.lead(profile, "acme.com").await.verdict.is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn agent_weights_drive_the_lead_confidence_and_the_verdict(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let good = harness.provider("model-good", 0).await;
    let bad = harness.provider("model-bad", 0).await;
    harness.script("model-bad", vec![Reply::Worst]);
    let profile = harness
        .profile(vec![
            agent("light", 1, Some(good), "q"),
            agent("heavy", 3, Some(bad), "q"),
        ])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let results = harness.results(profile, "acme.com").await;
    assert_eq!(result_of(&results, "light").agent_score, Some(1.0));
    assert_eq!(result_of(&results, "heavy").agent_score, Some(0.0));
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.confidence, Some(25.0));
    assert_eq!(lead.verdict, Some(Verdict::Discarded));
}

fn broken() -> Value {
    json!({"answers": {"q": {"type": "choice", "choice": "enorme", "confidence": 0.5,
        "probabilities": {"forte": 1.0, "moyenne": 0.0, "faible": 0.0}}}})
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_contract_violation_is_retried_once_with_the_violation_list(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script("model-a", vec![Reply::Raw(broken()), Reply::Best]);
    let profile = harness
        .profile(vec![agent("solo", 5, Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.valid, 1);
    assert_eq!(report.invalid, 0);
    let seen = harness.seen();
    assert_eq!(seen.len(), 2);
    assert!(!seen[0].instruction.contains("broke the contract"));
    assert!(seen[1].instruction.contains("broke the contract"));
    assert!(seen[1].instruction.contains("`q`"));
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results[0].status, ResultStatus::Valid);
}

#[sqlx::test(migrations = "../../migrations")]
async fn two_contract_violations_store_an_invalid_result(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script("model-a", vec![Reply::Raw(broken()), Reply::Raw(broken())]);
    let profile = harness
        .profile(vec![agent("solo", 5, Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.valid, 0);
    assert_eq!(report.invalid, 1);
    assert_eq!(report.backlog, 0);
    assert_eq!(harness.calls(), 2);
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results[0].status, ResultStatus::Invalid);
    assert_eq!(results[0].agent_score, None);
    assert!(
        results[0]
            .error
            .as_deref()
            .unwrap()
            .contains("Réponse non conforme au contrat")
    );
    assert!(results[0].error.as_deref().unwrap().contains("`q`"));
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.confidence, None);
    assert_eq!(lead.verdict, None);
    assert!(lead.qualified_at.is_some());
    let again = harness.step(settings(100, 10)).await;
    assert_eq!(again.leads, 0);
    assert_eq!(harness.calls(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_provider_failure_stores_an_invalid_result(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script("model-a", vec![Reply::Fail(LlmError::Refused)]);
    let profile = harness
        .profile(vec![agent("solo", 5, Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results[0].status, ResultStatus::Invalid);
    assert!(
        results[0]
            .error
            .as_deref()
            .unwrap()
            .contains("Échec de l'appel au modèle")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_missing_provider_invalidates_one_agent_without_blocking_the_others(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    let disabled = harness.provider("model-off", 0).await;
    harness
        .usecase
        .set_llm_provider_enabled(disabled, false)
        .await
        .unwrap();
    let ghost = ProviderId(uuid::Uuid::from_u128(42));
    let profile = harness
        .profile(vec![
            agent("ghost", 5, Some(ghost), "q"),
            agent("off", 5, Some(disabled), "q"),
            agent("fine", 5, Some(provider), "q"),
        ])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.valid, 1);
    assert_eq!(report.invalid, 2);
    assert_eq!(harness.calls(), 1);
    let results = harness.results(profile, "acme.com").await;
    for id in ["ghost", "off"] {
        let result = result_of(&results, id);
        assert_eq!(result.status, ResultStatus::Invalid);
        assert_eq!(result.error.as_deref(), Some("fournisseur indisponible"));
    }
    assert_eq!(result_of(&results, "fine").status, ResultStatus::Valid);
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.confidence, Some(100.0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_a_default_provider_agents_are_invalid_not_blocking(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let profile = harness.profile(vec![agent("solo", 5, None, "q")]).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.invalid, 1);
    assert_eq!(harness.calls(), 0);
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(
        results[0].error.as_deref(),
        Some("fournisseur indisponible")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_is_due_on_creation_only_once_it_has_a_signal(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    harness.default_provider("model-default").await;
    let profile = harness.profile(vec![agent("solo", 5, None, "q")]).await;
    support::seen_lead(&pool, profile, "bare.com").await;

    let before = harness.step(settings(100, 10)).await;
    assert_eq!(before.leads, 0);
    assert_eq!(before.backlog, 0);

    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let after = harness.step(settings(100, 10)).await;
    assert_eq!(after.leads, 1);
    assert_eq!(after.backlog, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_signal_from_a_new_source_makes_the_lead_due_again(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    harness.default_provider("model-default").await;
    let profile = harness.profile(vec![agent("solo", 5, None, "q")]).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    assert_eq!(harness.step(settings(100, 10)).await.leads, 1);

    harness.observe("acme.com", SourceName::DnsCname, 20).await;
    let known = harness.step(settings(100, 10)).await;
    assert_eq!(known.leads, 1);

    harness.observe("acme.com", SourceName::HttpProbe, 30).await;
    let none = harness.step(settings(100, 10)).await;
    assert_eq!(none.leads, 0);
    assert_eq!(none.backlog, 0);
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results.len(), 1);
    assert_eq!(harness.calls(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_profile_revision_makes_every_lead_due_again(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    harness.default_provider("model-default").await;
    let profile = harness.profile(vec![agent("solo", 5, None, "q")]).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    assert_eq!(harness.step(settings(100, 10)).await.leads, 1);
    assert_eq!(harness.step(settings(100, 10)).await.leads, 0);

    harness
        .revise(profile, vec![agent("solo", 7, None, "q")])
        .await;

    let report = harness.step(settings(100, 10)).await;
    assert_eq!(report.leads, 1);
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results.len(), 1);
    assert_eq!(harness.calls(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn suppressed_domains_are_never_qualified(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    harness.default_provider("model-default").await;
    harness.profile(vec![agent("solo", 5, None, "q")]).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness
        .usecase
        .add_suppression(SuppressionTarget::Domain(domain("acme.com")), None, None)
        .await
        .unwrap();

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.leads, 0);
    assert_eq!(report.backlog, 0);
    assert_eq!(harness.calls(), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_step_takes_a_bounded_batch_oldest_first_and_reports_the_backlog(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    harness.default_provider("model-default").await;
    let profile = harness.profile(vec![agent("solo", 5, None, "q")]).await;
    for index in 0..12 {
        harness
            .observe(
                &format!("lead-{index:02}.com"),
                SourceName::HttpProbe,
                100 - i64::from(index),
            )
            .await;
    }

    let first = harness.step(settings(100, 10)).await;

    assert_eq!(first.leads, 10);
    assert_eq!(first.backlog, 2);
    assert!(harness.lead(profile, "lead-11.com").await.verdict.is_some());
    assert!(harness.lead(profile, "lead-00.com").await.verdict.is_none());
    let second = harness.step(settings(100, 10)).await;
    assert_eq!(second.leads, 2);
    assert_eq!(second.backlog, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_exhausted_budget_leaves_the_lead_due(pool: PgPool) {
    let harness = Harness::new(pool, 1_000_000);
    let pricey = harness.provider("model-pricey", 100).await;
    let profile = harness
        .profile(vec![
            agent("first", 5, Some(pricey), "q"),
            agent("second", 5, Some(pricey), "q"),
        ])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(1, 10)).await;

    assert!(report.budget_exhausted);
    assert_eq!(report.leads, 0);
    assert_eq!(report.backlog, 1);
    assert_eq!(harness.calls(), 1);
    assert!(
        harness
            .lead(profile, "acme.com")
            .await
            .qualified_at
            .is_none()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn answers_are_cached_per_agent_and_prompt(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile(vec![
            agent("one", 5, Some(provider), "q"),
            agent("two", 5, Some(provider), "q"),
        ])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.step(settings(100, 10)).await;
    assert_eq!(harness.calls(), 2);

    sqlx::query("DELETE FROM lead_qualification_state")
        .execute(&pool)
        .await
        .unwrap();
    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.leads, 1);
    assert_eq!(report.valid, 2);
    assert_eq!(harness.calls(), 2);
    assert_eq!(harness.results(profile, "acme.com").await.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_cached_contract_violation_is_never_served(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script(
        "model-a",
        vec![Reply::Raw(broken()), Reply::Raw(broken()), Reply::Best],
    );
    harness
        .profile(vec![agent("solo", 5, Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.step(settings(100, 10)).await;
    assert_eq!(harness.calls(), 2);

    sqlx::query("DELETE FROM lead_qualification_state")
        .execute(&pool)
        .await
        .unwrap();
    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.valid, 1);
    assert_eq!(harness.calls(), 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_disabled_agent_is_not_run(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    let mut off = agent("off", 5, Some(provider), "q");
    off["enabled"] = json!(false);
    let profile = harness
        .profile(vec![off, agent("on", 5, Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].agent_id.as_str(), "on");
}
