use std::{
    num::{NonZeroU32, NonZeroUsize},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, SecondsFormat, Utc};
use common::{CoreError, generate_uuid_v7};
use futures_util::{StreamExt, stream};
use lodestone_macros::transactional;
use serde_json::json;
use tracing::{info, warn};

use crate::{
    application::{
        LodestoneUseCase,
        llm::{
            gateway::{
                CallSettings, LlmCallError, Prompt, RunBudget, SchemaCall, Structured,
                call_with_schema_until,
            },
            limits::{Throttled, locked},
        },
        timeline::{
            AgentTrace, BUDGET_EXHAUSTED, agent_entries, agent_interrupted, agent_span,
            agent_started, call_failed, combined, in_flight::Subject as Running, round_key,
        },
    },
    domain::{
        agent_work::{
            AgentWork, CancelReason, WorkKind, WorkTarget, first_stop, ports::AgentWorkRepository,
        },
        calibration::{Calibrations, opinion_score},
        company::{CompanyDossier, DossierDocument},
        facts::{LeadFacts, signal_sentence},
        job::ports::Clock,
        lead::{Lead, LeadId, Signal},
        llm::{
            GenerationParams, GenerationSnapshot, LlmError, LlmProvider, ProviderDefinition,
            ReasoningEffort, RunProvenance, UntrustedDocument,
            cache::{CacheScope, LlmCache},
            provenance::DEFAULT_SAMPLES,
            provider::{ProviderId, SecretCipher, Tier, TierError, TokenPricing},
        },
        profile::{
            ActiveProfile, RevisionNumber,
            contract::contract_schema,
            definition::{ProfileDefinition, QualificationAgent},
        },
        qualification::{
            AgentCallFailure, AgentFailure, AgentKind, AgentOpinion, Completion, DueGates,
            EvidenceStrength, Judgement, QualificationResult, ResultId, ResultStatus,
            SCORING_VERSION, ValidOutput, Verdict, VerdictCap, Violations, agent_retry_delay,
            exclusions, judge,
            ports::QualificationRepository,
            prompt::{AgentPrompt, build, lead_document, with_ungrounded, with_violations},
            sample_record, validate_answers, vote,
        },
        timeline::{
            EventActor, EventStatus, TimelineEntry, TimelineStep, kind, ports::TimelineRepository,
        },
    },
};

mod trial;

pub(crate) use trial::{Attempt, Qualified, TrialWorks, qualify_in_memory};

pub const QUALIFICATION_LISTENER: &str = "qualification";
const RESCORE_BATCH_SIZE: u32 = 500;
const PROVIDER_UNAVAILABLE: &str = "fournisseur indisponible";
const ESCALATION_TEMPERATURE: f64 = 0.0;
const ESCALATION_TAG: &str = "+escalation";
const ESCALATION_LABEL: &str = "escalation";

pub trait ProviderBuilder: Send + Sync {
    type Provider: LlmProvider;

    fn build(&self, definition: &ProviderDefinition) -> Result<Self::Provider, LlmError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvaluationSettings {
    pub max_tokens: u32,
    pub budget_cents: NonZeroU32,
    pub batch_size: u32,
    pub base_delay: Duration,
    pub concurrency: NonZeroUsize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EvaluationReport {
    pub leads: u64,
    pub valid: u64,
    pub invalid: u64,
    pub rescored: u64,
    pub deferred: u64,
    pub stopped: u64,
    pub backlog: u64,
    pub budget_exhausted: bool,
}

pub(crate) struct Loaded<P> {
    pub(crate) provider: Throttled<P>,
    pub(crate) id: ProviderId,
    pub(crate) name: String,
    pub(crate) model: String,
    pub(crate) generation: GenerationParams,
    pub(crate) reasoning_effort: Option<ReasoningEffort>,
    pub(crate) max_retries: u8,
    pub(crate) pricing: TokenPricing,
}

impl<P> Loaded<P> {
    pub(crate) fn provenance(
        &self,
        model: &str,
        prompt_version: String,
        default_max_tokens: u32,
    ) -> RunProvenance {
        self.sampled_provenance(
            model,
            prompt_version,
            default_max_tokens,
            None,
            DEFAULT_SAMPLES,
        )
    }

    pub(crate) fn sampled_provenance(
        &self,
        model: &str,
        prompt_version: String,
        default_max_tokens: u32,
        temperature: Option<f64>,
        samples: u32,
    ) -> RunProvenance {
        let mut generation =
            GenerationSnapshot::new(&self.generation, self.reasoning_effort, default_max_tokens);
        if temperature.is_some() {
            generation.temperature = temperature;
        }
        RunProvenance {
            provider_id: Some(self.id),
            provider_name: self.name.clone(),
            model: model.to_owned(),
            prompt_version,
            generation,
            samples: samples.max(DEFAULT_SAMPLES),
        }
    }
}

pub(crate) async fn load_provider<B, S>(
    usecase: &LodestoneUseCase,
    builder: &B,
    cipher: &S,
    wanted: Option<ProviderId>,
    default_tier: Tier,
    label: &str,
) -> Result<Option<Loaded<B::Provider>>, CoreError>
where
    B: ProviderBuilder,
    S: SecretCipher,
{
    let resolved = match wanted {
        Some(id) => usecase.resolve_llm_provider(id, cipher).await?,
        None => match usecase.resolve_llm_tier(default_tier, cipher).await {
            Ok(resolved) => Some(resolved),
            Err(TierError::Storage(message)) => return Err(CoreError::Internal(message)),
            Err(_) => None,
        },
    };
    Ok(
        resolved.and_then(|resolved| match builder.build(&resolved.definition) {
            Ok(provider) => Some(Loaded {
                provider: Throttled::new(
                    provider,
                    usecase.provider_gate(resolved.id, resolved.max_concurrency),
                ),
                id: resolved.id,
                name: resolved.name.clone(),
                model: resolved.definition.model.clone(),
                generation: resolved.definition.generation,
                reasoning_effort: resolved.definition.reasoning_effort,
                max_retries: resolved.max_retries,
                pricing: resolved.pricing,
            }),
            Err(error) => {
                warn!(label, %error, "qualification provider could not be built");
                None
            }
        }),
    )
}

pub fn render_signal(signal: &Signal) -> String {
    format!(
        "{}\nsource: {}\nobserved: {}",
        signal_sentence(signal),
        signal.source.as_str(),
        signal
            .observed_at
            .to_rfc3339_opts(SecondsFormat::Secs, true),
    )
}

pub(crate) struct LeadMaterial {
    pub(crate) signals: Vec<Signal>,
    pub(crate) facts: LeadFacts,
    pub(crate) dossier: Option<CompanyDossier>,
    overview: String,
    contents: Vec<String>,
    grounding: Vec<DossierDocument>,
}

impl LeadMaterial {
    pub(crate) async fn load(usecase: &LodestoneUseCase, lead: &Lead) -> Result<Self, CoreError> {
        let (signals, facts, dossier) = usecase.signals_with_facts(lead).await?;
        let grounding = usecase.grounding_documents(lead, dossier.as_ref()).await?;
        Ok(Self {
            overview: lead_document(lead.domain.as_str(), &facts),
            contents: signals.iter().map(render_signal).collect(),
            signals,
            facts,
            dossier,
            grounding,
        })
    }

    pub(crate) fn documents(&self) -> Vec<UntrustedDocument<'_>> {
        let mut documents = Vec::with_capacity(self.signals.len() + self.grounding.len() + 1);
        documents.push(UntrustedDocument {
            source: "lead",
            content: &self.overview,
        });
        documents.extend(
            self.signals
                .iter()
                .zip(&self.contents)
                .map(|(signal, content)| UntrustedDocument {
                    source: signal.provenance.as_str(),
                    content,
                }),
        );
        documents.extend(self.grounding.iter().map(|document| UntrustedDocument {
            source: &document.source,
            content: &document.content,
        }));
        documents
    }
}

type Entry<P> = (Option<ProviderId>, Option<Arc<Loaded<P>>>);

pub(crate) struct Providers<P> {
    entries: Mutex<Vec<Entry<P>>>,
}

impl<P> Providers<P> {
    pub(crate) fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    fn cached(&self, wanted: Option<ProviderId>) -> Option<Option<Arc<Loaded<P>>>> {
        locked(&self.entries)
            .iter()
            .find(|(key, _)| *key == wanted)
            .map(|(_, loaded)| loaded.clone())
    }

    fn keep(
        &self,
        wanted: Option<ProviderId>,
        loaded: Option<Loaded<P>>,
    ) -> Option<Arc<Loaded<P>>> {
        let mut entries = locked(&self.entries);
        if let Some((_, known)) = entries.iter().find(|(key, _)| *key == wanted) {
            return known.clone();
        }
        let loaded = loaded.map(Arc::new);
        entries.push((wanted, loaded.clone()));
        loaded
    }
}

pub(crate) async fn load_cached<B, S>(
    usecase: &LodestoneUseCase,
    builder: &B,
    cipher: &S,
    wanted: Option<ProviderId>,
    tier: Tier,
    label: &str,
    providers: &Providers<B::Provider>,
) -> Result<Option<Arc<Loaded<B::Provider>>>, CoreError>
where
    B: ProviderBuilder,
    S: SecretCipher,
{
    if let Some(known) = providers.cached(wanted) {
        return Ok(known);
    }
    let loaded = load_provider(usecase, builder, cipher, wanted, tier, label).await?;
    Ok(providers.keep(wanted, loaded))
}

struct Evaluator<'a, B, S, C> {
    builder: &'a B,
    cipher: &'a S,
    cache: &'a C,
    settings: EvaluationSettings,
}

pub(crate) const CALL_FAILURE_PREFIX: &str = "Échec de l'appel au modèle";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallFailure {
    pub(crate) agent_id: String,
    pub(crate) provider_id: ProviderId,
    pub(crate) error: String,
}

impl CallFailure {
    pub(crate) fn into_record(
        self,
        lead_id: LeadId,
        kind: AgentKind,
        profile_revision: RevisionNumber,
    ) -> AgentCallFailure {
        AgentCallFailure {
            lead_id,
            kind,
            profile_revision,
            agent_id: self.agent_id,
            provider_id: Some(self.provider_id),
            error: self.error,
        }
    }
}

enum AgentOutcome {
    Result(Box<QualificationResult>, Option<AgentOpinion>),
    CallFailed(CallFailure),
    BudgetExhausted,
    Stopped(CancelReason),
}

enum PassOutcome {
    Done(Pass),
    CallFailed(CallFailure),
    BudgetExhausted,
    Stopped(AgentWork, CancelReason),
}

enum LeadOutcome {
    Done { valid: u64, invalid: u64 },
    Deferred,
    BudgetExhausted,
    Stopped,
}

pub(crate) enum LeadStep<T> {
    Skipped,
    Halted(CancelReason),
    Ran(T),
}

pub(crate) fn lead_step<T>(halted: &AtomicBool, works: &[AgentWork]) -> Option<LeadStep<T>> {
    if halted.load(Ordering::Acquire) {
        return Some(LeadStep::Skipped);
    }
    first_stop(works).map(LeadStep::Halted)
}

enum SampleOutcome {
    Valid { output: ValidOutput, model: String },
    Invalid { error: String },
    CallFailed { error: String },
    BudgetExhausted,
    Stopped(CancelReason),
}

enum Escalated {
    Replaced(bool),
    BudgetExhausted,
    Stopped(AgentWork, CancelReason),
}

struct Pass {
    results: Vec<QualificationResult>,
    opinions: Vec<Option<AgentOpinion>>,
    entries: Vec<TimelineEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RunPlan {
    provider: Option<ProviderId>,
    samples: u32,
    temperature: Option<f64>,
    escalated: bool,
}

impl RunPlan {
    fn primary(agent: &QualificationAgent) -> Self {
        let samples = u32::from(agent.samples).max(1);
        Self {
            provider: agent.provider_id.map(ProviderId),
            samples,
            temperature: (samples > 1).then_some(agent.sampling_temperature),
            escalated: false,
        }
    }

    fn escalation(provider: ProviderId) -> Self {
        Self {
            provider: Some(provider),
            samples: 1,
            temperature: Some(ESCALATION_TEMPERATURE),
            escalated: true,
        }
    }
}

struct Subject<'a> {
    active: &'a ActiveProfile,
    definition: &'a ProfileDefinition,
    calibrations: &'a Calibrations,
    lead: &'a Lead,
    documents: &'a [UntrustedDocument<'a>],
    fresh: bool,
    round: String,
    at: DateTime<Utc>,
}

struct AgentRun<'a, P> {
    loaded: &'a Loaded<P>,
    subject: &'a Subject<'a>,
    agent: &'a QualificationAgent,
    prompt: &'a AgentPrompt,
    work: &'a AgentWork,
}

pub(crate) fn qualification_works(
    usecase: &LodestoneUseCase,
    active: &ActiveProfile,
) -> Vec<AgentWork> {
    active
        .revision
        .definition
        .definition()
        .qualification_agents
        .iter()
        .filter(|agent| agent.enabled)
        .map(|agent| {
            usecase.agent_work(active, WorkTarget::new(WorkKind::Qualification, &agent.id))
        })
        .collect()
}

pub(crate) fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

pub(crate) fn agent_prompt_version(prompt: &AgentPrompt) -> String {
    format!("qualification-agent@{}", prompt.hash)
}

fn failed(
    subject: &Subject<'_>,
    agent: &QualificationAgent,
    provenance: RunProvenance,
    error: String,
    raw_samples: serde_json::Value,
) -> QualificationResult {
    QualificationResult {
        id: ResultId(generate_uuid_v7()),
        lead_id: subject.lead.id,
        profile_id: subject.active.profile.id,
        profile_revision: subject.active.revision.number,
        agent_id: agent.id.clone(),
        provenance,
        status: ResultStatus::Invalid,
        summary: None,
        answers: serde_json::json!({}),
        agent_score: None,
        error: Some(error),
        escalated: false,
        raw_samples,
        created_at: subject.at,
    }
}

fn succeeded(
    subject: &Subject<'_>,
    agent: &QualificationAgent,
    provenance: RunProvenance,
    output: &ValidOutput,
    stored: (bool, serde_json::Value),
) -> (QualificationResult, AgentOpinion) {
    let score = opinion_score(
        agent,
        &output.answers,
        subject.definition.verdict.calibration_floor,
        subject.calibrations,
    );
    let result = QualificationResult {
        id: ResultId(generate_uuid_v7()),
        lead_id: subject.lead.id,
        profile_id: subject.active.profile.id,
        profile_revision: subject.active.revision.number,
        agent_id: agent.id.clone(),
        provenance,
        status: ResultStatus::Valid,
        summary: Some(output.summary.clone()),
        answers: output.answers.to_json(),
        agent_score: Some(score),
        error: None,
        escalated: stored.0,
        raw_samples: stored.1,
        created_at: subject.at,
    };
    let opinion = AgentOpinion {
        score,
        axes: output.answers.0.len(),
    };
    (result, opinion)
}

fn judgement_of(
    definition: &ProfileDefinition,
    facts: &LeadFacts,
    opinions: &[AgentOpinion],
    at: DateTime<Utc>,
) -> Judgement {
    judge(
        opinions,
        EvidenceStrength::compute(facts, &definition.evidence, at),
        definition.verdict,
        &definition.evidence,
    )
}

impl<B, S, C> Evaluator<'_, B, S, C>
where
    B: ProviderBuilder,
    S: SecretCipher,
    C: LlmCache,
{
    async fn load(
        &self,
        usecase: &LodestoneUseCase,
        wanted: Option<ProviderId>,
        label: &str,
        providers: &Providers<B::Provider>,
    ) -> Result<Option<Arc<Loaded<B::Provider>>>, CoreError> {
        load_cached(
            usecase,
            self.builder,
            self.cipher,
            wanted,
            Tier::Triage,
            label,
            providers,
        )
        .await
    }

    async fn ask(
        &self,
        run: &AgentRun<'_, B::Provider>,
        instruction: &str,
        call: (RunPlan, u32),
        budget: &RunBudget,
    ) -> Result<Structured<serde_json::Value>, LlmCallError> {
        let AgentRun {
            loaded,
            subject,
            agent,
            prompt,
            work,
        } = *run;
        let (plan, sample) = call;
        let schema = contract_schema(agent);
        let tag = if plan.escalated {
            format!("qualification@{}{ESCALATION_TAG}", prompt.hash)
        } else {
            format!("qualification@{}", prompt.hash)
        };
        let scope = CacheScope {
            profile_id: subject.active.profile.id,
            revision: subject.active.revision.number,
            prompt: tag,
            domain: subject.lead.domain.clone(),
            agent: Some(agent.id.clone()),
            sample,
            fresh: subject.fresh,
        };
        let call = SchemaCall {
            scope: &scope,
            prompt: Prompt {
                system: &prompt.system,
                instruction,
                documents: subject.documents,
                max_tokens: self.settings.max_tokens,
                temperature: plan.temperature,
            },
            schema: &schema,
        };
        call_with_schema_until(
            &loaded.provider,
            self.cache,
            call,
            |output| validate_answers(agent, output, subject.documents).is_ok(),
            CallSettings {
                max_retries: loaded.max_retries,
                pricing: loaded.pricing,
                base_delay: self.settings.base_delay,
            },
            budget,
            work,
        )
        .await
    }

    async fn run_sample(
        &self,
        run: &AgentRun<'_, B::Provider>,
        call: (RunPlan, u32),
        budget: &RunBudget,
    ) -> SampleOutcome {
        let AgentRun {
            subject,
            agent,
            prompt,
            ..
        } = *run;
        let mut instruction = prompt.instruction.clone();
        let mut last: Option<Violations> = None;
        let mut fallback: Option<(ValidOutput, String)> = None;
        for attempt in 0..2 {
            let structured = match self.ask(run, &instruction, call, budget).await {
                Ok(structured) => structured,
                Err(LlmCallError::BudgetExhausted) => return SampleOutcome::BudgetExhausted,
                Err(LlmCallError::Cancelled(reason)) => return SampleOutcome::Stopped(reason),
                Err(error) => {
                    if let Some((output, model)) = fallback {
                        return SampleOutcome::Valid { output, model };
                    }
                    let call_failure = error.is_call_failure();
                    warn!(
                        agent = agent.id.as_str(),
                        %error,
                        provider_message = error.provider_message(),
                        call_failure,
                        "qualification call failed"
                    );
                    let error = format!("{CALL_FAILURE_PREFIX} : {error}");
                    return if call_failure {
                        SampleOutcome::CallFailed { error }
                    } else {
                        SampleOutcome::Invalid { error }
                    };
                }
            };
            match validate_answers(agent, &structured.value, subject.documents) {
                Ok(output) => {
                    let ungrounded = output.answers.ungrounded();
                    if attempt == 0 && !ungrounded.is_empty() {
                        warn!(
                            agent = agent.id.as_str(),
                            axes = ungrounded.len(),
                            "qualification answers without a verifiable quote, retrying"
                        );
                        instruction = with_ungrounded(&prompt.instruction, &ungrounded);
                        fallback = Some((output, structured.model));
                        continue;
                    }
                    return SampleOutcome::Valid {
                        output,
                        model: structured.model,
                    };
                }
                Err(violations) => {
                    warn!(agent = agent.id.as_str(), %violations, "qualification answer broke the contract");
                    instruction = with_violations(&prompt.instruction, &violations);
                    last = Some(violations);
                }
            }
        }
        if let Some((output, model)) = fallback {
            return SampleOutcome::Valid { output, model };
        }
        let message = last.map_or_else(String::new, |violations| violations.to_string());
        SampleOutcome::Invalid {
            error: format!("Réponse non conforme au contrat : {message}"),
        }
    }

    async fn evaluate_agent(
        &self,
        usecase: &LodestoneUseCase,
        subject: &Subject<'_>,
        agent: &QualificationAgent,
        (plan, work): (RunPlan, &AgentWork),
        providers: &Providers<B::Provider>,
        budget: &RunBudget,
    ) -> Result<AgentOutcome, CoreError> {
        let schema = contract_schema(agent);
        let prompt = build(subject.definition, agent, &schema);
        let version = agent_prompt_version(&prompt);
        let Some(loaded) = self
            .load(usecase, plan.provider, agent.id.as_str(), providers)
            .await?
        else {
            let result = failed(
                subject,
                agent,
                RunProvenance::unresolved(plan.provider, version),
                PROVIDER_UNAVAILABLE.to_owned(),
                serde_json::json!([]),
            );
            return Ok(AgentOutcome::Result(Box::new(result), None));
        };
        let loaded = loaded.as_ref();
        let run = &AgentRun {
            loaded,
            subject,
            agent,
            prompt: &prompt,
            work,
        };
        let mut valid: Vec<ValidOutput> = Vec::new();
        let mut records: Vec<serde_json::Value> = Vec::new();
        let mut model: Option<String> = None;
        let mut first_error: Option<String> = None;
        let mut call_errors: Vec<String> = Vec::new();
        let mut samples = stream::iter(0..plan.samples)
            .map(move |sample| async move {
                let outcome = match work.stop_reason() {
                    Some(reason) => SampleOutcome::Stopped(reason),
                    None => self.run_sample(run, (plan, sample), budget).await,
                };
                (sample, outcome)
            })
            .buffered(self.settings.concurrency.get());
        while let Some((sample, outcome)) = samples.next().await {
            match outcome {
                SampleOutcome::BudgetExhausted => return Ok(AgentOutcome::BudgetExhausted),
                SampleOutcome::Stopped(reason) => return Ok(AgentOutcome::Stopped(reason)),
                SampleOutcome::CallFailed { error } => {
                    warn!(
                        agent = agent.id.as_str(),
                        sample, "qualification sample dropped, its call failed"
                    );
                    records.push(sample_record(sample, Err(&error)));
                    call_errors.push(error);
                }
                SampleOutcome::Valid {
                    output,
                    model: answered,
                } => {
                    records.push(sample_record(sample, Ok(&output)));
                    model.get_or_insert(answered);
                    valid.push(output);
                }
                SampleOutcome::Invalid { error } => {
                    warn!(
                        agent = agent.id.as_str(),
                        sample, "qualification sample dropped"
                    );
                    records.push(sample_record(sample, Err(&error)));
                    first_error.get_or_insert(error);
                }
            }
        }
        if valid.is_empty()
            && first_error.is_none()
            && let Some(error) = call_errors.into_iter().next()
        {
            return Ok(AgentOutcome::CallFailed(CallFailure {
                agent_id: agent.id.as_str().to_owned(),
                provider_id: loaded.id,
                error,
            }));
        }
        let raw_samples = if plan.samples > 1 {
            serde_json::Value::Array(records)
        } else {
            serde_json::json!([])
        };
        let voted = if plan.samples > 1 {
            vote(agent, &valid)
        } else {
            valid.pop()
        };
        let valid_samples = u32::try_from(valid.len().max(1)).unwrap_or(plan.samples);
        let Some(output) = voted else {
            let result = failed(
                subject,
                agent,
                loaded.sampled_provenance(
                    &loaded.model,
                    version,
                    self.settings.max_tokens,
                    plan.temperature,
                    DEFAULT_SAMPLES,
                ),
                first_error.unwrap_or_default(),
                raw_samples,
            );
            return Ok(AgentOutcome::Result(Box::new(result), None));
        };
        let samples = if plan.samples > 1 {
            valid_samples
        } else {
            DEFAULT_SAMPLES
        };
        let (result, opinion) = succeeded(
            subject,
            agent,
            loaded.sampled_provenance(
                model.as_deref().unwrap_or(&loaded.model),
                version,
                self.settings.max_tokens,
                plan.temperature,
                samples,
            ),
            &output,
            (plan.escalated, raw_samples),
        );
        Ok(AgentOutcome::Result(Box::new(result), Some(opinion)))
    }

    async fn run_agents<K: Clock>(
        &self,
        usecase: &LodestoneUseCase,
        subject: &Subject<'_>,
        escalation: Option<ProviderId>,
        providers: &Providers<B::Provider>,
        (budget, clock): (&RunBudget, &K),
    ) -> Result<PassOutcome, CoreError> {
        let mut pass = Pass {
            results: Vec::new(),
            opinions: Vec::new(),
            entries: Vec::new(),
        };
        let agents: &Vec<&QualificationAgent> = &subject
            .definition
            .qualification_agents
            .iter()
            .filter(|agent| agent.enabled)
            .collect();
        let mut outcomes = stream::iter(0..agents.len())
            .map(move |index| async move {
                let agent = agents[index];
                let plan = escalation.map_or_else(|| RunPlan::primary(agent), RunPlan::escalation);
                let work = usecase.agent_work(
                    subject.active,
                    WorkTarget::new(WorkKind::Qualification, &agent.id),
                );
                let step = if plan.escalated {
                    TimelineStep::Escalation
                } else {
                    TimelineStep::Qualification
                };
                let started = clock.now();
                let span = agent_span(step, agent.id.as_str(), &subject.round, started);
                let outcome = match work.stop_reason() {
                    Some(reason) => AgentOutcome::Stopped(reason),
                    None => {
                        let _running = usecase.enter_step(
                            Running::Lead(subject.lead.id),
                            step,
                            EventActor::agent(agent.id.as_str(), &agent.name),
                            started,
                        );
                        usecase
                            .record_timeline(
                                subject.lead.id,
                                &agent_started(
                                    step,
                                    (agent.id.as_str(), &agent.name),
                                    &span,
                                    started,
                                ),
                            )
                            .await?;
                        self.evaluate_agent(
                            usecase,
                            subject,
                            agent,
                            (plan, &work),
                            providers,
                            budget,
                        )
                        .await?
                    }
                };
                Ok::<_, CoreError>((agent, plan, work, outcome, (span, started, clock.now())))
            })
            .buffered(self.settings.concurrency.get());
        while let Some(next) = outcomes.next().await {
            let (agent, plan, work, outcome, (span, started, finished)) = next?;
            let step = if plan.escalated {
                TimelineStep::Escalation
            } else {
                TimelineStep::Qualification
            };
            let interrupted = |ending: (EventStatus, Option<&str>)| {
                agent_interrupted(
                    step,
                    (agent.id.as_str(), &agent.name),
                    (&span, started),
                    finished,
                    ending,
                )
            };
            match outcome {
                AgentOutcome::BudgetExhausted => {
                    usecase
                        .record_timeline(
                            subject.lead.id,
                            &interrupted((EventStatus::Skipped, Some(BUDGET_EXHAUSTED))),
                        )
                        .await?;
                    return Ok(PassOutcome::BudgetExhausted);
                }
                AgentOutcome::Stopped(reason) => {
                    usecase
                        .record_timeline(
                            subject.lead.id,
                            &interrupted((EventStatus::Skipped, Some(reason.as_str()))),
                        )
                        .await?;
                    return Ok(PassOutcome::Stopped(work, reason));
                }
                AgentOutcome::CallFailed(failure) => {
                    usecase
                        .record_timeline(
                            subject.lead.id,
                            &interrupted((EventStatus::Failed, Some(&failure.error))),
                        )
                        .await?;
                    return Ok(PassOutcome::CallFailed(failure));
                }
                AgentOutcome::Result(result, opinion) => {
                    info!(
                        monotonic_counter.qualification_results = 1u64,
                        agent = agent.id.as_str(),
                        status = result.status.as_str(),
                        escalated = plan.escalated,
                        "qualification result recorded"
                    );
                    pass.entries.extend(agent_entries(AgentTrace {
                        step,
                        agent: (agent.id.as_str(), &agent.name),
                        provenance: &result.provenance,
                        valid: result.status == ResultStatus::Valid,
                        error: result.error.as_deref(),
                        raw_samples: &result.raw_samples,
                        detail: json!({
                            "agent_score": result.agent_score,
                            "samples": result.provenance.samples,
                            "escalated": plan.escalated,
                            "summary": result.summary,
                        }),
                        span: &span,
                        started_at: started,
                        finished_at: finished,
                    }));
                    pass.results.push(*result);
                    pass.opinions.push(opinion);
                }
            }
        }
        Ok(PassOutcome::Done(pass))
    }

    async fn escalate<K: Clock>(
        &self,
        usecase: &LodestoneUseCase,
        subject: &Subject<'_>,
        provider: ProviderId,
        pass: &mut Pass,
        providers: &Providers<B::Provider>,
        (budget, clock): (&RunBudget, &K),
    ) -> Result<Escalated, CoreError> {
        if self
            .load(usecase, Some(provider), ESCALATION_LABEL, providers)
            .await?
            .is_none()
        {
            warn!(lead_id = %subject.lead.id, "escalation skipped, its provider is unavailable");
            return Ok(Escalated::Replaced(false));
        }
        let escalated = match self
            .run_agents(usecase, subject, Some(provider), providers, (budget, clock))
            .await?
        {
            PassOutcome::Done(escalated) => escalated,
            PassOutcome::BudgetExhausted => return Ok(Escalated::BudgetExhausted),
            PassOutcome::Stopped(work, reason) => return Ok(Escalated::Stopped(work, reason)),
            PassOutcome::CallFailed(failure) => {
                warn!(
                    lead_id = %subject.lead.id,
                    agent = failure.agent_id,
                    error = failure.error,
                    "escalation skipped, its model call failed"
                );
                return Ok(Escalated::Replaced(false));
            }
        };
        pass.entries.extend(escalated.entries);
        let mut replaced = false;
        for (index, (result, opinion)) in escalated
            .results
            .into_iter()
            .zip(escalated.opinions)
            .enumerate()
        {
            if opinion.is_some() {
                pass.results[index] = result;
                pass.opinions[index] = opinion;
                replaced = true;
            }
        }
        Ok(Escalated::Replaced(replaced))
    }

    async fn evaluate_lead<K: Clock>(
        &self,
        usecase: &LodestoneUseCase,
        active: &ActiveProfile,
        lead: &Lead,
        providers: &Providers<B::Provider>,
        budget: &RunBudget,
        clock: &K,
    ) -> Result<LeadOutcome, CoreError> {
        let read_at = clock.now();
        let fresh = usecase.lead_fresh_calls(lead.id).await?;
        let definition = active.revision.definition.definition();
        let calibrations = usecase.profile_calibrations(active).await?;
        let material = LeadMaterial::load(usecase, lead).await?;
        let documents = material.documents();
        let round = round_key(lead.qualified_at);
        let trigger = if lead.qualified_at.is_none() {
            "first"
        } else if fresh {
            "manual"
        } else {
            "requalification"
        };
        usecase
            .record_timeline(
                lead.id,
                &TimelineEntry::round_started(read_at, trigger, lead.qualified_at),
            )
            .await?;
        let subject = Subject {
            active,
            definition,
            calibrations: &calibrations,
            lead,
            documents: &documents,
            fresh,
            round,
            at: clock.now(),
        };
        let at = subject.at;
        let revision = active.revision.number;
        let mut pass = match self
            .run_agents(usecase, &subject, None, providers, (budget, clock))
            .await?
        {
            PassOutcome::Done(pass) => pass,
            PassOutcome::BudgetExhausted => return Ok(LeadOutcome::BudgetExhausted),
            PassOutcome::Stopped(work, reason) => {
                usecase.note_stop(lead.id, &work, reason, at).await?;
                return Ok(LeadOutcome::Stopped);
            }
            PassOutcome::CallFailed(failure) => {
                let record = failure.into_record(lead.id, AgentKind::Qualification, revision);
                let attempts = usecase.note_agent_failure(&record, clock.now()).await?;
                warn!(
                    lead_id = %lead.id,
                    agent = record.agent_id,
                    attempts,
                    error = record.error,
                    "qualification deferred, a model call failed"
                );
                return Ok(LeadOutcome::Deferred);
            }
        };
        let mut escalated_revision = None;
        let scored = |pass: &Pass| -> Vec<AgentOpinion> {
            pass.opinions.iter().flatten().copied().collect()
        };
        let mut judgement = judgement_of(definition, &material.facts, &scored(&pass), at);
        if escalated_revision.is_none()
            && let (Some(provider), Some(confidence)) =
                (definition.escalation.provider, judgement.confidence)
            && definition.escalation.triggers(confidence)
        {
            info!(lead_id = %lead.id, confidence, "grey-zone lead, escalating");
            pass.entries.push(
                TimelineEntry::new(
                    TimelineStep::Escalation,
                    kind::ESCALATED,
                    EventStatus::Succeeded,
                    clock.now(),
                    format!("escalated:{}", subject.round),
                )
                .detail(json!({ "confidence": confidence })),
            );
            match self
                .escalate(
                    usecase,
                    &subject,
                    ProviderId(provider),
                    &mut pass,
                    providers,
                    (budget, clock),
                )
                .await?
            {
                Escalated::BudgetExhausted => return Ok(LeadOutcome::BudgetExhausted),
                Escalated::Stopped(work, reason) => {
                    usecase.note_stop(lead.id, &work, reason, at).await?;
                    return Ok(LeadOutcome::Stopped);
                }
                Escalated::Replaced(true) => {
                    escalated_revision = Some(revision);
                    judgement = judgement_of(definition, &material.facts, &scored(&pass), at);
                    info!(
                        monotonic_counter.qualification_escalations = 1u64,
                        "lead escalated"
                    );
                }
                Escalated::Replaced(false) => {}
            }
        }
        let valid = count(pass.opinions.iter().flatten().count());
        let invalid = count(pass.results.len()) - valid;
        let mut sources: Vec<String> = material
            .signals
            .iter()
            .map(|signal| signal.source.as_str().to_owned())
            .collect();
        sources.sort();
        sources.dedup();
        pass.entries.push(combined(
            "indicators",
            &subject.round,
            clock.now(),
            json!({
                "source": "indicators",
                "confidence": judgement.confidence,
                "opinion": judgement.opinion,
                "verdict": judgement.verdict.map(Verdict::as_str),
                "cap": judgement.cap.map(VerdictCap::as_str),
            }),
        ));
        let completion = Completion {
            lead_id: lead.id,
            profile_revision: revision,
            sources,
            judgement,
            exclusions: exclusions(&pass.results, &definition.qualification_agents),
            scoring_version: SCORING_VERSION,
            dossier_at: material
                .dossier
                .as_ref()
                .and_then(|dossier| dossier.content_at),
            escalated_revision,
            read_at,
            at,
        };
        let works = qualification_works(usecase, active);
        if let Some(work) = works.iter().find(|work| !work.is_live())
            && let Some(reason) = work.stop_reason()
        {
            usecase.note_stop(lead.id, work, reason, at).await?;
            return Ok(LeadOutcome::Stopped);
        }
        usecase
            .store_evaluation(&pass.results, &completion, &pass.entries)
            .await?;
        Ok(LeadOutcome::Done { valid, invalid })
    }
}

impl LodestoneUseCase {
    pub async fn has_enabled_qualification_agent(&self) -> Result<bool, CoreError> {
        Ok(self.list_active_profiles().await?.iter().any(|active| {
            active
                .revision
                .definition
                .definition()
                .qualification_agents
                .iter()
                .any(|agent| agent.enabled)
        }))
    }

    pub async fn qualify_due_leads<B, S, C, K>(
        &self,
        builder: &B,
        cipher: &S,
        cache: &C,
        settings: EvaluationSettings,
        clock: &K,
    ) -> Result<EvaluationReport, CoreError>
    where
        B: ProviderBuilder,
        S: SecretCipher,
        C: LlmCache,
        K: Clock,
    {
        let evaluator = Evaluator {
            builder,
            cipher,
            cache,
            settings,
        };
        let profiles: Vec<ActiveProfile> = self
            .refreshed_active_profiles()
            .await?
            .into_iter()
            .filter(|active| {
                active
                    .revision
                    .definition
                    .definition()
                    .qualification_agents
                    .iter()
                    .any(|agent| agent.enabled)
            })
            .collect();
        let mut report = EvaluationReport::default();
        let budget = &RunBudget::new(settings.budget_cents);
        let providers = &Providers::new();
        let evaluator = &evaluator;
        let mut remaining = settings.batch_size;
        let rescoring = self
            .rescore_stale_leads(RESCORE_BATCH_SIZE, clock.now())
            .await?;
        report.rescored = rescoring.leads;
        for active in &profiles {
            if remaining == 0 || report.budget_exhausted {
                break;
            }
            let leads = self
                .due_leads(active, remaining, self.due_gates(), clock.now())
                .await?;
            let works = &qualification_works(self, active);
            let halted = &AtomicBool::new(false);
            let leads = &leads;
            let mut steps = stream::iter(0..leads.len())
                .map(move |index| async move {
                    if let Some(step) = lead_step(halted, works) {
                        return Ok(step);
                    }
                    let lead = &leads[index];
                    evaluator
                        .evaluate_lead(self, active, lead, providers, budget, clock)
                        .await
                        .map(LeadStep::Ran)
                })
                .buffered(settings.concurrency.get());
            while let Some(step) = steps.next().await {
                match step? {
                    LeadStep::Skipped => {}
                    LeadStep::Halted(reason) => {
                        if !halted.swap(true, Ordering::AcqRel) {
                            info!(profile = %active.profile.id, reason = reason.as_str(), "qualification pass stopped for the profile");
                        }
                    }
                    LeadStep::Ran(LeadOutcome::Stopped) => {
                        report.stopped += 1;
                        halted.store(true, Ordering::Release);
                    }
                    LeadStep::Ran(LeadOutcome::Done { valid, invalid }) => {
                        report.leads += 1;
                        report.valid += valid;
                        report.invalid += invalid;
                        remaining = remaining.saturating_sub(1);
                    }
                    LeadStep::Ran(LeadOutcome::Deferred) => {
                        report.deferred += 1;
                        remaining = remaining.saturating_sub(1);
                    }
                    LeadStep::Ran(LeadOutcome::BudgetExhausted) => {
                        if !report.budget_exhausted {
                            warn!("qualification stopped, the run budget is exhausted");
                        }
                        report.budget_exhausted = true;
                        halted.store(true, Ordering::Release);
                    }
                }
            }
        }
        report.backlog = self.due_backlog(&profiles, clock.now()).await? + rescoring.backlog;
        Ok(report)
    }

    pub(crate) fn due_gates(&self) -> DueGates {
        DueGates {
            requalify_max_per_day: self.requalify.max_per_day,
        }
    }

    async fn due_backlog(
        &self,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut backlog = 0;
        for active in profiles {
            backlog += self.count_due_leads(active, self.due_gates(), now).await?;
        }
        Ok(backlog)
    }

    #[transactional(qualification)]
    async fn due_leads(
        &self,
        active: &ActiveProfile,
        limit: u32,
        gates: DueGates,
        now: DateTime<Utc>,
    ) -> Result<Vec<Lead>, CoreError> {
        let mut repo = qualification_repository;
        repo.due_leads(active.profile.id, active.revision.number, limit, gates, now)
            .await
    }

    #[transactional(qualification)]
    async fn count_due_leads(
        &self,
        active: &ActiveProfile,
        gates: DueGates,
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut repo = qualification_repository;
        repo.count_due(active.profile.id, active.revision.number, gates, now)
            .await
    }

    #[transactional(qualification)]
    pub(crate) async fn lead_fresh_calls(&self, lead_id: LeadId) -> Result<bool, CoreError> {
        let mut repo = qualification_repository;
        repo.fresh_calls(lead_id).await
    }

    #[transactional(qualification)]
    pub async fn backfill_dossier_requalifications(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut repo = qualification_repository;
        repo.backfill_dossier_requalifications(self.requalify.requalify_after(now), now)
            .await
    }

    #[transactional(qualification, timeline)]
    pub(crate) async fn note_agent_failure(
        &self,
        failure: &AgentCallFailure,
        at: DateTime<Utc>,
    ) -> Result<u32, CoreError> {
        let mut repo = qualification_repository;
        let mut timeline = timeline_repository;
        let attempts = repo.record_agent_failure(failure, at).await?;
        let step = match failure.kind {
            AgentKind::Qualification => TimelineStep::Qualification,
            AgentKind::Scoring => TimelineStep::Scoring,
        };
        timeline
            .record(
                failure.lead_id,
                &call_failed(
                    step,
                    &failure.agent_id,
                    attempts,
                    at + agent_retry_delay(attempts),
                    &failure.error,
                    at,
                ),
            )
            .await?;
        Ok(attempts)
    }

    #[transactional(qualification)]
    pub async fn lead_agent_failures(
        &self,
        lead_id: LeadId,
    ) -> Result<Vec<AgentFailure>, CoreError> {
        let mut repo = qualification_repository;
        repo.agent_failures(lead_id).await
    }

    #[transactional(qualification, agent_work, timeline)]
    async fn store_evaluation(
        &self,
        results: &[QualificationResult],
        completion: &Completion,
        entries: &[TimelineEntry],
    ) -> Result<(), CoreError> {
        let mut repo = qualification_repository;
        let mut stops = agent_work_repository;
        let mut timeline = timeline_repository;
        for result in results {
            repo.store_result(result).await?;
        }
        for entry in entries {
            timeline.record(completion.lead_id, entry).await?;
        }
        stops
            .clear_stops(
                completion.lead_id,
                &[
                    WorkKind::Qualification,
                    WorkKind::Scoring,
                    WorkKind::Aggregator,
                ],
            )
            .await?;
        repo.complete(completion).await
    }

    #[transactional(qualification)]
    pub async fn lead_qualification_results(
        &self,
        lead_id: LeadId,
    ) -> Result<Vec<QualificationResult>, CoreError> {
        let mut repo = qualification_repository;
        repo.current_results(lead_id).await
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;
    use crate::domain::{
        lead::SignalId,
        prompt::{EvidenceQuote, check_evidence},
        values::{Provenance, SignalKind, SourceName, UnitInterval},
    };

    fn signal(value: Option<&str>) -> Signal {
        Signal {
            id: SignalId(uuid::Uuid::nil()),
            lead_id: LeadId(uuid::Uuid::nil()),
            kind: SignalKind::parse("product_detected").unwrap(),
            value: value.map(str::to_owned),
            source: SourceName::HttpProbe,
            raw: json!({"version": "21.1"}),
            provenance: Provenance::parse("https://example.com/realms/master").unwrap(),
            strength: UnitInterval::new(0.5).unwrap(),
            observed_at: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
            agent: None,
        }
    }

    #[test]
    fn rendering_is_a_sentence_then_source_and_date() {
        assert_eq!(
            render_signal(&signal(Some("keycloak"))),
            "example.com expose Keycloak (sonde HTTP, version 21.1)\nsource: http_probe\nobserved: 2027-01-15T08:00:00Z"
        );
    }

    #[test]
    fn a_missing_value_falls_back_to_the_kind() {
        assert!(render_signal(&signal(None)).starts_with("product_detected\nsource: http_probe"));
    }

    #[test]
    fn a_quote_from_the_rendered_sentence_passes_the_evidence_check() {
        let content = render_signal(&signal(Some("keycloak")));
        let documents = [UntrustedDocument {
            source: "https://example.com/realms/master",
            content: &content,
        }];
        let quotes = [
            EvidenceQuote {
                document_index: 0,
                quote: "expose Keycloak (sonde HTTP, version 21.1)".to_owned(),
            },
            EvidenceQuote {
                document_index: 0,
                quote: "sso.example.com expose Keycloak".to_owned(),
            },
        ];
        let check = check_evidence(&quotes, &documents);
        assert_eq!(check.verified.len(), 1);
        assert_eq!(check.rejected, 1);
    }

    #[test]
    fn the_raw_json_is_not_given_to_the_model() {
        assert!(!render_signal(&signal(Some("keycloak"))).contains('{'));
    }
}
