mod support;

use std::{
    collections::{HashMap, VecDeque},
    num::{NonZeroU32, NonZeroUsize},
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, Utc};
use common::{EncryptionKey, Secret};
use lodestone_core::{
    ActiveProfile, AgentRegistry, CancelReason, Clock, CompanyDossier, CompanyRepository,
    CreateProfileCommand, DossierState, EventStatus, GenerationParams, Lead, LeadId,
    LeadRepository, LeadScoring, LlmError, LlmProvider, LlmRequest, LlmResponse, LodestoneUseCase,
    MaxConcurrency, Observation, OfficialData, ProfileId, ProfileName, Provenance,
    ProviderDefinition, ProviderId, ProviderKind, QualificationResult, ROUND_STARTED,
    ReasoningEffort, ResultId, ResultStatus, ReviseProfileCommand, RevisionNumber, RoundTrigger,
    SCORING_VERSION, SignalKind, Siren, SourceName, Stage, StageReason, StageStatus,
    SuppressionTarget, Tier, TimelineEvent, TimelineStep, TokenPricing, TokenUsage, UnitInterval,
    ValidatedProfile, Verdict, VerdictCap, VerdictReason, WorkKind,
    evaluation::{EvaluationReport, EvaluationSettings, ProviderBuilder},
    infrastructure::{
        company::postgres::PgCompanyRepository, crypto::AesGcmCipher,
        lead::postgres::PgLeadRepository, llm_cache::postgres::PgLlmCache, postgres::with_tx,
    },
    llm::{CreateLlmProvider, KeyChange, UpdateLlmProvider},
    timeline_kind, verdict_for,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{at, domain};
use tokio::{spawn, time::sleep};

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
    Middle,
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
    model: String,
    temperature: Option<f64>,
}

struct Shared {
    replies: Mutex<HashMap<String, VecDeque<Reply>>>,
    seen: Mutex<Vec<Seen>>,
    input_tokens: u64,
    on_call: Mutex<Option<(LodestoneUseCase, ActiveProfile)>>,
    delays: Mutex<VecDeque<Duration>>,
    pace: Mutex<Duration>,
    running: Mutex<(u32, u32)>,
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

#[derive(Clone, Copy)]
enum Pick {
    First,
    Middle,
    Last,
}

fn answers_from(schema: &Value, pick: Pick, quote: &str) -> Value {
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
        let chosen = match pick {
            Pick::First => known[0],
            Pick::Middle => known[known.len() / 2],
            Pick::Last => known[known.len() - 1],
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
        let change = self.shared.on_call.lock().unwrap().take();
        if let Some((usecase, changed)) = change {
            usecase.agent_registry().publish(&changed);
        }
        self.shared.seen.lock().unwrap().push(Seen {
            system: request.system.to_owned(),
            instruction: request.instruction.to_owned(),
            documents: request
                .documents
                .iter()
                .map(|document| document.content.to_owned())
                .collect(),
            schema: request.schema.clone(),
            model: self.model.clone(),
            temperature: request.temperature,
        });
        let reply = self
            .shared
            .replies
            .lock()
            .unwrap()
            .get_mut(&self.model)
            .and_then(VecDeque::pop_front)
            .unwrap_or(Reply::Best);
        let delay = self
            .shared
            .delays
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(*self.shared.pace.lock().unwrap());
        {
            let mut running = self.shared.running.lock().unwrap();
            running.0 += 1;
            running.1 = running.1.max(running.0);
        }
        if !delay.is_zero() {
            sleep(delay).await;
        }
        self.shared.running.lock().unwrap().0 -= 1;
        let output = match reply {
            Reply::Best => answers_from(request.schema, Pick::First, &quote_of(request)),
            Reply::Middle => answers_from(request.schema, Pick::Middle, &quote_of(request)),
            Reply::Worst => answers_from(request.schema, Pick::Last, &quote_of(request)),
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
        concurrency: NonZeroUsize::MIN,
    }
}

fn concurrent(budget_cents: u32, batch_size: u32, concurrency: usize) -> EvaluationSettings {
    EvaluationSettings {
        concurrency: NonZeroUsize::new(concurrency).unwrap(),
        ..settings(budget_cents, batch_size)
    }
}

fn agent(id: &str, provider: Option<ProviderId>, question: &str) -> Value {
    json!({
        "id": id,
        "name": id,
        "description": format!("DESCRIPTION-OF-{id}"),
        "enabled": true,
        "provider_id": provider.map(|provider| provider.0.to_string()),
        "system_prompt": format!("PROMPT-OF-{id}"),
        "samples": 1,
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

fn revised(mut agent: Value) -> Value {
    agent["system_prompt"] = json!("PROMPT-REVISED");
    agent
}

fn sampled(mut agent: Value, samples: u8) -> Value {
    agent["samples"] = json!(samples);
    agent
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
                on_call: Mutex::new(None),
                delays: Mutex::new(VecDeque::new()),
                pace: Mutex::new(Duration::ZERO),
                running: Mutex::new((0, 0)),
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
                    max_concurrency: None,
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

    async fn limited_provider(
        &self,
        model: &str,
        input_cents_per_mtok: u32,
        limit: u8,
    ) -> ProviderId {
        let id = self.provider(model, input_cents_per_mtok).await;
        let current = self.usecase.get_llm_provider(id).await.unwrap();
        self.usecase
            .update_llm_provider(
                UpdateLlmProvider {
                    id,
                    name: current.name,
                    kind: current.kind,
                    base_url: current.base_url,
                    model: current.model,
                    timeout: current.timeout,
                    max_retries: current.max_retries,
                    max_concurrency: MaxConcurrency::new(limit).unwrap(),
                    pricing: current.pricing,
                    api_key: KeyChange::Keep,
                    reasoning_effort: current.reasoning_effort,
                    generation: current.generation,
                },
                &self.cipher,
            )
            .await
            .unwrap();
        id
    }

    fn pace(&self, delay: Duration) {
        *self.shared.pace.lock().unwrap() = delay;
    }

    fn delay(&self, delays: Vec<Duration>) {
        *self.shared.delays.lock().unwrap() = delays.into();
    }

    fn peak(&self) -> u32 {
        self.shared.running.lock().unwrap().1
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

    async fn indicated(&self, profile: ProfileId, host: &str) -> (Option<f64>, Option<Verdict>) {
        let Some(scoring) = self.scoring(profile, host).await else {
            return (None, None);
        };
        let thresholds = self.active().await.revision.definition.definition().verdict;
        let confidence = scoring.indicator_confidence;
        let verdict = if scoring.exclusions.is_empty() {
            confidence.map(|confidence| match verdict_for(confidence, thresholds) {
                Verdict::Reliable if scoring.cap.is_some() => Verdict::ToCheck,
                other => other,
            })
        } else {
            Some(Verdict::Discarded)
        };
        (confidence, verdict)
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

    fn calls_to(&self, model: &str) -> usize {
        self.seen()
            .iter()
            .filter(|seen| seen.model == model)
            .count()
    }

    async fn escalating_profile(
        &self,
        agents: Vec<Value>,
        provider: Option<ProviderId>,
    ) -> ProfileId {
        let mut value = definition(agents);
        value["escalation"]["provider"] = json!(provider.map(|provider| provider.0.to_string()));
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
            .record_observations_at(
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
                at(1_000),
            )
            .await
            .unwrap();
    }

    async fn observe_offer(&self, host: &str, excerpts: &[String], seconds: i64) {
        let active = self.active().await;
        self.usecase
            .record_observations_at(
                &active,
                SourceName::FranceTravail,
                &[Observation {
                    domain: domain(host),
                    kind: SignalKind::parse("job_offer").unwrap(),
                    value: Some("Ingénieur IAM".to_owned()),
                    raw: json!({
                        "title": "Ingénieur IAM",
                        "matched": ["keycloak"],
                        "route": "keywords",
                        "details": {"company": "Acme"},
                        "excerpts": excerpts
                    }),
                    provenance: Provenance::parse(&format!(
                        "https://candidat.francetravail.fr/offres/{host}"
                    ))
                    .unwrap(),
                    strength: UnitInterval::new(0.7).unwrap(),
                    observed_at: at(seconds),
                    agent: None,
                }],
                at(1_000),
            )
            .await
            .unwrap();
    }

    async fn link_company(&self, host: &str, siren: &str) {
        let mut dossier = CompanyDossier::pending(domain(host), at(0));
        dossier.state = DossierState::Complete;
        dossier.siren = Some(Siren::parse(siren).unwrap());
        dossier.official = Some(OfficialData {
            siren: Siren::parse(siren).unwrap(),
            legal_name: "ACME SAS".to_owned(),
            naf_code: None,
            naf_label: None,
            headcount_code: None,
            headcount_range: None,
            head_office_city: None,
            head_office_postcode: None,
            head_office_country: None,
            head_office_country_code: None,
            created_on: None,
        });
        with_tx(&self.pool, async |shared| {
            PgCompanyRepository::new(&shared).save(&dossier).await
        })
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
                max_concurrency: None,
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
        .profile(vec![agent("pertinence", None, "pertinence")])
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
        .profile(vec![agent("pertinence", None, "pertinence")])
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
    let indicated = harness.indicated(profile, "acme.com").await;
    assert_eq!(indicated.0, Some(100.0));
    assert_eq!(indicated.1, Some(Verdict::ToCheck));
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
        .profile(vec![agent("pertinence", None, "pertinence")])
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
    assert!(harness.indicated(profile, "acme.com").await.1.is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_seen_only_on_a_development_host_is_documented_as_such(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    harness.default_provider("model-default").await;
    harness
        .profile(vec![agent("pertinence", None, "pertinence")])
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
async fn the_indicator_opinion_is_the_plain_mean_of_the_agent_scores(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let good = harness.provider("model-good", 0).await;
    let bad = harness.provider("model-bad", 0).await;
    harness.script("model-bad", vec![Reply::Worst]);
    let profile = harness
        .profile(vec![
            agent("light", Some(good), "q"),
            agent("heavy", Some(bad), "q"),
        ])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let results = harness.results(profile, "acme.com").await;
    assert_eq!(result_of(&results, "light").agent_score, Some(1.0));
    assert_eq!(result_of(&results, "heavy").agent_score, Some(0.0));
    let indicated = harness.indicated(profile, "acme.com").await;
    assert_eq!(indicated.0, Some(50.0));
    assert_eq!(indicated.1, Some(Verdict::ToCheck));
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
        .profile(vec![agent("solo", Some(provider), "q")])
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
    let profile = ValidatedProfile::parse(definition(vec![agent("solo", None, "q")])).unwrap();
    let schema = lodestone_core::contract_schema(&profile.definition().qualification_agents[0]);
    answers_from(&schema, Pick::First, "lead: acme.com")
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
        .profile(vec![agent("solo", Some(provider), "q")])
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
        .profile(vec![agent("solo", Some(provider), "q")])
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
        .profile(vec![agent("solo", Some(provider), "q")])
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
    let indicated = harness.indicated(profile, "acme.com").await;
    assert_eq!(indicated.0, None);
    assert_eq!(indicated.1, None);
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
        .profile(vec![agent("solo", Some(provider), "q")])
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
            agent("ghost", Some(ghost), "q"),
            agent("off", Some(disabled), "q"),
            agent("fine", Some(provider), "q"),
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
    let indicated = harness.indicated(profile, "acme.com").await;
    assert_eq!(indicated.0, Some(100.0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_a_default_provider_agents_are_invalid_not_blocking(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let profile = harness.profile(vec![agent("solo", None, "q")]).await;
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
    let profile = harness.profile(vec![agent("solo", None, "q")]).await;
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
    let profile = harness.profile(vec![agent("solo", None, "q")]).await;
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
    let profile = harness.profile(vec![agent("solo", None, "q")]).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    assert_eq!(harness.step(settings(100, 10)).await.leads, 1);
    assert_eq!(harness.step(settings(100, 10)).await.leads, 0);

    harness
        .revise(profile, vec![revised(agent("solo", None, "q"))])
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
    harness.profile(vec![agent("solo", None, "q")]).await;
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
    let profile = harness.profile(vec![agent("solo", None, "q")]).await;
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
    assert!(harness.indicated(profile, "lead-11.com").await.1.is_some());
    assert!(harness.indicated(profile, "lead-00.com").await.1.is_none());
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
            agent("first", Some(pricey), "q"),
            agent("second", Some(pricey), "q"),
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
            agent("one", Some(provider), "q"),
            agent("two", Some(provider), "q"),
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
        .profile(vec![agent("solo", Some(provider), "q")])
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
    let mut off = agent("off", Some(provider), "q");
    off["enabled"] = json!(false);
    let profile = harness
        .profile(vec![off, agent("on", Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].agent_id.as_str(), "on");
}

fn two_axes_agent(id: &str, provider: ProviderId) -> Value {
    let mut value = agent(id, Some(provider), "q");
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
            vec![agent("pertinence", Some(provider), "q")],
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
    let indicated = harness.indicated(profile, "luzernrisk.com").await;
    let scoring = harness.scoring(profile, "luzernrisk.com").await.unwrap();
    assert!((scoring.opinion.unwrap() - score).abs() < 1e-9);
    assert!(scoring.evidence.only_non_production);
    assert_eq!(scoring.evidence.signal_count, 1);
    assert_eq!(scoring.evidence.components.environment, 0.0);
    assert_eq!(scoring.evidence.components.diversity, 0.5);
    let expected = scoring.opinion.unwrap() * (0.4 + 0.6 * scoring.evidence.score);
    assert!((indicated.0.unwrap() - expected * 100.0).abs() < 0.06);
    assert!(indicated.0.unwrap() < 70.0);
    assert_eq!(indicated.1, Some(Verdict::ToCheck));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_confident_lead_on_a_dev_host_names_the_cap(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let first = harness.provider("model-a", 0).await;
    let second = harness.provider("model-b", 0).await;
    let profile = harness
        .profile_tuned(
            vec![
                agent("one", Some(first), "q"),
                agent("two", Some(second), "q"),
            ],
            json!({"reliable_from": 40, "discard_below": 10}),
            json!({}),
        )
        .await;
    harness
        .observe("login.dev.acme.com", SourceName::DnsCname, 10)
        .await;

    harness.step(settings(100, 10)).await;

    let indicated = harness.indicated(profile, "acme.com").await;
    assert!(indicated.0.unwrap() >= 40.0);
    assert_eq!(indicated.1, Some(Verdict::ToCheck));
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

    let indicated = harness.indicated(profile, "acme.com").await;
    assert_eq!(indicated.1, Some(Verdict::Reliable));
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

    let indicated = harness.indicated(profile, "acme.com").await;
    assert_eq!(indicated.1, Some(Verdict::ToCheck));
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
            vec![agent("solo", Some(provider), "q")],
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
    let indicated_after = harness.indicated(profile, "acme.com").await;
    assert_ne!(indicated_after.0, Some(100.0));
    assert_eq!(indicated_after.1, Some(Verdict::ToCheck));
    assert_eq!(rescored.qualified_at, lead.qualified_at);
    let results = harness.results(profile, "acme.com").await;
    assert!((results[0].agent_score.unwrap() - 1.0 / 1.1).abs() < 1e-9);
    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert!(scoring.evidence.only_non_production);
    let again = harness.step(settings(100, 10)).await;
    assert_eq!(again.rescored, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_scoring_from_an_older_version_is_rescored_without_a_model_call(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile_tuned(
            vec![agent("solo", Some(provider), "q")],
            json!({}),
            json!({}),
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.step(settings(100, 10)).await;
    let calls = harness.calls();
    let stored: i32 = sqlx::query_scalar("SELECT scoring_version FROM lead_qualification_state")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, SCORING_VERSION);
    sqlx::query("UPDATE lead_qualification_state SET scoring_version = $1")
        .bind(4)
        .execute(&pool)
        .await
        .unwrap();

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.rescored, 1);
    assert_eq!(report.leads, 0);
    assert_eq!(harness.calls(), calls);
    let rescored: i32 = sqlx::query_scalar("SELECT scoring_version FROM lead_qualification_state")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rescored, SCORING_VERSION);
    assert!(harness.scoring(profile, "acme.com").await.is_some());
    assert_eq!(harness.step(settings(100, 10)).await.rescored, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_scoring_older_than_a_day_is_refreshed_and_the_old_signals_weigh_less(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile_tuned(
            vec![agent("solo", Some(provider), "q")],
            json!({}),
            json!({}),
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.step(settings(100, 10)).await;
    let calls = harness.calls();
    let before = harness.scoring(profile, "acme.com").await.unwrap();
    assert_eq!(harness.lead(profile, "acme.com").await.score.get(), 90.0);
    assert_eq!(harness.step(settings(100, 10)).await.rescored, 0);
    sqlx::query("UPDATE lead_qualification_state SET scored_at = scored_at - interval '25 hours'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE signals SET observed_at = observed_at - interval '365 days'")
        .execute(&pool)
        .await
        .unwrap();

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.rescored, 1);
    assert_eq!(harness.calls(), calls);
    let after = harness.scoring(profile, "acme.com").await.unwrap();
    assert!((before.evidence.decayed_signals - 1.0).abs() < 1e-3);
    assert!((after.evidence.decayed_signals - 0.5).abs() < 1e-3);
    assert_eq!(harness.lead(profile, "acme.com").await.score.get(), 45.0);
    assert_eq!(after.evidence.signal_score, 45.0);
    assert!(after.evidence.components.volume < before.evidence.components.volume);
    assert!(after.evidence.components.strength < before.evidence.components.strength);
    assert_eq!(harness.step(settings(100, 10)).await.rescored, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_signal_on_a_sibling_domain_rescores_the_other_leads_without_a_model_call(
    pool: PgPool,
) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile_tuned(
            vec![agent("solo", Some(provider), "q")],
            json!({}),
            json!({}),
        )
        .await;
    harness.link_company("acme.com", "552081317").await;
    harness.link_company("acme.fr", "552081317").await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.observe("acme.fr", SourceName::HttpProbe, 10).await;
    harness.step(settings(100, 10)).await;
    let calls = harness.calls();
    let before = harness.scoring(profile, "acme.com").await.unwrap();
    assert_eq!(before.evidence.signal_count, 2);
    assert_eq!(harness.step(settings(100, 10)).await.rescored, 0);
    harness
        .observe_value("acme.fr", SourceName::HttpProbe, "another", 20)
        .await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.rescored, 2);
    assert_eq!(harness.calls(), calls);
    let after = harness.scoring(profile, "acme.com").await.unwrap();
    assert_eq!(after.evidence.signal_count, 3);
    assert_eq!(before.evidence.independent_signals, 1);
    assert_eq!(after.evidence.independent_signals, 2);
    assert!(after.evidence.components.volume > before.evidence.components.volume);
    assert_eq!(harness.step(settings(100, 10)).await.rescored, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_signal_from_a_known_source_refreshes_the_evidence_without_a_model_call(
    pool: PgPool,
) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile_tuned(
            vec![agent("solo", Some(provider), "q")],
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
        .profile(vec![agent("solo", Some(provider), "q")])
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

const OFFER_QUOTE: &str = "nous migrons notre annuaire vers Keycloak pour déployer le SSO";

fn quoting_answers(quote: &str) -> Value {
    let mut value = solo_answers();
    value["answers"]["q"]["evidence"] = json!(quote);
    value
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_quote_copied_from_an_offer_excerpt_is_grounded_in_its_signal_document(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script("model-a", vec![Reply::Raw(quoting_answers(OFFER_QUOTE))]);
    let profile = harness
        .profile(vec![agent("solo", Some(provider), "q")])
        .await;
    harness
        .observe_offer(
            "acme.com",
            &[format!("…Dans ce cadre, {OFFER_QUOTE}, avec MFA…")],
            10,
        )
        .await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.valid, 1);
    assert_eq!(harness.calls(), 1);
    let documents = &harness.seen()[0].documents;
    assert!(
        documents
            .iter()
            .any(|document| document.contains("\nexcerpt: …Dans ce cadre, nous migrons"))
    );
    let results = harness.results(profile, "acme.com").await;
    let answer = &results[0].answers["q"];
    assert_eq!(answer["choice"], "forte");
    assert_eq!(answer["grounded"], true);
    assert_eq!(answer["evidence"], OFFER_QUOTE);
    assert_eq!(
        answer["evidence_source"],
        "https://candidat.francetravail.fr/offres/acme.com"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_same_quote_is_ungrounded_when_the_offer_kept_no_excerpt(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script(
        "model-a",
        vec![
            Reply::Raw(quoting_answers(OFFER_QUOTE)),
            Reply::Raw(quoting_answers(OFFER_QUOTE)),
        ],
    );
    let profile = harness
        .profile(vec![agent("solo", Some(provider), "q")])
        .await;
    harness.observe_offer("acme.com", &[], 10).await;

    harness.step(settings(100, 10)).await;

    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results[0].answers["q"]["grounded"], false);
    assert_eq!(results[0].answers["q"]["choice"], "inconnu");
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
        .profile(vec![agent("solo", Some(provider), "q")])
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
        .profile(vec![agent("solo", Some(provider), "q")])
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
        .profile(vec![agent("solo", Some(provider), "q")])
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
        .profile(vec![agent("solo", Some(provider), "q")])
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
    let profile = ValidatedProfile::parse(definition(vec![agent("solo", None, "q")])).unwrap();
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
        escalated: false,
        raw_samples: json!([]),
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
        "exclusion_threshold": threshold,
        "samples": 1,
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
            agent("solo", Some(provider), "q"),
            agent("second", Some(provider), "q"),
            agent("third", Some(provider), "q"),
            agent("fourth", Some(provider), "q"),
            excluding_agent("exclusions", provider, 0.6),
        ])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let indicated = harness.indicated(profile, "acme.com").await;
    assert_eq!(indicated.1, Some(Verdict::Discarded));
    assert!(indicated.0.unwrap() > 70.0);
    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert_eq!(scoring.reason, Some(VerdictReason::AwaitingScoring));
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
            agent("solo", Some(provider), "q"),
            agent("second", Some(provider), "q"),
            excluding_agent("exclusions", provider, 0.6),
        ])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let indicated = harness.indicated(profile, "acme.com").await;
    assert_eq!(indicated.1, Some(Verdict::Reliable));
    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert!(scoring.exclusions.is_empty());
    assert_eq!(scoring.reason, Some(VerdictReason::AwaitingScoring));
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
    let indicated = harness.indicated(profile, "acme.com").await;
    assert_ne!(indicated.1, Some(Verdict::Discarded));
    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert!(scoring.exclusions.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn rescoring_applies_exclusions_to_stored_leads_without_a_model_call(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile(vec![
            agent("solo", Some(provider), "q"),
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
    let indicated = harness.indicated(profile, "acme.com").await;
    assert_eq!(indicated.1, Some(Verdict::Discarded));
    assert_ne!(indicated.0, Some(100.0));
    assert!(indicated.0.is_some());
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, Some(Verdict::Discarded));
    assert_eq!(lead.confidence, Some(100.0));
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

#[sqlx::test(migrations = "../../migrations")]
async fn three_samples_are_three_calls_at_the_sampling_temperature_and_the_majority_wins(
    pool: PgPool,
) {
    let harness = Harness::new(pool.clone(), 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script("model-a", vec![Reply::Best, Reply::Worst, Reply::Best]);
    let profile = harness
        .profile(vec![sampled(agent("solo", Some(provider), "q"), 3)])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!((report.leads, report.valid, report.invalid), (1, 1, 0));
    assert_eq!(harness.calls(), 3);
    assert!(
        harness
            .seen()
            .iter()
            .all(|seen| seen.temperature == Some(0.7))
    );
    let cached: i64 = sqlx::query_scalar("SELECT count(*) FROM llm_cache")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cached, 3);
    let results = harness.results(profile, "acme.com").await;
    let result = &results[0];
    assert_eq!(result.status, ResultStatus::Valid);
    assert_eq!(result.answers["q"]["choice"], "forte");
    let agreement = result.answers["q"]["agreement"].as_f64().unwrap();
    assert!((agreement - 2.0 / 3.0).abs() < 1e-9);
    assert_eq!(result.provenance.samples, 3);
    assert_eq!(result.provenance.generation.temperature, Some(0.7));
    assert!(!result.escalated);
    let raw = result.raw_samples.as_array().unwrap();
    assert_eq!(raw.len(), 3);
    assert_eq!(raw[1]["answers"]["q"]["choice"], "faible");
    assert!(raw.iter().all(|sample| sample["valid"] == true));
    let forte = result.answers["q"]["probabilities"]["forte"]
        .as_f64()
        .unwrap();
    assert!((forte - (0.5 * 2.0 / 3.0 + 0.5 * 2.0 / 3.0)).abs() < 1e-9);
}

#[sqlx::test(migrations = "../../migrations")]
async fn identical_replies_still_cost_one_call_per_sample_and_replays_are_cached(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile(vec![sampled(agent("solo", Some(provider), "q"), 3)])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.step(settings(100, 10)).await;
    assert_eq!(harness.calls(), 3);

    sqlx::query("DELETE FROM lead_qualification_state")
        .execute(&pool)
        .await
        .unwrap();
    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.leads, 1);
    assert_eq!(harness.calls(), 3);
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results[0].answers["q"]["agreement"], 1.0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_budget_is_charged_per_sample_call(pool: PgPool) {
    let harness = Harness::new(pool, 1_000_000);
    let pricey = harness.provider("model-pricey", 1).await;
    let profile = harness
        .profile(vec![sampled(agent("solo", Some(pricey), "q"), 3)])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(2, 10)).await;

    assert!(report.budget_exhausted);
    assert_eq!(report.leads, 0);
    assert_eq!(harness.calls(), 2);
    assert!(
        harness
            .lead(profile, "acme.com")
            .await
            .qualified_at
            .is_none()
    );

    let report = harness.step(settings(10, 10)).await;

    assert!(!report.budget_exhausted);
    assert_eq!(report.leads, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_invalid_sample_is_dropped_and_the_others_vote(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script(
        "model-a",
        vec![Reply::Best, Reply::Fail(LlmError::Refused), Reply::Best],
    );
    let profile = harness
        .profile(vec![sampled(agent("solo", Some(provider), "q"), 3)])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!((report.valid, report.invalid), (1, 0));
    let results = harness.results(profile, "acme.com").await;
    let result = &results[0];
    assert_eq!(result.status, ResultStatus::Valid);
    assert_eq!(result.provenance.samples, 2);
    assert_eq!(result.answers["q"]["choice"], "forte");
    assert_eq!(result.answers["q"]["agreement"], 1.0);
    let raw = result.raw_samples.as_array().unwrap();
    assert_eq!(raw.len(), 3);
    assert_eq!(raw[1]["valid"], false);
    assert!(raw[1]["error"].as_str().unwrap().contains("Échec"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_result_is_invalid_only_when_every_sample_is_invalid(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script(
        "model-a",
        vec![
            Reply::Fail(LlmError::Refused),
            Reply::Fail(LlmError::Refused),
            Reply::Fail(LlmError::Refused),
        ],
    );
    let profile = harness
        .profile(vec![sampled(agent("solo", Some(provider), "q"), 3)])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!((report.valid, report.invalid), (0, 1));
    let results = harness.results(profile, "acme.com").await;
    let result = &results[0];
    assert_eq!(result.status, ResultStatus::Invalid);
    assert!(result.error.as_deref().unwrap().contains("Échec"));
    assert_eq!(result.raw_samples.as_array().unwrap().len(), 3);
    assert_eq!(result.provenance.samples, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_single_sample_keeps_the_provider_temperature_and_stores_no_agreement(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile(vec![agent("solo", Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    assert_eq!(harness.calls(), 1);
    assert_eq!(harness.seen()[0].temperature, None);
    let results = harness.results(profile, "acme.com").await;
    assert!(results[0].answers["q"].get("agreement").is_none());
    assert_eq!(results[0].provenance.samples, 1);
    assert_eq!(results[0].raw_samples, json!([]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn exclusions_read_the_voted_answers(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script("model-a", vec![Reply::Best, Reply::Worst, Reply::Best]);
    let profile = harness
        .profile(vec![sampled(
            excluding_agent("exclusions", provider, 0.6),
            3,
        )])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert_eq!(scoring.exclusions.len(), 1);
    assert!((scoring.exclusions[0].probability - 2.0 / 3.0).abs() < 1e-9);
    assert_eq!(
        harness.indicated(profile, "acme.com").await.1,
        Some(Verdict::Discarded)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_minority_exclusion_does_not_exclude(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script("model-a", vec![Reply::Best, Reply::Worst, Reply::Worst]);
    let profile = harness
        .profile(vec![sampled(
            excluding_agent("exclusions", provider, 0.6),
            3,
        )])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let scoring = harness.scoring(profile, "acme.com").await.unwrap();
    assert!(scoring.exclusions.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_grey_zone_lead_is_re_evaluated_by_the_escalation_provider(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let local = harness.provider("model-a", 0).await;
    let strong = harness.provider("model-b", 0).await;
    harness.script("model-a", vec![Reply::Middle, Reply::Middle]);
    harness.script("model-b", vec![Reply::Best, Reply::Best]);
    let profile = harness
        .escalating_profile(
            vec![
                agent("first", Some(local), "q"),
                agent("second", Some(local), "q"),
            ],
            Some(strong),
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!((report.leads, report.valid, report.invalid), (1, 2, 0));
    assert_eq!(harness.calls_to("model-a"), 2);
    assert_eq!(harness.calls_to("model-b"), 2);
    for seen in harness.seen().iter().filter(|seen| seen.model == "model-b") {
        assert_eq!(seen.temperature, Some(0.0));
    }
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results.len(), 2);
    for result in &results {
        assert!(result.escalated);
        assert_eq!(result.provenance.provider_id, Some(strong));
        assert_eq!(result.provenance.provider_name, "model-b");
        assert_eq!(result.provenance.model, "model-b");
        assert_eq!(result.provenance.samples, 1);
        assert_eq!(result.provenance.generation.temperature, Some(0.0));
        assert_eq!(result.answers["q"]["choice"], "forte");
        assert_eq!(result.status, ResultStatus::Valid);
    }
    let indicated = harness.indicated(profile, "acme.com").await;
    assert_eq!(indicated.0, Some(100.0));
    assert_eq!(indicated.1, Some(Verdict::Reliable));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_confident_lead_is_not_escalated(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let local = harness.provider("model-a", 0).await;
    let strong = harness.provider("model-b", 0).await;
    let profile = harness
        .escalating_profile(vec![agent("first", Some(local), "q")], Some(strong))
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    assert_eq!(harness.calls_to("model-b"), 0);
    let results = harness.results(profile, "acme.com").await;
    assert!(!results[0].escalated);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_band_is_inclusive_below_and_exclusive_above(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let local = harness.provider("model-a", 0).await;
    let strong = harness.provider("model-b", 0).await;
    harness.script("model-a", vec![Reply::Middle, Reply::Middle]);
    let mut value = definition(vec![agent("first", Some(local), "q")]);
    value["escalation"]["provider"] = json!(strong.0.to_string());
    value["escalation"]["band_low"] = json!(10);
    value["escalation"]["band_high"] = json!(50);
    let profile = harness
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
    harness
        .usecase
        .set_profile_active(profile, true)
        .await
        .unwrap();
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    assert_eq!(harness.indicated(profile, "acme.com").await.0, Some(50.0));
    assert_eq!(harness.calls_to("model-b"), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_an_escalation_provider_a_grey_zone_lead_stays_as_it_is(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let local = harness.provider("model-a", 0).await;
    harness.script("model-a", vec![Reply::Middle]);
    let profile = harness
        .escalating_profile(vec![agent("first", Some(local), "q")], None)
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    assert_eq!(harness.calls(), 1);
    let indicated = harness.indicated(profile, "acme.com").await;
    assert_eq!(indicated.0, Some(50.0));
    assert!(!harness.results(profile, "acme.com").await[0].escalated);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_replayed_evaluation_reuses_the_cached_escalation_without_new_calls(pool: PgPool) {
    let harness = Harness::new(pool.clone(), 10);
    let local = harness.provider("model-a", 0).await;
    let strong = harness.provider("model-b", 0).await;
    harness.script("model-a", vec![Reply::Middle, Reply::Middle, Reply::Middle]);
    let profile = harness
        .escalating_profile(vec![agent("first", Some(local), "q")], Some(strong))
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.step(settings(100, 10)).await;
    assert_eq!(harness.calls_to("model-b"), 1);

    sqlx::query("DELETE FROM lead_qualification_state")
        .execute(&pool)
        .await
        .unwrap();
    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.leads, 1);
    assert_eq!(harness.calls_to("model-a"), 1);
    assert_eq!(harness.calls_to("model-b"), 1);
    assert!(harness.results(profile, "acme.com").await[0].escalated);
    assert_eq!(harness.indicated(profile, "acme.com").await.0, Some(100.0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_that_becomes_due_again_and_stays_grey_is_escalated_again(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let local = harness.provider("model-a", 0).await;
    let strong = harness.provider("model-b", 0).await;
    harness.script("model-a", vec![Reply::Middle, Reply::Middle]);
    harness.script("model-b", vec![Reply::Middle, Reply::Middle]);
    let profile = harness
        .escalating_profile(vec![agent("first", Some(local), "q")], Some(strong))
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.step(settings(100, 10)).await;
    assert_eq!(harness.calls_to("model-b"), 1);

    harness.observe("acme.com", SourceName::Ats, 20).await;
    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.leads, 1);
    assert_eq!(harness.calls_to("model-a"), 2);
    assert_eq!(harness.calls_to("model-b"), 2);
    assert!(harness.results(profile, "acme.com").await[0].escalated);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_revision_may_escalate_the_lead_again(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let local = harness.provider("model-a", 0).await;
    let strong = harness.provider("model-b", 0).await;
    harness.script("model-a", vec![Reply::Middle, Reply::Middle]);
    let profile = harness
        .escalating_profile(vec![agent("first", Some(local), "q")], Some(strong))
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.step(settings(100, 10)).await;
    assert_eq!(harness.calls_to("model-b"), 1);

    let mut value = definition(vec![agent("first", Some(local), "q")]);
    value["escalation"]["provider"] = json!(strong.0.to_string());
    value["identity"]["offer"] = json!("Managed IAM, second revision");
    harness
        .usecase
        .revise_profile(ReviseProfileCommand {
            profile_id: profile,
            definition: ValidatedProfile::parse(value).unwrap(),
            created_by: None,
        })
        .await
        .unwrap();
    harness.step(settings(100, 10)).await;

    assert_eq!(harness.calls_to("model-b"), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_escalation_keeps_the_original_results(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let local = harness.provider("model-a", 0).await;
    let strong = harness.provider("model-b", 0).await;
    harness.script("model-a", vec![Reply::Middle]);
    harness.script("model-b", vec![Reply::Fail(LlmError::Refused)]);
    let profile = harness
        .escalating_profile(vec![agent("first", Some(local), "q")], Some(strong))
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!((report.valid, report.invalid), (1, 0));
    let results = harness.results(profile, "acme.com").await;
    assert_eq!(results[0].status, ResultStatus::Valid);
    assert!(!results[0].escalated);
    assert_eq!(results[0].provenance.provider_id, Some(local));
    assert_eq!(harness.indicated(profile, "acme.com").await.0, Some(50.0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn escalation_spends_the_run_budget_and_a_replay_reuses_the_cached_calls(pool: PgPool) {
    let harness = Harness::new(pool, 1_000_000);
    let local = harness.provider("model-a", 100).await;
    let strong = harness.provider("model-b", 100).await;
    harness.script("model-a", vec![Reply::Middle, Reply::Middle]);
    let profile = harness
        .escalating_profile(vec![agent("first", Some(local), "q")], Some(strong))
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(1, 10)).await;

    assert!(report.budget_exhausted);
    assert_eq!(harness.calls_to("model-a"), 1);
    assert_eq!(harness.calls_to("model-b"), 0);
    assert!(
        harness
            .lead(profile, "acme.com")
            .await
            .qualified_at
            .is_none()
    );

    let report = harness.step(settings(10, 10)).await;

    assert!(!report.budget_exhausted);
    assert_eq!(report.leads, 1);
    assert_eq!(harness.calls_to("model-a"), 1);
    assert_eq!(harness.calls_to("model-b"), 1);
    assert!(harness.results(profile, "acme.com").await[0].escalated);
}

#[sqlx::test(migrations = "../../migrations")]
async fn disabling_an_agent_during_a_pass_stops_the_lead_and_stores_nothing(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    harness.default_provider("model-default").await;
    let profile = harness
        .profile(vec![
            agent("pertinence", None, "pertinence"),
            agent("maturite", None, "maturite"),
        ])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let mut changed = harness.active().await;
    let mut disabled = agent("maturite", None, "maturite");
    disabled["enabled"] = json!(false);
    changed.revision.number = changed.revision.number.next().unwrap();
    changed.revision.definition = ValidatedProfile::parse(definition(vec![
        agent("pertinence", None, "pertinence"),
        disabled,
    ]))
    .unwrap();
    *harness.shared.on_call.lock().unwrap() = Some((harness.usecase.clone(), changed));

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.stopped, 1);
    assert_eq!(report.leads, 0);
    assert_eq!(harness.calls(), 1);
    assert!(harness.results(profile, "acme.com").await.is_empty());
    let lead = harness.lead(profile, "acme.com").await;
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    assert!(detail.scoring.is_none());
    assert_eq!(detail.agent_stops.len(), 1);
    assert_eq!(detail.agent_stops[0].target.kind, WorkKind::Qualification);
    assert_eq!(detail.agent_stops[0].target.agent.as_str(), "maturite");
    assert_eq!(detail.agent_stops[0].reason, CancelReason::AgentDisabled);
}

const HOSTS: [&str; 4] = ["acme.com", "globex.com", "initech.com", "umbrella.com"];

#[sqlx::test(migrations = "../../migrations")]
async fn leads_agents_and_samples_run_at_once_within_the_provider_limit(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.limited_provider("model-a", 0, 3).await;
    let profile = harness
        .profile(vec![
            sampled(agent("first", Some(provider), "q"), 2),
            sampled(agent("second", Some(provider), "r"), 2),
        ])
        .await;
    for host in &HOSTS[..3] {
        harness.observe(host, SourceName::HttpProbe, 10).await;
    }
    harness.pace(Duration::from_millis(40));

    let report = harness.step(concurrent(100, 10, 4)).await;

    assert_eq!((report.leads, report.valid, report.invalid), (3, 6, 0));
    assert_eq!(harness.calls(), 12);
    let peak = harness.peak();
    assert!(peak > 1, "calls never overlapped");
    assert!(peak <= 3, "the provider limit was exceeded: {peak}");
    assert_eq!(harness.usecase.llm_in_flight(provider), 0);
    for host in &HOSTS[..3] {
        assert_eq!(harness.results(profile, host).await.len(), 2);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lower_provider_limit_serialises_the_calls(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.limited_provider("model-a", 0, 1).await;
    harness
        .profile(vec![
            agent("first", Some(provider), "q"),
            agent("second", Some(provider), "r"),
        ])
        .await;
    for host in &HOSTS[..2] {
        harness.observe(host, SourceName::HttpProbe, 10).await;
    }
    harness.pace(Duration::from_millis(20));

    let report = harness.step(concurrent(100, 10, 4)).await;

    assert_eq!(report.leads, 2);
    assert_eq!(harness.calls(), 4);
    assert_eq!(harness.peak(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn votes_and_records_follow_the_sample_order_whatever_the_completion_order(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.limited_provider("model-a", 0, 3).await;
    let profile = harness
        .profile(vec![sampled(agent("solo", Some(provider), "q"), 3)])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.script("model-a", vec![Reply::Worst, Reply::Best, Reply::Best]);
    harness.delay(vec![
        Duration::from_millis(90),
        Duration::from_millis(60),
        Duration::from_millis(30),
    ]);

    let report = harness.step(concurrent(100, 10, 4)).await;

    assert_eq!((report.leads, report.valid), (1, 1));
    assert!(harness.peak() > 1);
    let results = harness.results(profile, "acme.com").await;
    let result = &results[0];
    assert_eq!(result.answers["q"]["choice"], "forte");
    let agreement = result.answers["q"]["agreement"].as_f64().unwrap();
    assert!((agreement - 2.0 / 3.0).abs() < 1e-9);
    let indexes: Vec<u64> = result
        .raw_samples
        .as_array()
        .unwrap()
        .iter()
        .map(|sample| sample["index"].as_u64().unwrap())
        .collect();
    assert_eq!(indexes, [0, 1, 2]);
    let choices: Vec<&str> = result
        .raw_samples
        .as_array()
        .unwrap()
        .iter()
        .map(|sample| sample["answers"]["q"]["choice"].as_str().unwrap())
        .collect();
    assert_eq!(
        choices.iter().filter(|choice| **choice == "forte").count(),
        2
    );
    assert_eq!(
        choices.iter().filter(|choice| **choice == "faible").count(),
        1
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_leads_stop_at_the_budget_exactly(pool: PgPool) {
    let harness = Harness::new(pool, 4_000);
    let pricey = harness.limited_provider("model-pricey", 1_000, 8).await;
    let profile = harness
        .profile(vec![agent("solo", Some(pricey), "q")])
        .await;
    for host in HOSTS {
        harness.observe(host, SourceName::HttpProbe, 10).await;
    }
    harness.pace(Duration::from_millis(30));

    let report = harness.step(concurrent(10, 10, 4)).await;

    assert!(report.budget_exhausted);
    assert_eq!(harness.calls(), 3);
    assert_eq!(report.leads, 3);
    let mut qualified = 0;
    for host in HOSTS {
        if harness.lead(profile, host).await.qualified_at.is_some() {
            qualified += 1;
        }
    }
    assert_eq!(qualified, 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn disabling_an_agent_mid_flight_drops_the_running_calls_and_stores_nothing(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    harness.default_provider("model-default").await;
    let profile = harness
        .profile(vec![
            agent("pertinence", None, "pertinence"),
            agent("maturite", None, "maturite"),
        ])
        .await;
    for host in &HOSTS[..3] {
        harness.observe(host, SourceName::HttpProbe, 10).await;
    }
    let mut changed = harness.active().await;
    let mut disabled = agent("maturite", None, "maturite");
    disabled["enabled"] = json!(false);
    changed.revision.number = changed.revision.number.next().unwrap();
    changed.revision.definition = ValidatedProfile::parse(definition(vec![
        agent("pertinence", None, "pertinence"),
        disabled,
    ]))
    .unwrap();
    *harness.shared.on_call.lock().unwrap() = Some((harness.usecase.clone(), changed));
    harness.pace(Duration::from_millis(200));

    let report = harness.step(concurrent(100, 10, 4)).await;

    assert_eq!(report.leads, 0);
    assert!(report.stopped >= 1);
    assert!(harness.peak() > 1);
    for host in &HOSTS[..3] {
        assert!(harness.results(profile, host).await.is_empty());
        assert!(harness.lead(profile, host).await.qualified_at.is_none());
    }
}

impl Harness {
    async fn timeline(&self, profile: ProfileId, host: &str) -> Vec<TimelineEvent> {
        let lead = self.lead(profile, host).await;
        self.usecase
            .lead_timeline(lead.id)
            .await
            .unwrap()
            .rounds
            .into_iter()
            .flat_map(|round| round.events)
            .collect()
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_qualification_records_its_round_agents_samples_and_combination(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script(
        "model-a",
        vec![Reply::Best, Reply::Fail(LlmError::Refused), Reply::Best],
    );
    let profile = harness
        .profile(vec![sampled(agent("solo", Some(provider), "q"), 3)])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let lead = harness.lead(profile, "acme.com").await;
    let timeline = harness.usecase.lead_timeline(lead.id).await.unwrap();
    assert_eq!(timeline.rounds.len(), 1);
    assert_eq!(timeline.rounds[0].number, 1);
    assert_eq!(timeline.rounds[0].trigger, RoundTrigger::Detection);
    assert_eq!(timeline.rounds[0].sources, vec!["http_probe".to_owned()]);
    let events = &timeline.rounds[0].events;
    let steps: Vec<(TimelineStep, &str, EventStatus)> = events
        .iter()
        .map(|event| (event.step, event.kind.as_str(), event.status))
        .collect();
    for expected in [
        (
            TimelineStep::Detection,
            timeline_kind::SIGNAL,
            EventStatus::Succeeded,
        ),
        (
            TimelineStep::Qualification,
            ROUND_STARTED,
            EventStatus::Started,
        ),
        (
            TimelineStep::Qualification,
            timeline_kind::AGENT,
            EventStatus::Succeeded,
        ),
        (
            TimelineStep::Qualification,
            timeline_kind::SAMPLE_INVALID,
            EventStatus::Failed,
        ),
        (
            TimelineStep::Combination,
            timeline_kind::COMBINED,
            EventStatus::Succeeded,
        ),
    ] {
        assert!(
            steps.contains(&expected),
            "{expected:?} missing in {steps:?}"
        );
    }
    assert!(
        !steps.contains(&(
            TimelineStep::Qualification,
            timeline_kind::AGENT,
            EventStatus::Started
        )),
        "the agent start is folded into its end"
    );
    let agent = events
        .iter()
        .find(|event| event.kind == timeline_kind::AGENT)
        .unwrap();
    assert_eq!(agent.actor.agent_id.as_deref(), Some("solo"));
    assert_eq!(agent.actor.model.as_deref(), Some("model-a"));
    assert!(agent.finished_at.is_some());
    assert!(timeline.in_flight.is_empty());
    assert!(harness.usecase.in_flight().is_empty());
    let stage = |stage: Stage| {
        timeline
            .pipeline
            .stages
            .iter()
            .find(|state| state.stage == stage)
            .unwrap()
            .clone()
    };
    assert_eq!(stage(Stage::Qualification).status, StageStatus::Done);

    harness.step(settings(100, 10)).await;
    assert_eq!(
        harness.timeline(profile, "acme.com").await.len(),
        events.len()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_model_call_is_recorded_and_the_pipeline_waits_for_the_backoff(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let provider = harness.provider("model-a", 0).await;
    harness.script("model-a", vec![Reply::Fail(LlmError::Timeout)]);
    let profile = harness
        .profile(vec![agent("solo", Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.step(settings(100, 10)).await;

    assert_eq!(report.deferred, 1);
    let lead = harness.lead(profile, "acme.com").await;
    let timeline = harness.usecase.lead_timeline(lead.id).await.unwrap();
    let events: Vec<&TimelineEvent> = timeline
        .rounds
        .iter()
        .flat_map(|round| round.events.iter())
        .collect();
    let failure = events
        .iter()
        .find(|event| event.kind == timeline_kind::CALL_FAILED)
        .unwrap();
    assert_eq!(failure.status, EventStatus::Waiting);
    assert_eq!(failure.detail["attempts"], 1);
    assert!(failure.error.as_deref().unwrap().contains("Échec"));
    let agent = events
        .iter()
        .find(|event| event.kind == timeline_kind::AGENT)
        .unwrap();
    assert_eq!(agent.status, EventStatus::Failed);
    let qualification = timeline
        .pipeline
        .stages
        .iter()
        .find(|state| state.stage == Stage::Qualification)
        .unwrap();
    assert_eq!(qualification.status, StageStatus::Waiting);
    assert_eq!(
        qualification.reason_code,
        Some(StageReason::CallFailureBackoff)
    );
    assert!(qualification.next_at.unwrap() > at(1_000));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_escalation_is_recorded_with_its_agents(pool: PgPool) {
    let harness = Harness::new(pool, 10);
    let local = harness.provider("model-a", 0).await;
    let strong = harness.provider("model-b", 0).await;
    harness.script("model-a", vec![Reply::Middle, Reply::Middle]);
    harness.script("model-b", vec![Reply::Best, Reply::Best]);
    let profile = harness
        .escalating_profile(
            vec![
                agent("first", Some(local), "q"),
                agent("second", Some(local), "q"),
            ],
            Some(strong),
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.step(settings(100, 10)).await;

    let events = harness.timeline(profile, "acme.com").await;
    assert!(
        events
            .iter()
            .any(|event| event.kind == timeline_kind::ESCALATED)
    );
    let escalated: Vec<&TimelineEvent> = events
        .iter()
        .filter(|event| {
            event.step == TimelineStep::Escalation && event.kind == timeline_kind::AGENT
        })
        .collect();
    assert_eq!(escalated.len(), 2);
    assert!(
        escalated
            .iter()
            .all(|event| event.actor.model.as_deref() == Some("model-b"))
    );
    let primary = events
        .iter()
        .filter(|event| {
            event.step == TimelineStep::Qualification && event.kind == timeline_kind::AGENT
        })
        .count();
    assert_eq!(primary, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn running_agents_are_listed_in_flight_while_they_wait_for_the_model(pool: PgPool) {
    let harness = Arc::new(Harness::new(pool, 10));
    let provider = harness.provider("model-a", 0).await;
    let profile = harness
        .profile(vec![agent("solo", Some(provider), "q")])
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let lead = harness.lead(profile, "acme.com").await;
    harness.pace(Duration::from_millis(400));
    let running = Arc::clone(&harness);
    let pass = spawn(async move { running.step(settings(100, 10)).await });
    let mut seen = Vec::new();
    for _ in 0..40 {
        seen = harness
            .usecase
            .lead_timeline(lead.id)
            .await
            .unwrap()
            .in_flight;
        if !seen.is_empty() {
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].step, TimelineStep::Qualification);
    assert_eq!(seen[0].actor.agent_id.as_deref(), Some("solo"));
    let timeline = harness.usecase.lead_timeline(lead.id).await.unwrap();
    let qualification = timeline
        .pipeline
        .stages
        .iter()
        .find(|state| state.stage == Stage::Qualification)
        .unwrap();
    assert_eq!(qualification.status, StageStatus::Running);
    pass.await.unwrap();
    assert!(harness.usecase.in_flight().is_empty());
}
