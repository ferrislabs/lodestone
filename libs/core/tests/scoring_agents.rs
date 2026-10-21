mod support;

use std::{
    collections::{HashMap, VecDeque},
    num::{NonZeroU32, NonZeroUsize},
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, TimeDelta, Utc};
use common::{EncryptionKey, Secret};
use lodestone_core::{
    ActiveProfile, AgentKind, AgentRegistry, CancelReason, Clock, CreateProfileCommand,
    EventStatus, GenerationParams, Lead, LeadDetail, LeadRepository, LeadStatus, LlmError,
    LlmProvider, LlmRequest, LlmResponse, LodestoneUseCase, MaxConcurrency, Observation, ProfileId,
    ProfileName, Provenance, ProviderDefinition, ProviderId, ProviderKind, ResultStatus,
    ReviseProfileCommand, SignalKind, SourceName, Stage, StageReason, StageStatus, Tier,
    TimelineEvent, TimelineStep, TokenPricing, TokenUsage, UnitInterval, UserId, ValidatedProfile,
    Verdict, VerdictReason, VerdictSource, WorkKind,
    evaluation::{EvaluationSettings, ProviderBuilder},
    infrastructure::{
        crypto::AesGcmCipher, lead::postgres::PgLeadRepository, llm_cache::postgres::PgLlmCache,
        postgres::with_tx,
    },
    llm::{CreateLlmProvider, KeyChange, UpdateLlmProvider},
    timeline_kind,
};
use serde_json::{Value, json};
use sqlx::{PgPool, query};
use support::{at, domain};
use tokio::time::sleep;
use uuid::Uuid;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const CLASSIFIER: &str = "model-class";
const SYNTHESIS: &str = "model-synth";
const GOOD_QUOTE: &str = "Keycloak-http_probe (sonde HTTP, version 21.1)";

#[derive(Clone)]
struct TestClock(Arc<AtomicI64>);

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        at(1_000 + self.0.load(Ordering::SeqCst))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Qualification,
    Scoring,
    Aggregator,
}

#[derive(Debug, Clone)]
enum Reply {
    Good,
    Scored {
        score: u8,
        probabilities: (f64, f64, f64),
    },
    Raw(Value),
    Fail(LlmError),
}

#[derive(Debug, Clone)]
struct Seen {
    kind: Kind,
    model: String,
    instruction: String,
    documents: Vec<(String, String)>,
    temperature: Option<f64>,
    schema: Value,
}

enum Change {
    Publish(Box<ActiveProfile>),
    Delete(ProfileId),
}

struct Shared {
    replies: Mutex<HashMap<String, VecDeque<Reply>>>,
    seen: Mutex<Vec<Seen>>,
    on_call: Mutex<Option<(Kind, LodestoneUseCase, Change)>>,
    pace: Mutex<Duration>,
    running: Mutex<(u32, u32)>,
    concurrency: Mutex<NonZeroUsize>,
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

fn qualification_answer(schema: &Value, quote: &str) -> Value {
    let mut answers = serde_json::Map::new();
    for (id, question) in schema["properties"]["answers"]["properties"]
        .as_object()
        .unwrap()
    {
        let all = question["properties"]["choice"]["enum"].as_array().unwrap();
        let chosen = all[0].clone();
        let probabilities: serde_json::Map<String, Value> = all
            .iter()
            .map(|value| {
                (
                    value.as_str().unwrap().to_owned(),
                    json!(if *value == chosen { 1.0 } else { 0.0 }),
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

fn scoring_answer(schema: &Value, quote: &str, score: u8, probabilities: (f64, f64, f64)) -> Value {
    let mut answer = support::scoring_answer(schema, quote, false);
    answer["score"] = json!(score);
    answer["verdict"] = json!({
        "choice": "a_verifier",
        "confidence": probabilities.1,
        "probabilities": {
            "fiable": probabilities.0,
            "a_verifier": probabilities.1,
            "ecarte": probabilities.2
        }
    });
    answer
}

fn summary() -> Value {
    json!({
        "summary": "Le domaine expose un Keycloak ancien.",
        "verdict_narrative": "Les agents jugent le lead pertinent.",
        "approach_angle": "Proposer une migration accompagnée.",
        "evidence": [{"document_index": 0, "quote": GOOD_QUOTE}],
        "draft_message": {"subject": "Votre Keycloak", "body": "Bonjour, un échange de 20 minutes ?"}
    })
}

impl LlmProvider for Scripted {
    async fn complete(&self, request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        let properties = &request.schema["properties"];
        let kind = if properties.get("criteria").is_some() {
            Kind::Scoring
        } else if properties.get("answers").is_some() {
            Kind::Qualification
        } else {
            Kind::Aggregator
        };
        {
            let mut on_call = self.shared.on_call.lock().unwrap();
            if on_call
                .as_ref()
                .is_some_and(|(wanted, _, _)| *wanted == kind)
                && let Some((_, usecase, change)) = on_call.take()
            {
                match change {
                    Change::Publish(changed) => usecase.agent_registry().publish(&changed),
                    Change::Delete(profile) => usecase.agent_registry().delete(profile),
                }
            }
        }
        self.shared.seen.lock().unwrap().push(Seen {
            kind,
            model: self.model.clone(),
            instruction: request.instruction.to_owned(),
            documents: request
                .documents
                .iter()
                .map(|document| (document.source.to_owned(), document.content.to_owned()))
                .collect(),
            temperature: request.temperature,
            schema: request.schema.clone(),
        });
        let key = match kind {
            Kind::Qualification => self.model.clone(),
            Kind::Scoring => format!("{}#scoring", self.model),
            Kind::Aggregator => format!("{}#aggregator", self.model),
        };
        let reply = self
            .shared
            .replies
            .lock()
            .unwrap()
            .get_mut(&key)
            .and_then(VecDeque::pop_front)
            .unwrap_or(Reply::Good);
        let pace = *self.shared.pace.lock().unwrap();
        {
            let mut running = self.shared.running.lock().unwrap();
            running.0 += 1;
            running.1 = running.1.max(running.0);
        }
        if !pace.is_zero() {
            sleep(pace).await;
        }
        self.shared.running.lock().unwrap().0 -= 1;
        let quote = first_line(request);
        let output = match (reply, kind) {
            (Reply::Fail(error), _) => return Err(error),
            (Reply::Raw(value), _) => value,
            (
                Reply::Scored {
                    score,
                    probabilities,
                },
                Kind::Scoring,
            ) => scoring_answer(request.schema, &quote, score, probabilities),
            (_, Kind::Scoring) => scoring_answer(request.schema, &quote, 85, (0.8, 0.15, 0.05)),
            (_, Kind::Qualification) => qualification_answer(request.schema, &quote),
            (_, Kind::Aggregator) => summary(),
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

fn settings() -> EvaluationSettings {
    EvaluationSettings {
        max_tokens: 512,
        budget_cents: NonZeroU32::new(100).unwrap(),
        batch_size: 10,
        base_delay: Duration::ZERO,
        concurrency: NonZeroUsize::MIN,
    }
}

fn question(id: &str, values: [&str; 3]) -> Value {
    json!({
        "id": id,
        "wording": "Pertinent ?",
        "weight": 5,
        "options": [
            {"value": values[0], "label": "Haut", "score": 1.0},
            {"value": values[1], "label": "Moyen", "score": 0.5},
            {"value": values[2], "label": "Bas", "score": 0.0},
            {"value": "inconnu", "label": "Inconnu", "score": 0.5, "is_unknown": true}
        ]
    })
}

fn qualification_agent(id: &str) -> Value {
    json!({
        "id": id,
        "name": id,
        "description": format!("DESCRIPTION-OF-{id}"),
        "enabled": true,
        "provider_id": null,
        "system_prompt": format!("PROMPT-OF-{id}"),
        "samples": 1,
        "questions": [question("q", ["forte", "moyenne", "faible"])]
    })
}

fn scoring_agent(id: &str, weight: u8, provider: Option<ProviderId>, samples: u8) -> Value {
    json!({
        "id": id,
        "name": id,
        "description": format!("DESCRIPTION-OF-{id}"),
        "enabled": true,
        "provider_id": provider.map(|provider| provider.0.to_string()),
        "weight": weight,
        "system_prompt": format!("SCORING-PROMPT-OF-{id}"),
        "criteria": [question("adequation", ["forte", "moyenne", "faible"])],
        "samples": samples,
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

fn cipher() -> AesGcmCipher {
    let encoded = format!("{}QkI=", "QkJC".repeat(10));
    AesGcmCipher::new(&EncryptionKey::from_base64(&Secret::new(encoded)).unwrap()).unwrap()
}

struct Harness {
    pool: PgPool,
    usecase: LodestoneUseCase,
    cipher: AesGcmCipher,
    shared: Arc<Shared>,
    clock: TestClock,
}

impl Harness {
    async fn new(pool: PgPool) -> Self {
        let harness = Self {
            usecase: LodestoneUseCase::new(pool.clone()),
            pool,
            cipher: cipher(),
            clock: TestClock(Arc::new(AtomicI64::new(0))),
            shared: Arc::new(Shared {
                replies: Mutex::new(HashMap::new()),
                seen: Mutex::new(Vec::new()),
                on_call: Mutex::new(None),
                pace: Mutex::new(Duration::ZERO),
                running: Mutex::new((0, 0)),
                concurrency: Mutex::new(NonZeroUsize::MIN),
            }),
        };
        let classifier = harness.provider(CLASSIFIER).await;
        let synthesis = harness.provider(SYNTHESIS).await;
        harness
            .usecase
            .assign_llm_tier(Tier::Triage, classifier)
            .await
            .unwrap();
        harness
            .usecase
            .assign_llm_tier(Tier::Qualification, synthesis)
            .await
            .unwrap();
        harness
    }

    fn settings(&self) -> EvaluationSettings {
        EvaluationSettings {
            concurrency: *self.shared.concurrency.lock().unwrap(),
            ..settings()
        }
    }

    fn run_at_once(&self, concurrency: usize, pace: Duration) {
        *self.shared.concurrency.lock().unwrap() = NonZeroUsize::new(concurrency).unwrap();
        *self.shared.pace.lock().unwrap() = pace;
        *self.shared.running.lock().unwrap() = (0, 0);
    }

    fn peak(&self) -> u32 {
        self.shared.running.lock().unwrap().1
    }

    async fn limit(&self, id: ProviderId, limit: u8) {
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
    }

    async fn provider(&self, model: &str) -> ProviderId {
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
                        input_cents_per_mtok: 0,
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

    async fn profile(&self, qualification: Vec<Value>, scoring: Vec<Value>) -> ProfileId {
        let id = self
            .usecase
            .create_profile(CreateProfileCommand {
                name: ProfileName::parse("iam").unwrap(),
                definition: ValidatedProfile::parse(definition(qualification, scoring)).unwrap(),
                created_by: None,
            })
            .await
            .unwrap()
            .profile
            .id;
        self.usecase.set_profile_active(id, true).await.unwrap();
        id
    }

    async fn revise(&self, profile: ProfileId, qualification: Vec<Value>, scoring: Vec<Value>) {
        self.usecase
            .revise_profile(ReviseProfileCommand {
                profile_id: profile,
                definition: ValidatedProfile::parse(definition(qualification, scoring)).unwrap(),
                created_by: None,
            })
            .await
            .unwrap();
    }

    fn script(&self, key: &str, replies: Vec<Reply>) {
        self.shared
            .replies
            .lock()
            .unwrap()
            .insert(key.to_owned(), replies.into());
    }

    fn seen(&self) -> Vec<Seen> {
        self.shared.seen.lock().unwrap().clone()
    }

    fn kinds(&self) -> Vec<Kind> {
        self.seen().iter().map(|seen| seen.kind).collect()
    }

    fn scoring_calls(&self) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|seen| seen.kind == Kind::Scoring)
            .collect()
    }

    fn builder(&self) -> FakeBuilder {
        FakeBuilder {
            shared: Arc::clone(&self.shared),
        }
    }

    fn cache(&self) -> PgLlmCache {
        PgLlmCache::new(self.pool.clone())
    }

    fn set_time(&self, seconds: i64) {
        self.clock.0.store(seconds, Ordering::SeqCst);
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
                    provenance: Provenance::parse(&format!(
                        "https://{host}/keycloak-{}",
                        source.as_str()
                    ))
                    .unwrap(),
                    strength: UnitInterval::new(0.9).unwrap(),
                    observed_at: at(seconds),
                    agent: None,
                }],
            )
            .await
            .unwrap();
    }

    async fn qualify(&self) -> u64 {
        self.usecase
            .qualify_due_leads(
                &self.builder(),
                &self.cipher,
                &self.cache(),
                self.settings(),
                &self.clock,
            )
            .await
            .unwrap()
            .leads
    }

    async fn score(&self) -> lodestone_core::scoring_agent::ScoringReport {
        self.usecase
            .score_due_leads(
                &self.builder(),
                &self.cipher,
                &self.cache(),
                self.settings(),
                &self.clock,
            )
            .await
            .unwrap()
    }

    async fn summarise(&self) -> u64 {
        self.usecase
            .summarise_due_leads(
                &self.builder(),
                &self.cipher,
                &self.cache(),
                self.settings(),
                &self.clock,
            )
            .await
            .unwrap()
            .leads
    }

    async fn pass(&self) {
        self.qualify().await;
        self.score().await;
        self.summarise().await;
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

    async fn detail(&self, profile: ProfileId, host: &str) -> LeadDetail {
        let lead = self.lead(profile, host).await;
        self.usecase.lead_detail(lead.id).await.unwrap()
    }

    async fn count_rows(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM scoring_results")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

fn near(left: f64, right: f64) -> bool {
    (left - right).abs() < 1e-6
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_flows_through_qualification_scoring_and_aggregation_in_that_order(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    assert_eq!(
        harness.kinds(),
        [Kind::Qualification, Kind::Scoring, Kind::Aggregator]
    );
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, Some(Verdict::Reliable));
    assert_eq!(lead.llm_score, Some(85));
    assert_eq!(lead.confidence, Some(80.0));
    let detail = harness.detail(profile, "acme.com").await;
    assert_eq!(detail.assessments.len(), 1);
    let scoring = detail.scoring.unwrap();
    assert_eq!(scoring.verdict_source, VerdictSource::Scoring);
    assert_eq!(scoring.reason, None);
    assert!(scoring.indicator_confidence.is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_aggregator_waits_for_the_scoring_of_the_lead(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.qualify().await;
    assert_eq!(harness.summarise().await, 0);
    assert_eq!(harness.kinds(), [Kind::Qualification]);

    harness.score().await;
    assert_eq!(harness.summarise().await, 1);
    assert_eq!(
        harness.kinds(),
        [Kind::Qualification, Kind::Scoring, Kind::Aggregator]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_lead_has_no_verdict_until_it_is_scored(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.qualify().await;

    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, None);
    assert_eq!(lead.confidence, None);
    assert_eq!(lead.llm_score, None);
    let scoring = harness.detail(profile, "acme.com").await.scoring.unwrap();
    assert_eq!(scoring.reason, Some(VerdictReason::AwaitingScoring));
    assert!(scoring.indicator_confidence.is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_lead_is_scored_again_only_when_qualification_or_the_revision_changed(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.pass().await;
    assert_eq!(harness.scoring_calls().len(), 1);

    let idle = harness.score().await;
    assert_eq!((idle.leads, idle.backlog), (0, 0));
    assert_eq!(harness.scoring_calls().len(), 1);

    harness.observe("acme.com", SourceName::DnsCname, 20).await;
    harness.set_time(100);
    harness.qualify().await;
    let due = harness.score().await;
    assert_eq!(due.leads, 1);
    assert_eq!(harness.scoring_calls().len(), 2);

    harness
        .revise(
            profile,
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 7, None, 1)],
        )
        .await;
    harness.set_time(200);
    harness.qualify().await;
    let revised = harness.score().await;
    assert_eq!(revised.leads, 1);
    assert_eq!(harness.scoring_calls().len(), 3);
    assert_eq!(harness.count_rows().await, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn scoring_results_are_upserted_once_per_lead_revision_and_agent(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![
                scoring_agent("one", 5, None, 1),
                scoring_agent("two", 5, None, 1),
            ],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.pass().await;
    assert_eq!(harness.count_rows().await, 2);
    let first: Vec<(String, chrono::DateTime<Utc>)> =
        sqlx::query_as("SELECT agent_id, created_at FROM scoring_results ORDER BY agent_id")
            .fetch_all(&harness.pool)
            .await
            .unwrap();

    harness.observe("acme.com", SourceName::DnsCname, 20).await;
    harness.set_time(500);
    harness.qualify().await;
    harness.score().await;

    assert_eq!(harness.count_rows().await, 2);
    let second: Vec<(String, chrono::DateTime<Utc>)> =
        sqlx::query_as("SELECT agent_id, created_at FROM scoring_results ORDER BY agent_id")
            .fetch_all(&harness.pool)
            .await
            .unwrap();
    assert!(
        first
            .iter()
            .zip(&second)
            .all(|(old, new)| old.0 == new.0 && new.1 > old.1)
    );
    let detail = harness.detail(profile, "acme.com").await;
    assert_eq!(detail.scoring_results.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_lead_score_and_verdict_are_the_weighted_means_of_the_agents(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let heavy = harness.provider("model-heavy").await;
    let light = harness.provider("model-light").await;
    harness.script(
        "model-heavy#scoring",
        vec![Reply::Scored {
            score: 80,
            probabilities: (0.7, 0.2, 0.1),
        }],
    );
    harness.script(
        "model-light#scoring",
        vec![Reply::Scored {
            score: 20,
            probabilities: (0.1, 0.2, 0.7),
        }],
    );
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![
                scoring_agent("heavy", 3, Some(heavy), 1),
                scoring_agent("light", 1, Some(light), 1),
            ],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.llm_score, Some(65));
    assert_eq!(lead.verdict, Some(Verdict::Reliable));
    assert_eq!(lead.confidence, Some(55.0));
    let scoring = harness.detail(profile, "acme.com").await.scoring.unwrap();
    let probabilities = scoring.probabilities.unwrap();
    assert!(near(probabilities.reliable, 0.55));
    assert!(near(probabilities.to_check, 0.2));
    assert!(near(probabilities.discarded, 0.25));
}

#[sqlx::test(migrations = "../../migrations")]
async fn samples_vote_the_criteria_and_average_the_score_and_the_probabilities(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness.script(
        "model-synth#scoring",
        vec![
            Reply::Scored {
                score: 60,
                probabilities: (0.2, 0.7, 0.1),
            },
            Reply::Scored {
                score: 70,
                probabilities: (0.4, 0.5, 0.1),
            },
            Reply::Scored {
                score: 80,
                probabilities: (0.3, 0.6, 0.1),
            },
        ],
    );
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 3)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    let calls = harness.scoring_calls();
    assert_eq!(calls.len(), 3);
    assert!(calls.iter().all(|call| call.temperature == Some(0.7)));
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.llm_score, Some(70));
    assert_eq!(lead.verdict, Some(Verdict::ToCheck));
    assert_eq!(lead.confidence, Some(60.0));
    let detail = harness.detail(profile, "acme.com").await;
    let result = &detail.scoring_results[0];
    assert_eq!(result.provenance.samples, 3);
    assert_eq!(result.raw_samples.as_array().unwrap().len(), 3);
    let answers = result.criteria_answers(&detail.scoring_agents[0]).unwrap();
    assert_eq!(answers.0[0].agreement, Some(1.0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_invalid_answer_is_retried_once_then_the_agent_result_is_invalid(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let broken = json!({"criteria": {}, "score": 120, "verdict": null, "reasons": []});
    harness.script(
        "model-synth#scoring",
        vec![Reply::Raw(broken.clone()), Reply::Raw(broken)],
    );
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    let calls = harness.scoring_calls();
    assert_eq!(calls.len(), 2);
    assert!(!calls[0].instruction.contains("broke the contract"));
    assert!(calls[1].instruction.contains("broke the contract"));
    assert!(calls[1].instruction.contains("score"));
    let detail = harness.detail(profile, "acme.com").await;
    let result = &detail.scoring_results[0];
    assert_eq!(result.status, ResultStatus::Invalid);
    assert!(result.error.as_deref().unwrap().contains("contrat"));
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, None);
    assert_eq!(lead.llm_score, None);
    assert_eq!(
        detail.scoring.unwrap().reason,
        Some(VerdictReason::AwaitingScoring)
    );
    assert_eq!(detail.assessments.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_correct_second_answer_is_kept_after_one_violation(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness.script(
        "model-synth#scoring",
        vec![Reply::Raw(json!({"nothing": true})), Reply::Good],
    );
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    assert_eq!(harness.scoring_calls().len(), 2);
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.llm_score, Some(85));
}

#[sqlx::test(migrations = "../../migrations")]
async fn criteria_without_a_verifiable_quote_are_downgraded_to_unknown(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let mut answer = support::scoring_answer(
        &lodestone_core::scoring_contract_schema(
            &serde_json::from_value(scoring_agent("global", 5, None, 1)).unwrap(),
        ),
        "citation inventée",
        false,
    );
    answer["score"] = json!(40);
    harness.script(
        "model-synth#scoring",
        vec![Reply::Raw(answer.clone()), Reply::Raw(answer)],
    );
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    let calls = harness.scoring_calls();
    assert_eq!(calls.len(), 2);
    assert!(calls[1].instruction.contains("verbatim quote"));
    let detail = harness.detail(profile, "acme.com").await;
    let result = &detail.scoring_results[0];
    assert_eq!(result.status, ResultStatus::Valid);
    assert_eq!(result.criteria["adequation"]["choice"], "inconnu");
    assert_eq!(result.criteria["adequation"]["grounded"], false);
    assert_eq!(result.criteria["adequation"]["original_choice"], "forte");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_scoring_prompt_carries_reports_indicators_and_the_contract(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    let call = &harness.scoring_calls()[0];
    assert_eq!(call.model, SYNTHESIS);
    let sources: Vec<&str> = call
        .documents
        .iter()
        .map(|document| document.0.as_str())
        .collect();
    assert_eq!(sources[0], "lead");
    assert_eq!(sources[1], "https://acme.com/keycloak-http_probe");
    assert_eq!(&sources[2..], ["qualification:fit", "indicators"]);
    assert!(call.documents[0].1.starts_with("lead: acme.com"));
    let report = &call.documents[2].1;
    assert!(report.contains("evaluates: DESCRIPTION-OF-fit"));
    assert!(report.contains("summary: Observations de l'agent sur le lead."));
    assert!(report.contains("answer: Haut (forte), confidence 90 %"));
    assert!(report.contains("quote: \"lead: acme.com\""));
    let indicators = &call.documents[3].1;
    assert!(indicators.contains("agents opinion: "));
    assert!(indicators.contains("evidence strength:"));
    assert!(indicators.contains("indicator confidence (opinion x evidence):"));
    assert_eq!(
        call.schema["required"],
        json!(["criteria", "score", "verdict", "reasons"])
    );
    assert!(call.instruction.contains("Question `adequation`"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_a_scoring_agent_the_verdict_stays_pending(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let profile = harness
        .profile(vec![qualification_agent("fit")], Vec::new())
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    assert_eq!(harness.kinds(), [Kind::Qualification, Kind::Aggregator]);
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, None);
    assert_eq!(lead.llm_score, None);
    let detail = harness.detail(profile, "acme.com").await;
    assert_eq!(
        detail.scoring.unwrap().reason,
        Some(VerdictReason::AwaitingScoring)
    );
    assert!(detail.scoring_results.is_empty());
    assert_eq!(detail.assessments.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_disabled_scoring_agent_is_not_run(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let mut off = scoring_agent("off", 9, None, 1);
    off["enabled"] = json!(false);
    harness
        .profile(
            vec![qualification_agent("fit")],
            vec![off, scoring_agent("on", 1, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    assert_eq!(harness.scoring_calls().len(), 1);
    assert_eq!(harness.count_rows().await, 1);
}

fn excluding_agent() -> Value {
    json!({
        "id": "exclusions",
        "name": "exclusions",
        "description": "Exclut les concurrents",
        "enabled": true,
        "provider_id": null,
        "system_prompt": "PROMPT-OF-exclusions",
        "samples": 1,
        "exclusion_threshold": 0.6,
        "questions": [{
            "id": "client_actuel",
            "wording": "Client actuel ?",
            "weight": 5,
            "options": [
                {"value": "oui", "label": "Client actuel", "score": 0.0, "excludes": true},
                {"value": "non", "label": "Pas client", "score": 1.0},
                {"value": "inconnu", "label": "Inconnu", "score": 0.5, "is_unknown": true}
            ]
        }]
    })
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_qualification_exclusion_forces_discarded_whatever_the_scoring_agents_say(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let profile = harness
        .profile(
            vec![excluding_agent()],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, Some(Verdict::Discarded));
    assert_eq!(lead.confidence, Some(100.0));
    assert_eq!(lead.llm_score, Some(85));
    let scoring = harness.detail(profile, "acme.com").await.scoring.unwrap();
    assert_eq!(scoring.reason, Some(VerdictReason::Excluded));
    assert_eq!(scoring.exclusions.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rescoring_keeps_the_verdict_and_refreshes_the_indicators_without_a_model_call(
    pool: PgPool,
) {
    let harness = Harness::new(pool.clone()).await;
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.pass().await;
    let before = harness.lead(profile, "acme.com").await;
    let calls = harness.seen().len();
    sqlx::query("UPDATE lead_qualification_state SET scoring_version = 0, evidence = NULL, opinion = NULL, indicator_confidence = NULL")
        .execute(&pool)
        .await
        .unwrap();

    harness.set_time(3_600);
    let report = harness.qualify().await;

    assert_eq!(report, 0);
    assert_eq!(harness.seen().len(), calls);
    let after = harness.lead(profile, "acme.com").await;
    assert_eq!(after.verdict, before.verdict);
    assert_eq!(after.confidence, before.confidence);
    assert_eq!(after.llm_score, before.llm_score);
    let scoring = harness.detail(profile, "acme.com").await.scoring.unwrap();
    assert!(scoring.indicator_confidence.is_some());
    assert!(scoring.opinion.is_some());
    assert_eq!(scoring.verdict_source, VerdictSource::Scoring);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rescoring_applies_exclusions_to_a_stored_verdict(pool: PgPool) {
    let harness = Harness::new(pool.clone()).await;
    let profile = harness
        .profile(
            vec![qualification_agent("fit"), excluding_agent()],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.pass().await;
    sqlx::query("UPDATE lead_qualification_state SET scoring_version = 0, exclusions = '[]'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE leads SET verdict = 'fiable', confidence = 80")
        .execute(&pool)
        .await
        .unwrap();
    let calls = harness.seen().len();

    harness.set_time(3_600);
    harness.qualify().await;

    assert_eq!(harness.seen().len(), calls);
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, Some(Verdict::Discarded));
    assert_eq!(lead.confidence, Some(100.0));
    assert_eq!(lead.llm_score, Some(85));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_qualified_before_the_scoring_agents_keeps_its_verdict_until_scored(pool: PgPool) {
    let harness = Harness::new(pool.clone()).await;
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.pass().await;
    sqlx::query("UPDATE lead_qualification_state SET verdict_source = 'indicators', scoring_revision = NULL, scoring_for = NULL, scoring_version = 0")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE leads SET verdict = 'a_verifier', confidence = 42, llm_score = NULL")
        .execute(&pool)
        .await
        .unwrap();

    let before = harness.detail(profile, "acme.com").await;
    assert_eq!(before.lead.verdict, Some(Verdict::ToCheck));
    assert_eq!(
        before
            .scoring
            .as_ref()
            .map(|scoring| scoring.verdict_source),
        Some(VerdictSource::Indicators)
    );
    harness.set_time(3_600);
    harness.qualify().await;
    let rescored = harness.lead(profile, "acme.com").await;
    assert_eq!(rescored.verdict, Some(Verdict::ToCheck));
    assert_eq!(rescored.confidence, Some(42.0));

    harness.score().await;

    let after = harness.detail(profile, "acme.com").await;
    assert_eq!(after.lead.verdict, Some(Verdict::Reliable));
    assert_eq!(after.lead.llm_score, Some(85));
    assert_eq!(
        after.scoring.map(|scoring| scoring.verdict_source),
        Some(VerdictSource::Scoring)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_provider_failure_leaves_the_agent_invalid_without_blocking_the_others(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let failing = harness.provider("model-failing").await;
    harness.script(
        "model-failing#scoring",
        vec![Reply::Fail(LlmError::Refused)],
    );
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![
                scoring_agent("failing", 9, Some(failing), 1),
                scoring_agent("fine", 1, None, 1),
            ],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    let detail = harness.detail(profile, "acme.com").await;
    let status = |id: &str| {
        detail
            .scoring_results
            .iter()
            .find(|result| result.agent_id.as_str() == id)
            .map(|result| result.status)
    };
    assert_eq!(status("failing"), Some(ResultStatus::Invalid));
    assert_eq!(status("fine"), Some(ResultStatus::Valid));
    assert_eq!(detail.lead.llm_score, Some(85));
    assert_eq!(detail.lead.verdict, Some(Verdict::Reliable));
}

fn answer_quoting(quote: &str) -> Value {
    let schema = lodestone_core::scoring_contract_schema(
        &serde_json::from_value(scoring_agent("global", 5, None, 1)).unwrap(),
    );
    let mut answer = support::scoring_answer(&schema, "lead: acme.com", false);
    answer["criteria"]["adequation"]["evidence"] = json!(quote);
    answer
}

#[sqlx::test(migrations = "../../migrations")]
async fn report_metadata_and_indicators_are_not_evidence(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let metadata = answer_quoting("status: valid");
    harness.script(
        "model-synth#scoring",
        vec![Reply::Raw(metadata.clone()), Reply::Raw(metadata)],
    );
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    assert_eq!(harness.scoring_calls().len(), 2);
    let detail = harness.detail(profile, "acme.com").await;
    let result = &detail.scoring_results[0];
    assert_eq!(result.criteria["adequation"]["choice"], "inconnu");
    assert_eq!(result.criteria["adequation"]["grounded"], false);
    assert_eq!(result.criteria["adequation"]["original_choice"], "forte");
}

#[sqlx::test(migrations = "../../migrations")]
async fn indicator_lines_are_not_evidence(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let indicators = answer_quoting("agents opinion");
    harness.script(
        "model-synth#scoring",
        vec![Reply::Raw(indicators.clone()), Reply::Raw(indicators)],
    );
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    let detail = harness.detail(profile, "acme.com").await;
    assert_eq!(
        detail.scoring_results[0].criteria["adequation"]["grounded"],
        false
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_qualification_summary_stays_groundable(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness.script(
        "model-synth#scoring",
        vec![Reply::Raw(answer_quoting(
            "Observations de l'agent sur le lead.",
        ))],
    );
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    assert_eq!(harness.scoring_calls().len(), 1);
    let detail = harness.detail(profile, "acme.com").await;
    let criterion = &detail.scoring_results[0].criteria["adequation"];
    assert_eq!(criterion["choice"], "forte");
    assert_eq!(criterion["grounded"], true);
    assert_eq!(criterion["evidence_source"], "qualification:fit");
}

fn network() -> Reply {
    Reply::Fail(LlmError::Network("connection failed".to_owned()))
}

fn sampled_agent(id: &str, samples: u8) -> Value {
    let mut agent = qualification_agent(id);
    agent["samples"] = json!(samples);
    agent
}

impl Harness {
    async fn qualify_report(&self) -> lodestone_core::evaluation::EvaluationReport {
        self.usecase
            .qualify_due_leads(
                &self.builder(),
                &self.cipher,
                &self.cache(),
                self.settings(),
                &self.clock,
            )
            .await
            .unwrap()
    }

    async fn failures(&self, profile: ProfileId, host: &str) -> Vec<lodestone_core::AgentFailure> {
        self.detail(profile, host).await.agent_failures
    }

    async fn result_rows(&self, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn sample_flow(&self, qualification: Vec<Value>) -> ProfileId {
        let profile = self
            .profile(qualification, vec![scoring_agent("global", 5, None, 1)])
            .await;
        self.observe("acme.com", SourceName::HttpProbe, 10).await;
        profile
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_scoring_call_failure_keeps_the_lead_due_with_a_backoff_then_succeeds(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness.script("model-synth#scoring", vec![network()]);
    let profile = harness.sample_flow(vec![qualification_agent("fit")]).await;
    harness.qualify().await;

    let first = harness.score().await;

    assert_eq!((first.leads, first.deferred), (0, 1));
    assert_eq!(harness.result_rows("scoring_results").await, 0);
    let failures = harness.failures(profile, "acme.com").await;
    assert_eq!(failures.len(), 1);
    let failure = &failures[0];
    assert_eq!(failure.kind, AgentKind::Scoring);
    assert_eq!(failure.agent_id, "global");
    assert_eq!(failure.attempts, 1);
    assert!(!failure.is_failed());
    assert_eq!(failure.next_attempt_at, Some(at(1_060)));
    assert!(failure.error.starts_with("Échec de l'appel au modèle"));
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, None);

    let waiting = harness.score().await;
    assert_eq!(
        (waiting.leads, waiting.deferred, waiting.backlog),
        (0, 0, 0)
    );
    assert_eq!(harness.scoring_calls().len(), 1);

    harness.set_time(61);
    let retried = harness.score().await;

    assert_eq!((retried.leads, retried.deferred), (1, 0));
    assert!(harness.failures(profile, "acme.com").await.is_empty());
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(lead.verdict, Some(Verdict::Reliable));
    assert_eq!(harness.result_rows("scoring_results").await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_backoff_follows_the_schedule_and_the_agents_fail_after_five_attempts(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness.script("model-synth#scoring", vec![network(); 6]);
    let profile = harness.sample_flow(vec![qualification_agent("fit")]).await;
    harness.qualify().await;

    let mut now = 0;
    for (attempt, delay) in [60, 300, 1_800, 7_200, 21_600].into_iter().enumerate() {
        harness.set_time(now);
        harness.score().await;
        let failure = harness.failures(profile, "acme.com").await.remove(0);
        assert_eq!(failure.attempts as usize, attempt + 1);
        assert_eq!(failure.next_attempt_at, Some(at(1_000 + now + delay)));
        assert_eq!(failure.is_failed(), attempt == 4);
        now += delay;
    }

    harness.set_time(now);
    let waiting = harness.score().await;
    assert_eq!(waiting.deferred, 1);
    let failure = harness.failures(profile, "acme.com").await.remove(0);
    assert_eq!(failure.attempts, 6);
    assert_eq!(failure.next_attempt_at, Some(at(1_000 + now + 21_600)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn retrying_resets_the_failed_agents_only_and_makes_them_due_now(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness.script("model-synth#scoring", vec![network(); 5]);
    let profile = harness.sample_flow(vec![qualification_agent("fit")]).await;
    harness.qualify().await;
    let lead = harness.lead(profile, "acme.com").await;
    let mut now = 0;
    for delay in [60, 300, 1_800, 7_200] {
        harness.set_time(now);
        harness.score().await;
        now += delay;
    }

    harness.usecase.retry_lead_agents(lead.id).await.unwrap();
    let untouched = harness.failures(profile, "acme.com").await.remove(0);
    assert_eq!(untouched.attempts, 4);

    harness.set_time(now);
    harness.score().await;
    assert!(harness.failures(profile, "acme.com").await[0].is_failed());

    harness.usecase.retry_lead_agents(lead.id).await.unwrap();
    let reset = harness.failures(profile, "acme.com").await.remove(0);
    assert_eq!((reset.attempts, reset.next_attempt_at), (0, None));

    let report = harness.score().await;
    assert_eq!((report.leads, report.deferred), (1, 0));
    assert!(harness.failures(profile, "acme.com").await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_contract_violation_after_the_retry_stays_a_final_invalid_result(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness.script(
        "model-synth#scoring",
        vec![Reply::Raw(json!({})), Reply::Raw(json!({}))],
    );
    let profile = harness.sample_flow(vec![qualification_agent("fit")]).await;
    harness.qualify().await;

    let report = harness.score().await;

    assert_eq!((report.leads, report.invalid, report.deferred), (1, 1, 0));
    assert!(harness.failures(profile, "acme.com").await.is_empty());
    let detail = harness.detail(profile, "acme.com").await;
    assert_eq!(detail.scoring_results.len(), 1);
    assert_eq!(detail.scoring_results[0].status, ResultStatus::Invalid);
    assert!(
        detail.scoring_results[0]
            .error
            .as_deref()
            .unwrap()
            .starts_with("Réponse non conforme")
    );
    let idle = harness.score().await;
    assert_eq!((idle.leads, idle.backlog), (0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_status_error_is_a_call_failure_but_a_refusal_is_not(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness.script(
        "model-synth#scoring",
        vec![Reply::Fail(LlmError::Status {
            status: 400,
            message: Some("reasoning_effort is not enabled for this model".to_owned()),
        })],
    );
    let profile = harness.sample_flow(vec![qualification_agent("fit")]).await;
    harness.qualify().await;

    harness.score().await;

    let failure = harness.failures(profile, "acme.com").await.remove(0);
    assert!(
        failure
            .error
            .contains("reasoning_effort is not enabled for this model")
    );
    assert_eq!(harness.result_rows("scoring_results").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn qualification_with_every_sample_call_failed_stays_due_with_a_backoff(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness.script("model-class", vec![network(); 3]);
    let profile = harness.sample_flow(vec![sampled_agent("fit", 3)]).await;

    let report = harness.qualify_report().await;

    assert_eq!((report.leads, report.deferred), (0, 1));
    assert_eq!(harness.result_rows("qualification_results").await, 0);
    let failures = harness.failures(profile, "acme.com").await;
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, AgentKind::Qualification);
    assert_eq!(failures[0].agent_id, "fit");
    assert_eq!(failures[0].next_attempt_at, Some(at(1_060)));
    assert_eq!(harness.lead(profile, "acme.com").await.qualified_at, None);

    assert_eq!(harness.qualify().await, 0);
    assert_eq!(harness.kinds().len(), 3);

    harness.set_time(61);
    assert_eq!(harness.qualify().await, 1);
    assert!(harness.failures(profile, "acme.com").await.is_empty());
    let detail = harness.detail(profile, "acme.com").await;
    assert_eq!(detail.qualification_results[0].status, ResultStatus::Valid);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_partial_sample_call_failure_is_ignored(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness.script("model-class", vec![Reply::Good, network(), Reply::Good]);
    let profile = harness.sample_flow(vec![sampled_agent("fit", 3)]).await;

    assert_eq!(harness.qualify().await, 1);

    assert!(harness.failures(profile, "acme.com").await.is_empty());
    let detail = harness.detail(profile, "acme.com").await;
    let result = &detail.qualification_results[0];
    assert_eq!(result.status, ResultStatus::Valid);
    assert_eq!(result.provenance.samples, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_provider_update_resets_the_backoff_of_the_leads_it_failed(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness.script("model-synth#scoring", vec![network()]);
    harness.script("model-synth#aggregator", vec![network()]);
    let profile = harness.sample_flow(vec![qualification_agent("fit")]).await;
    harness.qualify().await;
    harness.score().await;
    let provider = harness.failures(profile, "acme.com").await[0]
        .provider_id
        .unwrap();
    harness.set_time(61);
    harness.score().await;
    harness.summarise().await;
    let detail = harness.detail(profile, "acme.com").await;
    assert_eq!(detail.summary_state.unwrap().attempts, 1);
    harness.script("model-synth#scoring", vec![network()]);
    harness.observe("acme.com", SourceName::DnsCname, 20).await;
    harness.set_time(200);
    harness.qualify().await;
    harness.score().await;
    let before = harness.failures(profile, "acme.com").await.remove(0);
    assert_eq!(before.attempts, 1);
    assert!(before.next_attempt_at.is_some());

    harness
        .usecase
        .update_llm_provider(
            UpdateLlmProvider {
                id: provider,
                name: "synth-fixed".to_owned(),
                kind: ProviderKind::OpenAiCompatible,
                base_url: "https://llm.example/v1".parse().unwrap(),
                model: SYNTHESIS.to_owned(),
                timeout: Duration::from_secs(30),
                max_retries: 0,
                max_concurrency: MaxConcurrency::new(4).unwrap(),
                pricing: TokenPricing {
                    input_cents_per_mtok: 0,
                    output_cents_per_mtok: 0,
                },
                api_key: KeyChange::Keep,
                reasoning_effort: None,
                generation: GenerationParams::default(),
            },
            &harness.cipher,
        )
        .await
        .unwrap();

    let after = harness.failures(profile, "acme.com").await.remove(0);
    assert_eq!((after.attempts, after.next_attempt_at), (0, None));
    let detail = harness.detail(profile, "acme.com").await;
    assert_eq!(detail.summary_state.unwrap().attempts, 0);
    let report = harness.score().await;
    assert_eq!((report.leads, report.deferred), (1, 0));
}

const CALL_FAILURE: &str = "Échec de l''appel au modèle : llm provider answered with status 400";

async fn rerun_migration(pool: &PgPool) {
    sqlx::raw_sql(include_str!(
        "../../../migrations/20261007000005_add_agent_call_retries.down.sql"
    ))
    .execute(pool)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!(
        "../../../migrations/20261007000005_add_agent_call_retries.up.sql"
    ))
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_migration_makes_stored_scoring_call_failures_due_again(pool: PgPool) {
    let harness = Harness::new(pool.clone()).await;
    let profile = harness.sample_flow(vec![qualification_agent("fit")]).await;
    harness.pass().await;
    sqlx::query(&format!(
        "UPDATE scoring_results SET status = 'invalid', score = NULL, verdict = NULL, error = '{CALL_FAILURE}'"
    ))
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(harness.score().await.leads, 0);

    rerun_migration(&pool).await;

    let report = harness.score().await;
    assert_eq!((report.leads, report.valid), (1, 1));
    let detail = harness.detail(profile, "acme.com").await;
    assert_eq!(detail.scoring_results[0].status, ResultStatus::Valid);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_migration_leaves_contract_failures_and_valid_results_alone(pool: PgPool) {
    let harness = Harness::new(pool.clone()).await;
    harness.sample_flow(vec![qualification_agent("fit")]).await;
    harness.pass().await;
    sqlx::query(
        "UPDATE scoring_results SET status = 'invalid', score = NULL, verdict = NULL, error = 'Réponse non conforme au contrat : x'",
    )
    .execute(&pool)
    .await
    .unwrap();

    rerun_migration(&pool).await;

    assert_eq!(harness.score().await.leads, 0);
    assert_eq!(harness.qualify().await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_migration_makes_stored_qualification_call_failures_due_again(pool: PgPool) {
    let harness = Harness::new(pool.clone()).await;
    let profile = harness.sample_flow(vec![qualification_agent("fit")]).await;
    harness.pass().await;
    sqlx::query(&format!(
        "UPDATE qualification_results SET status = 'invalid', agent_score = NULL, error = '{CALL_FAILURE}'"
    ))
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(harness.qualify().await, 0);

    rerun_migration(&pool).await;

    assert_eq!(harness.qualify().await, 1);
    let detail = harness.detail(profile, "acme.com").await;
    assert_eq!(detail.qualification_results[0].status, ResultStatus::Valid);
}

impl Harness {
    fn on_call(&self, kind: Kind, change: Change) {
        *self.shared.on_call.lock().unwrap() = Some((kind, self.usecase.clone(), change));
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivating_the_profile_during_scoring_stores_no_scoring(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.qualify().await;
    let mut deactivated = harness.active().await;
    deactivated.profile.active = false;
    deactivated.profile.updated_at += TimeDelta::seconds(1);
    harness.on_call(Kind::Scoring, Change::Publish(Box::new(deactivated)));

    let report = harness.score().await;

    assert_eq!(report.stopped, 1);
    assert_eq!(report.leads, 0);
    assert_eq!(harness.count_rows().await, 0);
    let detail = harness.detail(profile, "acme.com").await;
    assert_eq!(detail.lead.llm_score, None);
    assert_eq!(detail.agent_stops.len(), 1);
    assert_eq!(detail.agent_stops[0].target.kind, WorkKind::Scoring);
    assert_eq!(
        detail.agent_stops[0].reason,
        CancelReason::ProfileDeactivated
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_the_profile_during_aggregation_stores_no_summary(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.qualify().await;
    harness.score().await;
    harness.on_call(Kind::Aggregator, Change::Delete(profile));

    let report = harness
        .usecase
        .summarise_due_leads(
            &harness.builder(),
            &harness.cipher,
            &harness.cache(),
            settings(),
            &harness.clock,
        )
        .await
        .unwrap();

    assert_eq!(report.stopped, 1);
    assert_eq!(report.leads, 0);
    assert_eq!(
        harness.kinds(),
        [Kind::Qualification, Kind::Scoring, Kind::Aggregator]
    );
    let detail = harness.detail(profile, "acme.com").await;
    assert!(detail.assessments.is_empty());
    assert_eq!(detail.agent_stops.len(), 1);
    assert_eq!(detail.agent_stops[0].target.kind, WorkKind::Aggregator);
    assert_eq!(detail.agent_stops[0].reason, CancelReason::ProfileDeleted);
}

#[sqlx::test(migrations = "../../migrations")]
async fn scoring_and_aggregation_run_several_leads_at_once_within_the_provider_limit(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let scorer = harness.provider("model-scorer").await;
    harness.limit(scorer, 3).await;
    let synthesis = harness
        .usecase
        .list_llm_providers()
        .await
        .unwrap()
        .into_iter()
        .find(|provider| provider.model == SYNTHESIS)
        .unwrap()
        .id;
    harness.limit(synthesis, 2).await;
    let profile = harness
        .profile(
            vec![qualification_agent("pertinence")],
            vec![
                scoring_agent("fit", 1, Some(scorer), 2),
                scoring_agent("timing", 1, Some(scorer), 2),
            ],
        )
        .await;
    let hosts = ["acme.com", "globex.com", "initech.com"];
    for host in hosts {
        harness.observe(host, SourceName::HttpProbe, 10).await;
    }
    assert_eq!(harness.qualify().await, 3);

    harness.run_at_once(4, Duration::from_millis(40));
    let scoring = harness.score().await;

    assert_eq!((scoring.leads, scoring.valid, scoring.invalid), (3, 6, 0));
    let peak = harness.peak();
    assert!(peak > 1, "scoring calls never overlapped");
    assert!(peak <= 3, "the scoring provider limit was exceeded: {peak}");
    assert_eq!(harness.scoring_calls().len(), 12);

    harness.run_at_once(4, Duration::from_millis(40));
    assert_eq!(harness.summarise().await, 3);
    assert_eq!(harness.peak(), 2);
    for host in hosts {
        assert_eq!(harness.detail(profile, host).await.assessments.len(), 1);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_complete_flow_records_scoring_synthesis_draft_and_decision(pool: PgPool) {
    let harness = Harness::new(pool.clone()).await;
    let profile = harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;

    harness.pass().await;

    let lead = harness.lead(profile, "acme.com").await;
    let timeline = harness.usecase.lead_timeline(lead.id).await.unwrap();
    let events: Vec<&TimelineEvent> = timeline
        .rounds
        .iter()
        .flat_map(|round| round.events.iter())
        .collect();
    let has = |step: TimelineStep, kind: &str, status: EventStatus| {
        events
            .iter()
            .any(|event| event.step == step && event.kind == kind && event.status == status)
    };
    assert!(has(
        TimelineStep::Scoring,
        timeline_kind::AGENT,
        EventStatus::Succeeded
    ));
    assert!(has(
        TimelineStep::Combination,
        timeline_kind::COMBINED,
        EventStatus::Succeeded
    ));
    assert!(has(
        TimelineStep::Aggregation,
        timeline_kind::SUMMARY,
        EventStatus::Succeeded
    ));
    assert!(has(
        TimelineStep::Aggregation,
        timeline_kind::DRAFT,
        EventStatus::Succeeded
    ));
    assert!(has(
        TimelineStep::Decision,
        timeline_kind::STATUS_CHANGED,
        EventStatus::Succeeded
    ));
    assert!(!has(
        TimelineStep::Aggregation,
        timeline_kind::SUMMARY,
        EventStatus::Started
    ));
    let scoring = events
        .iter()
        .find(|event| {
            event.step == TimelineStep::Combination && event.detail["source"] == "scoring"
        })
        .unwrap();
    assert_eq!(scoring.detail["score"], 85);
    assert_eq!(scoring.detail["verdict"], "fiable");
    assert!(timeline.rounds.iter().all(|round| round.number == 1));
    let status = |stage: Stage| {
        timeline
            .pipeline
            .stages
            .iter()
            .find(|state| state.stage == stage)
            .unwrap()
            .clone()
    };
    for done in [
        Stage::Detection,
        Stage::Qualification,
        Stage::Scoring,
        Stage::Synthesis,
        Stage::Draft,
    ] {
        assert_eq!(status(done).status, StageStatus::Done, "{done:?}");
    }
    let decision = status(Stage::Decision);
    assert_eq!(decision.status, StageStatus::Waiting);
    assert_eq!(
        decision.reason_code,
        Some(StageReason::DraftAwaitingDecision)
    );
    assert_eq!(timeline.pipeline.current, Some(Stage::Decision));

    let user = Uuid::now_v7();
    query("INSERT INTO users (id, sub, username, display_name) VALUES ($1, $1::text, 'alice', 'Alice')")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    harness
        .usecase
        .change_lead_status(lead.id, LeadStatus::Validated, UserId(user), None)
        .await
        .unwrap();
    let decided = harness.usecase.lead_timeline(lead.id).await.unwrap();
    let change = decided
        .rounds
        .iter()
        .flat_map(|round| round.events.iter())
        .find(|event| {
            event.kind == timeline_kind::STATUS_CHANGED && event.detail["to"] == "validated"
        })
        .unwrap();
    assert_eq!(change.detail["decisions"][0], "lead_validated");
    assert_eq!(change.step, TimelineStep::Decision);
    let decision = decided
        .pipeline
        .stages
        .iter()
        .find(|state| state.stage == Stage::Decision)
        .unwrap();
    assert_eq!(decision.status, StageStatus::Done);
    assert_ne!(decided.pipeline.current, Some(Stage::Decision));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_scoring_call_failure_is_recorded_with_its_next_attempt(pool: PgPool) {
    let harness = Harness::new(pool).await;
    harness.script("model-synth#scoring", vec![network()]);
    let profile = harness.sample_flow(vec![qualification_agent("fit")]).await;
    harness.qualify().await;
    harness.score().await;

    let lead = harness.lead(profile, "acme.com").await;
    let timeline = harness.usecase.lead_timeline(lead.id).await.unwrap();
    let failure = timeline
        .rounds
        .iter()
        .flat_map(|round| round.events.iter())
        .find(|event| event.kind == timeline_kind::CALL_FAILED)
        .unwrap();
    assert_eq!(failure.step, TimelineStep::Scoring);
    assert!(failure.detail["next_attempt_at"].is_string());
    let scoring = timeline
        .pipeline
        .stages
        .iter()
        .find(|state| state.stage == Stage::Scoring)
        .unwrap();
    assert_eq!(scoring.status, StageStatus::Waiting);
    assert_eq!(scoring.reason_code, Some(StageReason::CallFailureBackoff));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_decided_before_its_scoring_is_not_scored(pool: PgPool) {
    let harness = Harness::new(pool.clone()).await;
    harness
        .profile(
            vec![qualification_agent("fit")],
            vec![scoring_agent("global", 5, None, 1)],
        )
        .await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.qualify().await;
    sqlx::query("UPDATE leads SET status = 'rejected' WHERE domain = 'acme.com'")
        .execute(&pool)
        .await
        .unwrap();

    let report = harness.score().await;

    assert_eq!((report.leads, report.backlog), (0, 0));
    assert!(harness.scoring_calls().is_empty());
}
