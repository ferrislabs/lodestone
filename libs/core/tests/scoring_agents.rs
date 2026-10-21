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
    ActiveProfile, Clock, CreateProfileCommand, GenerationParams, Lead, LeadDetail, LeadRepository,
    LlmError, LlmProvider, LlmRequest, LlmResponse, LodestoneUseCase, Observation, ProfileId,
    ProfileName, Provenance, ProviderDefinition, ProviderId, ProviderKind, ResultStatus,
    ReviseProfileCommand, SignalKind, SourceName, Tier, TokenPricing, TokenUsage, UnitInterval,
    ValidatedProfile, Verdict, VerdictReason, VerdictSource,
    evaluation::{EvaluationSettings, ProviderBuilder},
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
                settings(),
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
                settings(),
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
                settings(),
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
