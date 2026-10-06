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
    ActiveProfile, Clock, CreateProfileCommand, GenerationParams, Lead, LeadId, LeadRepository,
    LeadScoring, LlmError, LlmProvider, LlmRequest, LlmResponse, LodestoneUseCase, Observation,
    ProfileId, ProfileName, Provenance, ProviderDefinition, ProviderId, ProviderKind,
    QualificationResult, ReasoningEffort, ResultId, ResultStatus, ReviseProfileCommand,
    RevisionNumber, SignalKind, SourceName, SuppressionTarget, Tier, TokenPricing, TokenUsage,
    UnitInterval, ValidatedProfile, Verdict, VerdictCap, VerdictReason,
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

fn quote_of(request: &LlmRequest<'_>) -> String {
    request
        .documents
        .first()
        .and_then(|document| document.content.lines().next())
        .unwrap_or_default()
        .to_owned()
}

fn answers_from(schema: &Value, last: bool, quote: &str) -> Value {
    let mut answers = serde_json::Map::new();
    let questions = schema["properties"]["answers"]["properties"]
        .as_object()
        .unwrap();
    for (id, question) in questions {
        let all = question["properties"]["choice"]["enum"].as_array().unwrap();
        let known: Vec<&Value> = all
            .iter()
            .filter(|value| value.as_str() != Some("inconnu"))
            .collect();
        let chosen = if last {
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
            Reply::Best => answers_from(request.schema, false, &quote_of(request)),
            Reply::Worst => answers_from(request.schema, true, &quote_of(request)),
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
        "description": format!("DESCRIPTION-OF-{id}"),
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
                {"value": "faible", "label": "Faible", "score": 0.0},
                {"value": "inconnu", "label": "Inconnu", "score": 0.5, "is_unknown": true}
            ]
        }]
    })
}

fn definition(agents: Vec<Value>) -> Value {
    let mut definition = default_definition(agents);
    definition["verdict"]["calibration_floor"] = json!(0.0);
    definition["verdict"]["min_evidence"] = json!(0.0);
    definition["evidence"]["influence"] = json!(0.0);
    definition
}

fn default_definition(agents: Vec<Value>) -> Value {
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
                    generation: GenerationParams::default(),
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

    async fn profile_tuned(
        &self,
        agents: Vec<Value>,
        verdict: Value,
        evidence: Value,
    ) -> ProfileId {
        let mut value = default_definition(agents);
        for (key, patch) in [("verdict", verdict), ("evidence", evidence)] {
            for (field, setting) in patch.as_object().unwrap() {
                value[key][field] = setting.clone();
            }
        }
        let id = self
            .usecase
            .create_profile(CreateProfileCommand {
                name: ProfileName::parse("iam").unwrap(),
                definition: ValidatedProfile::parse(value).unwrap(),
                created_by: None,
            })
            .await
            .unwrap()
            .profile
            .id;
        self.usecase.set_profile_active(id, true).await.unwrap();
        id
    }

    async fn scoring(&self, profile: ProfileId, host: &str) -> Option<LeadScoring> {
        let lead = self.lead(profile, host).await;
        self.usecase.lead_detail(lead.id).await.unwrap().scoring
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
        self.observe_value(
            host,
            source,
            &format!("keycloak-{}", source.as_str()),
            seconds,
        )
        .await;
    }

    async fn observe_value(&self, host: &str, source: SourceName, value: &str, seconds: i64) {
        let active = self.active().await;
        self.usecase
            .record_observations(
                &active,
                source,
                &[Observation {
                    domain: domain(host),
                    kind: SignalKind::parse("product_detected").unwrap(),
                    value: Some(value.to_owned()),
                    raw: json!({"version": "21.1"}),
                    provenance: Provenance::parse(&format!("https://{host}/{value}")).unwrap(),
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
async fn each_result_stores_the_provider_model_prompt_and_generation_settings(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    let provider = harness
        .usecase
        .create_llm_provider(
            CreateLlmProvider {
                name: "local-gpu".to_owned(),
                kind: ProviderKind::OpenAiCompatible,
                base_url: "https://llm.example/v1".parse().unwrap(),
                model: "model-tuned".to_owned(),
                timeout: Duration::from_secs(30),
                max_retries: 0,
                pricing: TokenPricing {
                    input_cents_per_mtok: 0,
                    output_cents_per_mtok: 0,
                },
                api_key: None,
                reasoning_effort: Some(ReasoningEffort::Low),
                generation: GenerationParams {
                    temperature: Some(0.3),
                    presence_penalty: Some(0.5),
                    ..GenerationParams::default()
                },
            },
            &harness.cipher,
        )
        .await
        .unwrap()
        .id;
    harness
        .usecase
        .assign_llm_tier(Tier::Triage, provider)
        .await
        .unwrap();
    let profile = harness
        .profile(vec![agent("pertinence", 5, None, "pertinence")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let results = harness.results(profile, "acme.com").await;
    let stored = &results[0].provenance;
    assert_eq!(stored.provider_id, Some(provider));
    assert_eq!(stored.provider_name, "local-gpu");
    assert_eq!(stored.model, "model-tuned");
    assert_eq!(stored.samples, 1);
    assert_eq!(stored.generation.temperature, Some(0.3));
    assert_eq!(stored.generation.presence_penalty, Some(0.5));
    assert_eq!(stored.generation.frequency_penalty, None);
    assert_eq!(
        stored.generation.reasoning_effort,
        Some(ReasoningEffort::Low)
    );
    assert_eq!(stored.generation.max_tokens, Some(512));
    let row: (String, serde_json::Value, i32) = sqlx::query_as(
        "SELECT provider_name, generation, samples FROM qualification_results WHERE lead_id = $1",
    )
    .bind(results[0].lead_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "local-gpu");
    assert_eq!(row.1["reasoning_effort"], "low");
    assert_eq!(row.2, 1);
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
    assert_eq!(stored.provenance.model, "model-default");
    assert_eq!(stored.answers["pertinence"]["choice"], "forte");
    assert_eq!(stored.answers["pertinence"]["type"], "choice");
    assert!(
        stored
            .provenance
            .prompt_version
            .strip_prefix("qualification-agent@")
            .is_some_and(|hash| hash.len() == 64)
    );
    assert_eq!(
        stored.summary.as_deref(),
        Some("Observations de l'agent sur le lead.")
    );
    assert!(stored.error.is_none());
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.confidence, Some(100.0));
    assert_eq!(lead.verdict, Some(Verdict::ToCheck));
    assert_eq!(lead.qualified_at, Some(at(1_000)));
    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert_eq!(scoring.opinion, Some(1.0));
    assert_eq!(scoring.cap, Some(VerdictCap::SingleAngle));
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
    assert!(call.system.contains("`summary` is 2 to 4 sentences"));
    assert_eq!(call.schema["required"], json!(["summary", "answers"]));
    assert!(call.instruction.contains("Question `pertinence`"));
    assert_eq!(call.documents.len(), 2);
    assert!(call.documents[0].starts_with("lead: acme.com"));
    assert!(
        call.documents[1]
            .starts_with("acme.com expose Keycloak-http_probe (sonde HTTP, version 21.1)")
    );
    assert!(call.documents[0].contains("- hosts carrying signals: 1 (acme.com (production))"));
    assert!(call.documents[0].contains("- production hosts: 1 of 1"));
    assert!(!call.documents[0].contains("warning"));
    assert!(
        call.system
            .contains("computed by the system from stored data")
    );
    let active = harness.active().await;
    let expected = lodestone_core::contract_schema(
        &active.revision.definition.definition().qualification_agents[0],
    );
    assert_eq!(call.schema, expected);
    assert!(harness.lead(profile, "acme.com").await.verdict.is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_seen_only_on_a_development_host_is_documented_as_such(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    harness.default_provider("model-default").await;
    harness
        .profile(vec![agent("pertinence", 5, None, "pertinence")])
        .await;
    lodestone_core::CandidateHosts::upsert(
        &lodestone_core::infrastructure::candidate_host::PgCandidateHosts::new(pool),
        &[lodestone_core::CandidateHost::discovered(
            lodestone_core::Hostname::parse("www.acme.com").unwrap(),
            lodestone_core::HostOrigin::CertificateTransparency,
            at(0),
        )
        .unwrap()],
    )
    .await
    .unwrap();
    harness
        .observe("login.dev.acme.com", SourceName::HttpProbe, 10)
        .await;

    harness.step(settings(100, 10)).await;

    let seen = harness.seen();
    let overview = &seen[0].documents[0];
    assert!(overview.contains("- hosts carrying signals: 1 (login.dev.acme.com (development))"));
    assert!(overview.contains("- production hosts: 0 of 1"));
    assert!(overview.contains("- warning: every host carrying a signal is a non-production"));
    assert!(overview.contains("- distinct subdomains seen for the domain: 2"));
    assert!(seen[0].documents[1].starts_with("login.dev.acme.com expose Keycloak-http_probe"));
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
    json!({"summary": "Observations.", "answers": {"q": {"type": "choice", "choice": "enorme", "confidence": 0.5,
        "evidence": "lead: acme.com", "probabilities": {"forte": 1.0, "moyenne": 0.0, "faible": 0.0, "inconnu": 0.0}}}})
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

fn solo_answers() -> Value {
    let profile = ValidatedProfile::parse(definition(vec![agent("solo", 5, None, "q")])).unwrap();
    let schema = lodestone_core::contract_schema(&profile.definition().qualification_agents[0]);
    answers_from(&schema, false, "lead: acme.com")
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_answer_without_summary_is_retried_then_stored_without_one(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    let mut headless = solo_answers();
    headless.as_object_mut().unwrap().remove("summary");
    harness.script(
        "model-a",
        vec![Reply::Raw(headless.clone()), Reply::Raw(headless)],
    );
    let profile = harness
        .profile(vec![agent("solo", 5, Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.invalid, 1);
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results[0].status, ResultStatus::Invalid);
    assert_eq!(results[0].summary, None);
    assert!(results[0].error.as_deref().unwrap().contains("`summary`"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_over_long_summary_is_a_violation_and_a_retry_fixes_it(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    let mut verbose = solo_answers();
    verbose["summary"] = json!("x".repeat(601));
    harness.script("model-a", vec![Reply::Raw(verbose), Reply::Best]);
    let profile = harness
        .profile(vec![agent("solo", 5, Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.valid, 1);
    let seen = harness.seen();
    assert_eq!(seen.len(), 2);
    assert!(
        seen[1]
            .instruction
            .contains("`summary` fait 601 caractères")
    );
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(
        results[0].summary.as_deref(),
        Some("Observations de l'agent sur le lead.")
    );
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

fn two_axes_agent(id: &str, provider: ProviderId) -> Value {
    let mut value = agent(id, 5, Some(provider), "q");
    let second = value["questions"][0].clone();
    let mut second = second;
    second["id"] = json!("q2");
    value["questions"].as_array_mut().unwrap().push(second);
    value
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_single_dev_host_answered_with_certainty_is_calibrated_and_not_reliable(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile_tuned(
            vec![agent("pertinence", 5, Some(provider), "q")],
            json!({}),
            json!({}),
        )
        .await;
    harness
        .observe("login.dev.luzernrisk.com", SourceName::DnsCname, 10)
        .await;

    harness.step(settings(100, 10)).await;

    let results = harness.results(profile, "luzernrisk.com").await;
    let score = results[0].agent_score.unwrap();
    assert!((score - 1.0 / 1.1).abs() < 1e-9, "{score}");
    let lead = harness.lead(profile, "luzernrisk.com").await;
    let scoring = harness.scoring(profile, "luzernrisk.com").await.unwrap();
    assert!((scoring.opinion.unwrap() - score).abs() < 1e-9);
    assert!(scoring.evidence.only_non_production);
    assert_eq!(scoring.evidence.signal_count, 1);
    assert_eq!(scoring.evidence.components.environment, 0.0);
    assert_eq!(scoring.evidence.components.diversity, 0.5);
    let expected = scoring.opinion.unwrap() * (0.4 + 0.6 * scoring.evidence.score);
    assert!((lead.confidence.unwrap() - expected * 100.0).abs() < 0.06);
    assert!(lead.confidence.unwrap() < 70.0);
    assert_eq!(lead.verdict, Some(Verdict::ToCheck));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_confident_lead_on_a_dev_host_names_the_cap(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let first = harness.provider("model-a", 0).await;
    let second = harness.provider("model-b", 0).await;
    let profile = harness
        .profile_tuned(
            vec![
                agent("one", 5, Some(first), "q"),
                agent("two", 5, Some(second), "q"),
            ],
            json!({"reliable_from": 40, "discard_below": 10}),
            json!({}),
        )
        .await;
    harness
        .observe("login.dev.acme.com", SourceName::DnsCname, 10)
        .await;

    harness.step(settings(100, 10)).await;

    let lead = harness.lead(profile, "acme.com").await;
    assert!(lead.confidence.unwrap() >= 40.0);
    assert_eq!(lead.verdict, Some(Verdict::ToCheck));
    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert_eq!(scoring.cap, Some(VerdictCap::NonProduction));
}

#[sqlx::test(migrations = "../../migrations")]
async fn two_axes_and_enough_evidence_make_a_lead_reliable(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile_tuned(
            vec![two_axes_agent("solo", provider)],
            json!({"min_evidence": 0.0}),
            json!({"influence": 0.0}),
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, Some(Verdict::Reliable));
    assert_eq!(
        harness.scoring(profile, "acme.com").await.unwrap().cap,
        None
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn weak_evidence_caps_an_otherwise_reliable_lead(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile_tuned(
            vec![two_axes_agent("solo", provider)],
            json!({"min_evidence": 0.95}),
            json!({"influence": 0.0}),
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, Some(Verdict::ToCheck));
    assert_eq!(
        harness.scoring(profile, "acme.com").await.unwrap().cap,
        Some(VerdictCap::WeakEvidence)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn leads_scored_before_the_evidence_rules_are_rescored_without_a_model_call(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile_tuned(
            vec![agent("solo", 5, Some(provider), "q")],
            json!({}),
            json!({}),
        )
        .await;
    harness
        .observe("login.dev.acme.com", SourceName::DnsCname, 10)
        .await;
    harness.step(settings(100, 10)).await;
    let lead = harness.lead(profile, "acme.com").await;
    sqlx::query("UPDATE lead_qualification_state SET scoring_version = 0, opinion = NULL, evidence = NULL, verdict_reason = NULL")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE leads SET confidence = 100, verdict = 'fiable'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE qualification_results SET agent_score = 1")
        .execute(&pool)
        .await
        .unwrap();
    let calls = harness.calls();
    assert!(harness.scoring(profile, "acme.com").await.is_none());

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.rescored, 1);
    assert_eq!(report.leads, 0);
    assert_eq!(report.backlog, 0);
    assert_eq!(harness.calls(), calls);
    let rescored = harness.lead(profile, "acme.com").await;
    assert_ne!(rescored.confidence, Some(100.0));
    assert_eq!(rescored.verdict, Some(Verdict::ToCheck));
    assert_eq!(rescored.qualified_at, lead.qualified_at);
    let results = harness.results(profile, "acme.com").await;
    assert!((results[0].agent_score.unwrap() - 1.0 / 1.1).abs() < 1e-9);
    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert!(scoring.evidence.only_non_production);
    let again = harness.step(settings(100, 10)).await;
    assert_eq!(again.rescored, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_signal_from_a_known_source_refreshes_the_evidence_without_a_model_call(
    pool: PgPool,
) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile_tuned(
            vec![agent("solo", 5, Some(provider), "q")],
            json!({}),
            json!({}),
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.step(settings(100, 10)).await;
    let calls = harness.calls();
    let before = harness.scoring(profile, "acme.com").await.unwrap();
    assert_eq!(before.evidence.signal_count, 1);
    harness
        .observe_value("acme.com", SourceName::HttpProbe, "another", 20)
        .await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.rescored, 1);
    assert_eq!(harness.calls(), calls);
    let after = harness.scoring(profile, "acme.com").await.unwrap();
    assert_eq!(after.evidence.signal_count, 2);
    assert!(after.evidence.components.volume > before.evidence.components.volume);
}

fn ungrounded_answers() -> Value {
    let mut value = solo_answers();
    value["answers"]["q"]["evidence"] = json!("une phrase inventée par le modèle");
    value
}

fn unknown_answers() -> Value {
    let mut value = solo_answers();
    value["answers"]["q"]["choice"] = json!("inconnu");
    value["answers"]["q"]["evidence"] = json!("");
    value["answers"]["q"]["probabilities"] =
        json!({"forte": 0.0, "moyenne": 0.0, "faible": 0.0, "inconnu": 1.0});
    value
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_ungrounded_answer_is_retried_with_its_axes_and_stored_grounded(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script(
        "model-a",
        vec![Reply::Raw(ungrounded_answers()), Reply::Best],
    );
    let profile = harness
        .profile(vec![agent("solo", 5, Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.valid, 1);
    let seen = harness.seen();
    assert_eq!(seen.len(), 2);
    assert!(!seen[0].instruction.contains("no verbatim quote"));
    assert!(seen[1].instruction.contains("no verbatim quote"));
    assert!(seen[1].instruction.contains("`q`"));
    let results = harness.results(profile, "acme.com").await;
    let answer = &results[0].answers["q"];
    assert_eq!(answer["choice"], "forte");
    assert_eq!(answer["grounded"], true);
    assert_eq!(answer["evidence"], "lead: acme.com");
    assert_eq!(answer["evidence_source"], "lead");
    assert_eq!(results[0].agent_score, Some(1.0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_answer_still_ungrounded_after_the_retry_is_downgraded_not_invalid(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script(
        "model-a",
        vec![
            Reply::Raw(ungrounded_answers()),
            Reply::Raw(ungrounded_answers()),
        ],
    );
    let profile = harness
        .profile(vec![agent("solo", 5, Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.valid, 1);
    assert_eq!(report.invalid, 0);
    assert_eq!(harness.calls(), 2);
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results[0].status, ResultStatus::Valid);
    let answer = &results[0].answers["q"];
    assert_eq!(answer["choice"], "inconnu");
    assert_eq!(answer["grounded"], false);
    assert_eq!(answer["original_choice"], "forte");
    assert_eq!(answer["probabilities"]["inconnu"], 1.0);
    assert_eq!(answer["probabilities"]["forte"], 0.0);
    assert_eq!(results[0].agent_score, Some(0.5));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_retry_that_breaks_the_contract_keeps_the_first_downgraded_answer(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script(
        "model-a",
        vec![Reply::Raw(ungrounded_answers()), Reply::Raw(broken())],
    );
    let profile = harness
        .profile(vec![agent("solo", 5, Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.valid, 1);
    assert_eq!(harness.calls(), 2);
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results[0].status, ResultStatus::Valid);
    assert_eq!(results[0].answers["q"]["choice"], "inconnu");
    assert_eq!(results[0].answers["q"]["original_choice"], "forte");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_unknown_option_with_an_empty_quote_is_accepted_without_a_retry(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script("model-a", vec![Reply::Raw(unknown_answers())]);
    let profile = harness
        .profile(vec![agent("solo", 5, Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    assert_eq!(harness.calls(), 1);
    let results = harness.results(profile, "acme.com").await;
    let answer = &results[0].answers["q"];
    assert_eq!(answer["choice"], "inconnu");
    assert_eq!(answer["grounded"], true);
    assert!(answer.get("original_choice").is_none());
    assert_eq!(results[0].agent_score, Some(0.5));
}

#[sqlx::test(migrations = "../../migrations")]
async fn rescoring_reads_the_downgraded_answers(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script(
        "model-a",
        vec![
            Reply::Raw(ungrounded_answers()),
            Reply::Raw(ungrounded_answers()),
        ],
    );
    let profile = harness
        .profile(vec![agent("solo", 5, Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.step(settings(100, 10)).await;
    sqlx::query("UPDATE lead_qualification_state SET scoring_version = 0, opinion = NULL, evidence = NULL, verdict_reason = NULL")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE qualification_results SET agent_score = 1")
        .execute(&pool)
        .await
        .unwrap();
    let calls = harness.calls();

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.rescored, 1);
    assert_eq!(harness.calls(), calls);
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results[0].agent_score, Some(0.5));
    assert_eq!(results[0].answers["q"]["grounded"], false);
}

#[test]
fn the_aggregator_document_carries_the_quote_and_the_ungrounded_marker() {
    let profile = ValidatedProfile::parse(definition(vec![agent("solo", 5, None, "q")])).unwrap();
    let agent = &profile.definition().qualification_agents[0];
    let mut result = QualificationResult {
        id: ResultId(uuid::Uuid::nil()),
        lead_id: LeadId(uuid::Uuid::nil()),
        profile_id: ProfileId(uuid::Uuid::nil()),
        profile_revision: RevisionNumber::FIRST,
        agent_id: agent.id.clone(),
        provenance: lodestone_core::RunProvenance::legacy("m".to_owned(), String::new()),
        status: ResultStatus::Valid,
        summary: Some("Résumé.".to_owned()),
        answers: json!({"q": {
            "type": "choice", "choice": "forte", "evidence": "Keycloak 18", "grounded": true,
            "confidence": 0.9,
            "probabilities": {"forte": 1.0, "moyenne": 0.0, "faible": 0.0, "inconnu": 0.0}
        }}),
        agent_score: Some(1.0),
        error: None,
        created_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
    };
    let grounded = lodestone_core::render_agent(agent, Some(&result));
    assert!(grounded.contains("quote: \"Keycloak 18\""));
    result.answers["q"]["choice"] = json!("inconnu");
    result.answers["q"]["grounded"] = json!(false);
    result.answers["q"]["original_choice"] = json!("forte");
    let ungrounded = lodestone_core::render_agent(agent, Some(&result));
    assert!(ungrounded.contains("ungrounded: the agent first chose `forte`"));
}

fn excluding_agent(id: &str, provider: ProviderId, threshold: f64) -> Value {
    json!({
        "id": id,
        "name": "Exclusions",
        "description": "Raisons d'écarter le lead.",
        "enabled": true,
        "provider_id": provider.0.to_string(),
        "system_prompt": format!("PROMPT-OF-{id}"),
        "weight": 1,
        "exclusion_threshold": threshold,
        "questions": [{
            "id": "client_actuel",
            "wording": "Déjà client ?",
            "weight": 5,
            "options": [
                {"value": "oui", "label": "Client actuel", "score": 0.0, "excludes": true},
                {"value": "non", "label": "Non", "score": 1.0},
                {"value": "inconnu", "label": "Inconnu", "score": 0.5, "is_unknown": true}
            ]
        }]
    })
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_grounded_exclusion_discards_a_confident_lead_and_keeps_the_confidence(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile(vec![
            agent("solo", 5, Some(provider), "q"),
            agent("second", 5, Some(provider), "q"),
            excluding_agent("exclusions", provider, 0.6),
        ])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, Some(Verdict::Discarded));
    assert!(lead.confidence.unwrap() > 70.0);
    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert_eq!(scoring.reason(), Some(VerdictReason::Excluded));
    assert_eq!(scoring.exclusions.len(), 1);
    let exclusion = &scoring.exclusions[0];
    assert_eq!(exclusion.agent_id.as_str(), "exclusions");
    assert_eq!(exclusion.agent_name, "Exclusions");
    assert_eq!(exclusion.question_id, "client_actuel");
    assert_eq!(exclusion.option, "oui");
    assert_eq!(exclusion.label, "Client actuel");
    assert_eq!(exclusion.probability, 1.0);
    assert_eq!(exclusion.evidence, "lead: acme.com");
    assert_eq!(exclusion.evidence_source.as_deref(), Some("lead"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_answer_that_does_not_pick_the_excluding_option_leaves_the_verdict(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script("model-a", vec![Reply::Best, Reply::Best, Reply::Worst]);
    let profile = harness
        .profile(vec![
            agent("solo", 5, Some(provider), "q"),
            agent("second", 5, Some(provider), "q"),
            excluding_agent("exclusions", provider, 0.6),
        ])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, Some(Verdict::Reliable));
    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert!(scoring.exclusions.is_empty());
    assert_eq!(scoring.reason(), None);
}

fn ungrounded_exclusion() -> Value {
    json!({
        "summary": "Observations de l'agent sur le lead.",
        "answers": {"client_actuel": {
            "type": "choice",
            "choice": "oui",
            "evidence": "une phrase inventée par le modèle",
            "confidence": 0.9,
            "probabilities": {"oui": 1.0, "non": 0.0, "inconnu": 0.0}
        }}
    })
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_downgraded_exclusion_does_not_exclude(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script(
        "model-a",
        vec![
            Reply::Raw(ungrounded_exclusion()),
            Reply::Raw(ungrounded_exclusion()),
        ],
    );
    let profile = harness
        .profile(vec![excluding_agent("exclusions", provider, 0.6)])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results[0].answers["client_actuel"]["grounded"], false);
    let lead = harness.lead(profile, "acme.com").await;
    assert_ne!(lead.verdict, Some(Verdict::Discarded));
    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert!(scoring.exclusions.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn rescoring_applies_exclusions_to_stored_leads_without_a_model_call(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile(vec![
            agent("solo", 5, Some(provider), "q"),
            excluding_agent("exclusions", provider, 0.6),
        ])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.step(settings(100, 10)).await;
    sqlx::query("UPDATE lead_qualification_state SET scoring_version = 0, opinion = NULL, evidence = NULL, verdict_reason = NULL, exclusions = '[]'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE leads SET confidence = 100, verdict = 'fiable'")
        .execute(&pool)
        .await
        .unwrap();
    let calls = harness.calls();
    assert!(harness.scoring(profile, "acme.com").await.is_none());

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.rescored, 1);
    assert_eq!(harness.calls(), calls);
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, Some(Verdict::Discarded));
    assert_ne!(lead.confidence, Some(100.0));
    assert!(lead.confidence.is_some());
    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert_eq!(scoring.exclusions.len(), 1);
    assert_eq!(scoring.exclusions[0].option, "oui");
    let stored: String = sqlx::query_scalar("SELECT verdict_reason FROM lead_qualification_state")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, "excluded");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_threshold_above_the_probability_keeps_the_lead(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    let soft = json!({
        "summary": "Observations de l'agent sur le lead.",
        "answers": {"client_actuel": {
            "type": "choice",
            "choice": "oui",
            "evidence": "lead: acme.com",
            "confidence": 0.9,
            "probabilities": {"oui": 0.7, "non": 0.2, "inconnu": 0.1}
        }}
    });
    harness.script("model-a", vec![Reply::Raw(soft)]);
    let profile = harness
        .profile(vec![excluding_agent("exclusions", provider, 0.8)])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert!(scoring.exclusions.is_empty());
}
