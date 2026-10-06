mod support;

use std::{
    collections::{HashMap, VecDeque},
    num::NonZeroU32,
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use common::{EncryptionKey, Secret};
use lodestone_core::{
    ActiveProfile, Assessment, Clock, CreateProfileCommand, DraftEdit, DraftStatus, Lead,
    LeadRepository, LeadStatus, LlmError, LlmProvider, LlmRequest, LlmResponse, LodestoneUseCase,
    Observation, ProfileId, ProfileName, Provenance, ProviderDefinition, ProviderId, ProviderKind,
    ReviseProfileCommand, SignalKind, SourceName, SummaryState, Tier, TokenPricing, TokenUsage,
    UnitInterval, UserId, ValidatedProfile, Verdict, VerdictFilter,
    evaluation::{EvaluationSettings, ProviderBuilder},
    infrastructure::{
        crypto::AesGcmCipher, lead::postgres::PgLeadRepository, llm_cache::postgres::PgLlmCache,
        postgres::with_tx,
    },
    llm::CreateLlmProvider,
    qualification::AggregationReport,
    summary_retry_delay,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{at, domain};

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const CLASSIFIER: &str = "model-class";
const SYNTHESIS: &str = "model-synth";
const GOOD_QUOTE: &str = "value: keycloak-http_probe";

#[derive(Clone)]
struct TestClock(Arc<AtomicI64>);

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        at(1_000 + self.0.load(Ordering::SeqCst))
    }
}

#[derive(Debug, Clone)]
enum Reply {
    Good,
    Quote(&'static str),
    Raw(Value),
    Fail(LlmError),
    Worst,
}

#[derive(Debug, Clone)]
struct Seen {
    model: String,
    system: String,
    instruction: String,
    documents: Vec<(String, String)>,
    aggregator: bool,
}

struct Shared {
    replies: Mutex<HashMap<String, VecDeque<Reply>>>,
    seen: Mutex<Vec<Seen>>,
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

fn summary_with(quote: &str) -> Value {
    json!({
        "summary": "Le domaine expose un Keycloak ancien.",
        "verdict_narrative": "Les agents jugent le lead pertinent.",
        "approach_angle": "Proposer une migration accompagnée.",
        "evidence": [{"document_index": 0, "quote": quote}],
        "draft_message": {"subject": "Votre Keycloak", "body": "Bonjour, un échange de 20 minutes ?"}
    })
}

impl LlmProvider for Scripted {
    async fn complete(&self, request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        let aggregator = request.schema["properties"].get("summary").is_some();
        self.shared.seen.lock().unwrap().push(Seen {
            model: self.model.clone(),
            system: request.system.to_owned(),
            instruction: request.instruction.to_owned(),
            documents: request
                .documents
                .iter()
                .map(|document| (document.source.to_owned(), document.content.to_owned()))
                .collect(),
            aggregator,
        });
        let reply = self
            .shared
            .replies
            .lock()
            .unwrap()
            .get_mut(&self.model)
            .and_then(VecDeque::pop_front)
            .unwrap_or(Reply::Good);
        let output = match (reply, aggregator) {
            (Reply::Fail(error), _) => return Err(error),
            (Reply::Raw(value), _) => value,
            (Reply::Quote(quote), _) => summary_with(quote),
            (Reply::Worst, false) => answers_from(request.schema, true),
            (_, true) => summary_with(GOOD_QUOTE),
            (_, false) => answers_from(request.schema, false),
        };
        Ok(LlmResponse {
            output,
            model: self.model.clone(),
            usage: TokenUsage {
                input_tokens: 10,
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
    clock: TestClock,
}

fn settings(budget_cents: u32, batch_size: u32) -> EvaluationSettings {
    EvaluationSettings {
        max_tokens: 512,
        budget_cents: NonZeroU32::new(budget_cents).unwrap(),
        batch_size,
        base_delay: Duration::ZERO,
    }
}

fn agent(id: &str, weight: u8, provider: Option<ProviderId>) -> Value {
    json!({
        "id": id,
        "name": id,
        "enabled": true,
        "provider_id": provider.map(|provider| provider.0.to_string()),
        "system_prompt": format!("PROMPT-OF-{id}"),
        "weight": weight,
        "questions": [{
            "id": "q",
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

fn definition(agents: Vec<Value>, aggregator_provider: Option<ProviderId>) -> Value {
    let mut definition: Value = serde_json::from_str(IAM).unwrap();
    definition["qualification_agents"] = Value::Array(agents);
    definition["aggregator"]["system_prompt"] = json!("AGGREGATOR-PROMPT");
    definition["aggregator"]["provider_id"] =
        json!(aggregator_provider.map(|provider| provider.0.to_string()));
    definition
}

fn cipher() -> AesGcmCipher {
    let encoded = format!("{}QkI=", "QkJC".repeat(10));
    AesGcmCipher::new(&EncryptionKey::from_base64(&Secret::new(encoded)).unwrap()).unwrap()
}

impl Harness {
    fn new(pool: PgPool) -> Self {
        Self {
            usecase: LodestoneUseCase::new(pool.clone()),
            pool,
            cipher: cipher(),
            clock: TestClock(Arc::new(AtomicI64::new(0))),
            shared: Arc::new(Shared {
                replies: Mutex::new(HashMap::new()),
                seen: Mutex::new(Vec::new()),
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

    async fn defaults(&self) {
        let classifier = self.provider(CLASSIFIER, 0).await;
        let synthesis = self.provider(SYNTHESIS, 0).await;
        self.usecase
            .assign_llm_tier(Tier::Triage, classifier)
            .await
            .unwrap();
        self.usecase
            .assign_llm_tier(Tier::Qualification, synthesis)
            .await
            .unwrap();
    }

    async fn profile(&self, agents: Vec<Value>, aggregator: Option<ProviderId>) -> ProfileId {
        let id = self
            .usecase
            .create_profile(CreateProfileCommand {
                name: ProfileName::parse("iam").unwrap(),
                definition: ValidatedProfile::parse(definition(agents, aggregator)).unwrap(),
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
                definition: ValidatedProfile::parse(definition(agents, None)).unwrap(),
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

    async fn user(&self) -> UserId {
        let id = uuid::Uuid::now_v7();
        sqlx::query("INSERT INTO users (id, sub, username, display_name) VALUES ($1, $1::text, 'alice', 'Alice') ON CONFLICT DO NOTHING")
            .bind(id)
            .execute(&self.pool)
            .await
            .unwrap();
        UserId(id)
    }

    fn aggregator_calls(&self) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|call| call.aggregator)
            .collect()
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

    fn set_time(&self, seconds: i64) {
        self.clock.0.store(seconds, Ordering::SeqCst);
    }

    async fn summary_state(&self, profile: ProfileId, host: &str) -> Option<SummaryState> {
        let lead = self.lead(profile, host).await;
        self.usecase
            .lead_detail(lead.id)
            .await
            .unwrap()
            .summary_state
    }

    fn builder(&self) -> FakeBuilder {
        FakeBuilder {
            shared: Arc::clone(&self.shared),
        }
    }

    async fn evaluate(&self) {
        self.usecase
            .qualify_due_leads(
                &self.builder(),
                &self.cipher,
                &PgLlmCache::new(self.pool.clone()),
                settings(100, 10),
                &self.clock,
            )
            .await
            .unwrap();
    }

    async fn summarise(&self, settings: EvaluationSettings) -> AggregationReport {
        self.usecase
            .summarise_due_leads(
                &self.builder(),
                &self.cipher,
                &PgLlmCache::new(self.pool.clone()),
                settings,
                &self.clock,
            )
            .await
            .unwrap()
    }

    async fn pass(&self) -> AggregationReport {
        self.evaluate().await;
        self.summarise(settings(100, 10)).await
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

    async fn assessments(&self, profile: ProfileId, host: &str) -> Vec<Assessment> {
        let lead = self.lead(profile, host).await;
        self.usecase.lead_detail(lead.id).await.unwrap().assessments
    }

    async fn queue(&self, verdicts: &[VerdictFilter]) -> Vec<String> {
        self.usecase
            .list_pending_drafts(None, verdicts, 50, 0)
            .await
            .unwrap()
            .items
            .iter()
            .map(|entry| entry.record.lead.domain.as_str().to_owned())
            .collect()
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_summary_is_written_once_every_agent_has_a_result(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness
        .profile(vec![agent("one", 5, None), agent("two", 5, None)], None)
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let before = harness.summarise(settings(100, 10)).await;
    assert_eq!(before.leads, 0);
    assert!(harness.aggregator_calls().is_empty());

    let report = harness.pass().await;

    assert_eq!(report.leads, 1);
    assert_eq!(report.unverified, 0);
    assert_eq!(report.backlog, 0);
    let assessments = harness.assessments(profile, "acme.com").await;
    assert_eq!(assessments.len(), 1);
    let stored = &assessments[0];
    assert_eq!(stored.summary, "Le domaine expose un Keycloak ancien.");
    assert_eq!(
        stored.verdict_narrative,
        "Les agents jugent le lead pertinent."
    );
    assert_eq!(stored.approach_angle, "Proposer une migration accompagnée.");
    assert!(stored.evidence_verified);
    assert_eq!(stored.evidence.len(), 1);
    assert_eq!(stored.evidence[0].quote, GOOD_QUOTE);
    assert_eq!(
        stored.evidence[0].provenance.as_str(),
        "https://acme.com/http_probe"
    );
    assert_eq!(stored.model, SYNTHESIS);
    assert!(stored.prompt_version.starts_with("aggregator@"));
    assert_eq!(harness.aggregator_calls().len(), 1);
    assert_eq!(
        harness.lead(profile, "acme.com").await.status,
        LeadStatus::Qualified
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_missing_an_agent_result_is_not_summarised(pool: PgPool) {
    let harness = Harness::new(pool.clone());
    harness.defaults().await;
    let profile = harness
        .profile(vec![agent("one", 5, None), agent("two", 5, None)], None)
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.evaluate().await;
    sqlx::query("DELETE FROM qualification_results WHERE agent_id = 'two'")
        .execute(&pool)
        .await
        .unwrap();

    let report = harness.summarise(settings(100, 10)).await;

    assert_eq!(report.leads, 0);
    assert!(harness.aggregator_calls().is_empty());
    assert!(harness.assessments(profile, "acme.com").await.is_empty());
    assert_eq!(report.backlog, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_agent_results_still_complete_the_set(pool: PgPool) {
    let harness = Harness::new(pool);
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let synthesis = harness.provider(SYNTHESIS, 0).await;
    harness
        .usecase
        .assign_llm_tier(Tier::Qualification, synthesis)
        .await
        .unwrap();

    let report = harness.pass().await;

    assert_eq!(report.leads, 1);
    let calls = harness.aggregator_calls();
    assert_eq!(calls.len(), 1);
    assert!(
        calls[0]
            .instruction
            .contains("confidence not computed; verdict pending")
    );
    let agent_block = &calls[0].documents.last().unwrap().1;
    assert!(agent_block.contains("status: invalid, left out of the confidence"));
    assert!(agent_block.contains("fournisseur indisponible"));
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.confidence, None);
    assert_eq!(lead.verdict, None);
    assert_eq!(harness.assessments(profile, "acme.com").await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_aggregator_uses_its_provider_else_the_synthesis_default(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let own = harness.provider("model-own", 0).await;
    let profile = harness
        .profile(vec![agent("solo", 5, None)], Some(own))
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.observe("beta.com", SourceName::HttpProbe, 11).await;
    harness.pass().await;
    let calls = harness.aggregator_calls();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|call| call.model == "model-own"));
    assert!(
        harness
            .seen()
            .iter()
            .filter(|call| !call.aggregator)
            .all(|call| call.model == CLASSIFIER)
    );
    assert_eq!(
        harness.assessments(profile, "acme.com").await[0].model,
        "model-own"
    );

    harness.revise(profile, vec![agent("solo", 5, None)]).await;
    harness.pass().await;

    let latest = harness.aggregator_calls();
    assert!(latest.iter().skip(2).all(|call| call.model == SYNTHESIS));
    assert_eq!(latest.len(), 4);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_prompt_holds_the_aggregator_context_signals_agents_and_computed_facts(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    harness
        .profile(vec![agent("one", 5, None), agent("two", 5, None)], None)
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.observe("acme.com", SourceName::DnsCname, 11).await;

    harness.pass().await;

    let calls = harness.aggregator_calls();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert!(call.system.starts_with("AGGREGATOR-PROMPT"));
    assert!(call.system.contains("Offer: "));
    assert!(call.system.contains("in French."));
    assert!(call.system.contains("Contract JSON schema:"));
    assert!(
        call.instruction
            .contains("confidence 100.0 %; verdict reliable")
    );
    assert!(call.instruction.contains("Documents 0 to 1"));
    assert_eq!(call.documents.len(), 5);
    assert!(call.documents[0].1.contains("signal: product_detected"));
    assert!(call.documents[1].1.contains("signal: product_detected"));
    assert_eq!(call.documents[2].0, "lead");
    assert!(call.documents[2].1.starts_with("lead: acme.com"));
    assert_eq!(call.documents[3].0, "qualification:one");
    assert!(call.documents[3].1.contains("agent score: 100 %"));
    assert!(
        call.documents[3]
            .1
            .contains("answer: Forte (forte), confidence 90 %")
    );
    assert_eq!(call.documents[4].0, "qualification:two");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_aggregator_cannot_change_the_computed_confidence_or_verdict(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.script("model-class", vec![Reply::Worst]);
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let mut hostile = summary_with(GOOD_QUOTE);
    hostile["confidence"] = json!(99.0);
    hostile["verdict"] = json!("fiable");
    harness.script(SYNTHESIS, vec![Reply::Raw(hostile)]);

    harness.pass().await;

    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.confidence, Some(0.0));
    assert_eq!(lead.verdict, Some(Verdict::Discarded));
    assert_eq!(harness.assessments(profile, "acme.com").await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unverified_evidence_is_retried_once_then_the_summary_is_kept_flagged(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.script(
        SYNTHESIS,
        vec![
            Reply::Quote("invented quote"),
            Reply::Quote("still invented"),
        ],
    );

    let report = harness.pass().await;

    assert_eq!(report.leads, 1);
    assert_eq!(report.unverified, 1);
    let calls = harness.aggregator_calls();
    assert_eq!(calls.len(), 2);
    assert!(!calls[0].instruction.contains("does not appear verbatim"));
    assert!(calls[1].instruction.contains("does not appear verbatim"));
    let stored = &harness.assessments(profile, "acme.com").await[0];
    assert!(!stored.evidence_verified);
    assert!(stored.evidence.is_empty());
    assert_eq!(stored.summary, "Le domaine expose un Keycloak ancien.");
    assert!(stored.draft.is_some());
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.status, LeadStatus::Qualified);
    assert_eq!(lead.verdict, Some(Verdict::Reliable));
    assert_eq!(harness.queue(&[]).await, ["acme.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_correct_second_answer_is_stored_as_verified(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.script(SYNTHESIS, vec![Reply::Quote("invented quote"), Reply::Good]);

    let report = harness.pass().await;

    assert_eq!(report.unverified, 0);
    assert_eq!(harness.aggregator_calls().len(), 2);
    let stored = &harness.assessments(profile, "acme.com").await[0];
    assert!(stored.evidence_verified);
    assert_eq!(stored.evidence.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_partly_verified_answer_keeps_only_the_checked_quotes(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let mut mixed = summary_with(GOOD_QUOTE);
    mixed["evidence"] = json!([
        {"document_index": 0, "quote": GOOD_QUOTE},
        {"document_index": 0, "quote": "made up"},
        {"document_index": 1, "quote": "lead: acme.com"}
    ]);
    harness.script(
        SYNTHESIS,
        vec![Reply::Raw(mixed.clone()), Reply::Raw(mixed)],
    );

    harness.pass().await;

    let stored = &harness.assessments(profile, "acme.com").await[0];
    assert!(!stored.evidence_verified);
    assert_eq!(stored.evidence.len(), 1);
    assert_eq!(stored.evidence[0].quote, GOOD_QUOTE);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_contract_violation_is_retried_then_the_lead_stays_due(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.script(
        SYNTHESIS,
        vec![Reply::Raw(json!({"summary": "x"})), Reply::Raw(json!({}))],
    );

    let report = harness.pass().await;

    assert_eq!(report.leads, 0);
    assert_eq!(report.failed, 1);
    assert_eq!(report.backlog, 1);
    let calls = harness.aggregator_calls();
    assert_eq!(calls.len(), 2);
    assert!(calls[1].instruction.contains("broke the contract"));
    assert!(harness.assessments(profile, "acme.com").await.is_empty());
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, Some(Verdict::Reliable));

    let waiting = harness.summarise(settings(100, 10)).await;
    assert_eq!(waiting.leads, 0);
    assert_eq!(waiting.failed, 0);
    assert_eq!(waiting.backlog, 1);
    assert_eq!(harness.aggregator_calls().len(), 2);

    harness.set_time(60);
    let again = harness.summarise(settings(100, 10)).await;
    assert_eq!(again.leads, 1);
    assert_eq!(again.backlog, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_lead_gets_a_draft_whatever_its_verdict(pool: PgPool) {
    let harness = Harness::new(pool.clone());
    harness.defaults().await;
    let provider = harness.provider("model-bad", 0).await;
    let profile = harness
        .profile(
            vec![agent("good", 1, None), agent("bad", 9, Some(provider))],
            None,
        )
        .await;
    harness.script("model-bad", vec![Reply::Worst, Reply::Worst]);
    harness.observe("low.com", SourceName::HttpProbe, 10).await;
    harness.observe("low2.com", SourceName::HttpProbe, 11).await;
    harness.pass().await;
    assert_eq!(
        harness.lead(profile, "low.com").await.verdict,
        Some(Verdict::Discarded)
    );

    let queue = harness.queue(&[]).await;
    assert_eq!(queue.len(), 2);
    assert_eq!(harness.queue(&[VerdictFilter::Discarded]).await.len(), 2);
    assert!(harness.queue(&[VerdictFilter::Reliable]).await.is_empty());
    for host in ["low.com", "low2.com"] {
        let stored = &harness.assessments(profile, host).await[0];
        let review = stored.draft_review.as_ref().unwrap();
        assert_eq!(review.status, DraftStatus::Pending);
        assert!(stored.draft.is_some());
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn nothing_is_summarised_again_until_the_results_change(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.pass().await;
    assert_eq!(harness.aggregator_calls().len(), 1);

    assert_eq!(harness.pass().await.leads, 0);
    harness.observe("acme.com", SourceName::HttpProbe, 30).await;
    assert_eq!(harness.pass().await.leads, 0);
    assert_eq!(harness.aggregator_calls().len(), 1);

    harness.observe("acme.com", SourceName::DnsCname, 40).await;
    let new_source = harness.pass().await;
    assert_eq!(new_source.leads, 1);
    assert_eq!(harness.aggregator_calls().len(), 2);
    assert_eq!(harness.assessments(profile, "acme.com").await.len(), 2);

    harness.revise(profile, vec![agent("solo", 7, None)]).await;
    let revised = harness.pass().await;
    assert_eq!(revised.leads, 1);
    assert_eq!(harness.assessments(profile, "acme.com").await.len(), 3);
    assert_eq!(harness.queue(&[]).await, ["acme.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn decided_leads_are_left_alone(pool: PgPool) {
    let harness = Harness::new(pool.clone());
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.evaluate().await;
    sqlx::query("UPDATE leads SET status = 'validated'")
        .execute(&pool)
        .await
        .unwrap();

    let report = harness.summarise(settings(100, 10)).await;

    assert_eq!(report.leads, 0);
    assert_eq!(report.backlog, 0);
    assert!(harness.aggregator_calls().is_empty());
    assert!(harness.assessments(profile, "acme.com").await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_a_provider_nothing_is_stored_and_the_lead_stays_due(pool: PgPool) {
    let harness = Harness::new(pool);
    let classifier = harness.provider(CLASSIFIER, 0).await;
    harness
        .usecase
        .assign_llm_tier(Tier::Triage, classifier)
        .await
        .unwrap();
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    let report = harness.pass().await;

    assert_eq!(report.leads, 0);
    assert_eq!(report.failed, 1);
    assert_eq!(report.backlog, 1);
    assert!(harness.assessments(profile, "acme.com").await.is_empty());
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, Some(Verdict::Reliable));
    assert_eq!(lead.status, LeadStatus::New);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_provider_failure_leaves_the_lead_due_without_blocking_the_next(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("a.com", SourceName::HttpProbe, 10).await;
    harness.observe("b.com", SourceName::HttpProbe, 11).await;
    harness.script(SYNTHESIS, vec![Reply::Fail(LlmError::Refused)]);

    let report = harness.pass().await;

    assert_eq!(report.leads, 1);
    assert_eq!(report.failed, 1);
    assert_eq!(report.backlog, 1);
    assert!(harness.assessments(profile, "a.com").await.is_empty());
    assert_eq!(harness.assessments(profile, "b.com").await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_exhausted_budget_stops_the_batch_and_keeps_leads_due(pool: PgPool) {
    let harness = Harness::new(pool);
    let classifier = harness.provider(CLASSIFIER, 0).await;
    let pricey = harness.provider(SYNTHESIS, 1_000_000).await;
    harness
        .usecase
        .assign_llm_tier(Tier::Triage, classifier)
        .await
        .unwrap();
    harness
        .usecase
        .assign_llm_tier(Tier::Qualification, pricey)
        .await
        .unwrap();
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("a.com", SourceName::HttpProbe, 10).await;
    harness.observe("b.com", SourceName::HttpProbe, 11).await;
    harness.evaluate().await;

    let report = harness.summarise(settings(1, 10)).await;

    assert!(report.budget_exhausted);
    assert_eq!(report.leads, 1);
    assert_eq!(report.backlog, 1);
    assert_eq!(harness.aggregator_calls().len(), 1);
    assert!(harness.assessments(profile, "b.com").await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_batch_is_bounded_oldest_first(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    for index in 0..4 {
        harness
            .observe(
                &format!("lead-{index}.com"),
                SourceName::HttpProbe,
                10 + i64::from(index),
            )
            .await;
    }
    harness.evaluate().await;

    let first = harness.summarise(settings(100, 3)).await;
    assert_eq!(first.leads, 3);
    assert_eq!(first.backlog, 1);
    let second = harness.summarise(settings(100, 3)).await;
    assert_eq!(second.leads, 1);
    assert_eq!(second.backlog, 0);
    for index in 0..4 {
        assert_eq!(
            harness
                .assessments(profile, &format!("lead-{index}.com"))
                .await
                .len(),
            1
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_accepted_summary_is_cached_across_a_reset(pool: PgPool) {
    let harness = Harness::new(pool.clone());
    harness.defaults().await;
    harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.pass().await;
    assert_eq!(harness.aggregator_calls().len(), 1);

    sqlx::query("UPDATE lead_qualification_state SET summarised_at = NULL")
        .execute(&pool)
        .await
        .unwrap();
    let report = harness.summarise(settings(100, 10)).await;

    assert_eq!(report.leads, 1);
    assert_eq!(harness.aggregator_calls().len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn suppressed_domains_are_never_summarised(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.evaluate().await;
    harness
        .usecase
        .add_suppression(
            lodestone_core::SuppressionTarget::Domain(domain("acme.com")),
            None,
            None,
        )
        .await
        .unwrap();

    let report = harness.summarise(settings(100, 10)).await;

    assert_eq!(report.leads, 0);
    assert_eq!(report.backlog, 0);
    assert!(harness.aggregator_calls().is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn failures_back_off_then_the_summary_is_marked_failed(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.script(SYNTHESIS, vec![Reply::Fail(LlmError::Refused); 5]);
    harness.evaluate().await;

    for (attempt, (at, wait)) in [
        (0, 60),
        (60, 300),
        (360, 1_800),
        (2_160, 7_200),
        (9_360, 21_600),
    ]
    .into_iter()
    .enumerate()
    {
        harness.set_time(at);
        let calls = harness.aggregator_calls().len();
        let report = harness.summarise(settings(100, 10)).await;
        assert_eq!(report.failed, 1, "attempt {attempt}");
        assert_eq!(harness.aggregator_calls().len(), calls + 1);

        harness.set_time(at + wait - 1);
        let early = harness.summarise(settings(100, 10)).await;
        assert_eq!(early.failed, 0, "attempt {attempt}");
        assert_eq!(early.backlog, 1);
        assert_eq!(harness.aggregator_calls().len(), calls + 1);

        let state = harness.summary_state(profile, "acme.com").await.unwrap();
        assert_eq!(state.attempts, u32::try_from(attempt + 1).unwrap());
        assert_eq!(state.is_failed(), attempt == 4);
        assert_eq!(state.error.as_deref(), Some("échec de l'appel au modèle"));
        assert_eq!(
            state.next_attempt_at,
            Some(at_time(1_000 + at) + summary_retry_delay(state.attempts))
        );
    }
    assert_eq!(summary_retry_delay(9), chrono::Duration::hours(6));
}

fn at_time(seconds: i64) -> DateTime<Utc> {
    at(seconds)
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_summary_keeps_retrying_every_six_hours_until_it_succeeds(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.script(SYNTHESIS, vec![Reply::Fail(LlmError::Refused); 6]);
    harness.evaluate().await;
    let mut now = 0;
    for wait in [60, 300, 1_800, 7_200, 21_600] {
        harness.set_time(now);
        harness.summarise(settings(100, 10)).await;
        now += wait;
    }
    assert!(
        harness
            .summary_state(profile, "acme.com")
            .await
            .unwrap()
            .is_failed()
    );

    harness.set_time(now);
    let sixth = harness.summarise(settings(100, 10)).await;
    assert_eq!(sixth.failed, 1);
    assert_eq!(harness.aggregator_calls().len(), 6);
    harness.set_time(now + 21_599);
    assert_eq!(harness.summarise(settings(100, 10)).await.failed, 0);

    harness.set_time(now + 21_600);
    let success = harness.summarise(settings(100, 10)).await;
    assert_eq!(success.leads, 1);
    assert_eq!(
        harness
            .summary_state(profile, "acme.com")
            .await
            .unwrap()
            .attempts,
        0
    );
    assert_eq!(harness.assessments(profile, "acme.com").await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn retrying_resets_the_counter_and_makes_the_lead_due_at_once(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.script(SYNTHESIS, vec![Reply::Fail(LlmError::Refused)]);
    harness.pass().await;
    assert_eq!(
        harness
            .summary_state(profile, "acme.com")
            .await
            .unwrap()
            .attempts,
        1
    );
    let lead = harness.lead(profile, "acme.com").await;

    harness.usecase.retry_lead_summary(lead.id).await.unwrap();

    assert_eq!(
        harness
            .summary_state(profile, "acme.com")
            .await
            .unwrap()
            .attempts,
        0
    );
    let report = harness.summarise(settings(100, 10)).await;
    assert_eq!(report.leads, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_agent_pass_resets_the_failure_counter(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.script(SYNTHESIS, vec![Reply::Fail(LlmError::Refused)]);
    harness.pass().await;
    assert_eq!(
        harness
            .summary_state(profile, "acme.com")
            .await
            .unwrap()
            .attempts,
        1
    );

    harness.observe("acme.com", SourceName::DnsCname, 20).await;
    harness.evaluate().await;

    assert_eq!(
        harness
            .summary_state(profile, "acme.com")
            .await
            .unwrap()
            .attempts,
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_missing_provider_is_a_failure_with_its_own_reason(pool: PgPool) {
    let harness = Harness::new(pool);
    let classifier = harness.provider(CLASSIFIER, 0).await;
    harness
        .usecase
        .assign_llm_tier(Tier::Triage, classifier)
        .await
        .unwrap();
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    let state = harness.summary_state(profile, "acme.com").await.unwrap();
    assert_eq!(state.attempts, 1);
    assert_eq!(state.error.as_deref(), Some("fournisseur indisponible"));
}

async fn edit_subject(harness: &Harness, assessment: &Assessment, subject: &str) {
    harness
        .usecase
        .edit_draft(
            assessment.id,
            DraftEdit::new(Some(subject.to_owned()), None).unwrap(),
        )
        .await
        .unwrap();
}

fn rewritten(subject: &str) -> Reply {
    let mut value = summary_with(GOOD_QUOTE);
    value["summary"] = json!("Bilan réécrit.");
    value["draft_message"]["subject"] = json!(subject);
    value["draft_message"]["body"] = json!("Nouveau corps");
    Reply::Raw(value)
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unedited_pending_draft_is_replaced_by_the_new_one(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.pass().await;
    harness.script(SYNTHESIS, vec![rewritten("Objet réécrit")]);

    harness.observe("acme.com", SourceName::DnsCname, 20).await;
    harness.set_time(100);
    harness.pass().await;

    let latest = &harness.assessments(profile, "acme.com").await[0];
    assert_eq!(latest.summary, "Bilan réécrit.");
    assert_eq!(latest.draft.as_ref().unwrap().subject, "Objet réécrit");
    let review = latest.draft_review.as_ref().unwrap();
    assert_eq!(review.suggestion, None);
    assert_eq!(review.outdated_at, None);
    assert_eq!(harness.queue(&[]).await, ["acme.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_edited_pending_draft_is_kept_with_the_new_one_as_a_suggestion(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.pass().await;
    let first = harness.assessments(profile, "acme.com").await.remove(0);
    edit_subject(&harness, &first, "Mon objet").await;
    harness.script(SYNTHESIS, vec![rewritten("Objet suggéré")]);

    harness.observe("acme.com", SourceName::DnsCname, 20).await;
    harness.set_time(100);
    harness.pass().await;

    let assessments = harness.assessments(profile, "acme.com").await;
    assert_eq!(assessments.len(), 2);
    let latest = &assessments[0];
    assert_eq!(latest.summary, "Bilan réécrit.");
    let draft = latest.draft.as_ref().unwrap();
    assert_eq!(draft.subject, "Mon objet");
    assert_eq!(draft.body, "Bonjour, un échange de 20 minutes ?");
    let review = latest.draft_review.as_ref().unwrap();
    assert_eq!(review.status, DraftStatus::Pending);
    assert!(review.edited_at.is_some());
    assert_eq!(review.original.as_ref().unwrap().subject, "Votre Keycloak");
    let suggestion = review.suggestion.as_ref().unwrap();
    assert_eq!(suggestion.subject, "Objet suggéré");
    assert_eq!(suggestion.body, "Nouveau corps");
    assert_eq!(review.outdated_at, Some(at(1_100)));
    assert_eq!(harness.queue(&[]).await, ["acme.com"]);
    harness
        .usecase
        .approve_draft(latest.id, harness.user().await)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_decided_draft_stays_untouched_when_the_lead_is_summarised_again(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile(vec![agent("solo", 5, None)], None).await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.pass().await;
    let first = harness.assessments(profile, "acme.com").await.remove(0);
    let user = harness.user().await;
    harness
        .usecase
        .reject_draft(first.id, user, Some("hors sujet".to_owned()))
        .await
        .unwrap();
    let lead = harness.lead(profile, "acme.com").await;
    harness
        .usecase
        .change_lead_status(lead.id, LeadStatus::New, user)
        .await
        .unwrap();
    harness.script(SYNTHESIS, vec![rewritten("Objet suggéré")]);

    harness.observe("acme.com", SourceName::DnsCname, 20).await;
    harness.set_time(100);
    harness.pass().await;

    let latest = &harness.assessments(profile, "acme.com").await[0];
    assert_eq!(latest.summary, "Bilan réécrit.");
    assert_eq!(latest.draft.as_ref().unwrap().subject, "Votre Keycloak");
    let review = latest.draft_review.as_ref().unwrap();
    assert_eq!(review.status, DraftStatus::Rejected);
    assert_eq!(review.rejection_reason.as_deref(), Some("hors sujet"));
    assert_eq!(review.reviewed_by, Some(user));
    assert_eq!(review.suggestion, None);
    assert!(harness.queue(&[]).await.is_empty());
}
