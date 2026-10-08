use std::{num::NonZeroU32, time::Duration};

use chrono::{DateTime, SecondsFormat, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::transactional;
use tracing::{info, warn};

use crate::{
    application::{
        LodestoneUseCase,
        llm::gateway::{
            CallSettings, LlmCallError, Prompt, RunBudget, SchemaCall, Structured, call_with_schema,
        },
    },
    domain::{
        calibration::{Calibrations, opinion_score},
        company::PAGE_PROMPT_CHARS,
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
            SCORING_VERSION, ValidOutput, Violations, exclusions, judge,
            ports::QualificationRepository,
            prompt::{AgentPrompt, build, lead_document, with_ungrounded, with_violations},
            sample_record, validate_answers, vote,
        },
    },
};

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
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EvaluationReport {
    pub leads: u64,
    pub valid: u64,
    pub invalid: u64,
    pub rescored: u64,
    pub deferred: u64,
    pub backlog: u64,
    pub budget_exhausted: bool,
}

pub(crate) struct Loaded<P> {
    pub(crate) provider: P,
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
                provider,
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

pub(crate) struct Providers<P> {
    pub(crate) entries: Vec<(Option<ProviderId>, Option<Loaded<P>>)>,
}

impl<P> Providers<P> {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn position(&self, wanted: Option<ProviderId>) -> Option<usize> {
        self.entries.iter().position(|(key, _)| *key == wanted)
    }
}

pub(crate) async fn load_cached<B, S>(
    usecase: &LodestoneUseCase,
    builder: &B,
    cipher: &S,
    wanted: Option<ProviderId>,
    tier: Tier,
    label: &str,
    providers: &mut Providers<B::Provider>,
) -> Result<Option<usize>, CoreError>
where
    B: ProviderBuilder,
    S: SecretCipher,
{
    if let Some(index) = providers.position(wanted) {
        return Ok(providers.entries[index].1.is_some().then_some(index));
    }
    let loaded = load_provider(usecase, builder, cipher, wanted, tier, label).await?;
    let available = loaded.is_some();
    providers.entries.push((wanted, loaded));
    Ok(available.then(|| providers.entries.len() - 1))
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
}

enum PassOutcome {
    Done(Pass),
    CallFailed(CallFailure),
    BudgetExhausted,
}

enum LeadOutcome {
    Done { valid: u64, invalid: u64 },
    Deferred,
    BudgetExhausted,
}

enum SampleOutcome {
    Valid { output: ValidOutput, model: String },
    Invalid { error: String },
    CallFailed { error: String },
    BudgetExhausted,
}

struct Pass {
    results: Vec<QualificationResult>,
    opinions: Vec<Option<AgentOpinion>>,
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
    at: DateTime<Utc>,
}

struct AgentRun<'a, P> {
    loaded: &'a Loaded<P>,
    subject: &'a Subject<'a>,
    agent: &'a QualificationAgent,
    prompt: &'a AgentPrompt,
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
        providers: &mut Providers<B::Provider>,
    ) -> Result<Option<usize>, CoreError> {
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
        budget: &mut RunBudget,
    ) -> Result<Structured<serde_json::Value>, LlmCallError> {
        let AgentRun {
            loaded,
            subject,
            agent,
            prompt,
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
        call_with_schema(
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
        )
        .await
    }

    async fn run_sample(
        &self,
        run: &AgentRun<'_, B::Provider>,
        call: (RunPlan, u32),
        budget: &mut RunBudget,
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
        plan: RunPlan,
        providers: &mut Providers<B::Provider>,
        budget: &mut RunBudget,
    ) -> Result<AgentOutcome, CoreError> {
        let schema = contract_schema(agent);
        let prompt = build(subject.definition, agent, &schema);
        let version = agent_prompt_version(&prompt);
        let Some(index) = self
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
        let Some(loaded) = providers.entries[index].1.as_ref() else {
            return Err(CoreError::Internal(
                "provider cache is inconsistent".to_owned(),
            ));
        };
        let run = AgentRun {
            loaded,
            subject,
            agent,
            prompt: &prompt,
        };
        let mut valid: Vec<ValidOutput> = Vec::new();
        let mut records: Vec<serde_json::Value> = Vec::new();
        let mut model: Option<String> = None;
        let mut first_error: Option<String> = None;
        let mut call_errors: Vec<String> = Vec::new();
        for sample in 0..plan.samples {
            match self.run_sample(&run, (plan, sample), budget).await {
                SampleOutcome::BudgetExhausted => return Ok(AgentOutcome::BudgetExhausted),
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

    async fn run_agents(
        &self,
        usecase: &LodestoneUseCase,
        subject: &Subject<'_>,
        escalation: Option<ProviderId>,
        providers: &mut Providers<B::Provider>,
        budget: &mut RunBudget,
    ) -> Result<PassOutcome, CoreError> {
        let mut pass = Pass {
            results: Vec::new(),
            opinions: Vec::new(),
        };
        for agent in subject
            .definition
            .qualification_agents
            .iter()
            .filter(|agent| agent.enabled)
        {
            let plan = escalation.map_or_else(|| RunPlan::primary(agent), RunPlan::escalation);
            match self
                .evaluate_agent(usecase, subject, agent, plan, providers, budget)
                .await?
            {
                AgentOutcome::BudgetExhausted => return Ok(PassOutcome::BudgetExhausted),
                AgentOutcome::CallFailed(failure) => return Ok(PassOutcome::CallFailed(failure)),
                AgentOutcome::Result(result, opinion) => {
                    info!(
                        monotonic_counter.qualification_results = 1u64,
                        agent = agent.id.as_str(),
                        status = result.status.as_str(),
                        escalated = plan.escalated,
                        "qualification result recorded"
                    );
                    pass.results.push(*result);
                    pass.opinions.push(opinion);
                }
            }
        }
        Ok(PassOutcome::Done(pass))
    }

    async fn escalate(
        &self,
        usecase: &LodestoneUseCase,
        subject: &Subject<'_>,
        provider: ProviderId,
        pass: &mut Pass,
        providers: &mut Providers<B::Provider>,
        budget: &mut RunBudget,
    ) -> Result<Option<bool>, CoreError> {
        if self
            .load(usecase, Some(provider), ESCALATION_LABEL, providers)
            .await?
            .is_none()
        {
            warn!(lead_id = %subject.lead.id, "escalation skipped, its provider is unavailable");
            return Ok(Some(false));
        }
        let escalated = match self
            .run_agents(usecase, subject, Some(provider), providers, budget)
            .await?
        {
            PassOutcome::Done(escalated) => escalated,
            PassOutcome::BudgetExhausted => return Ok(None),
            PassOutcome::CallFailed(failure) => {
                warn!(
                    lead_id = %subject.lead.id,
                    agent = failure.agent_id,
                    error = failure.error,
                    "escalation skipped, its model call failed"
                );
                return Ok(Some(false));
            }
        };
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
        Ok(Some(replaced))
    }

    async fn evaluate_lead<K: Clock>(
        &self,
        usecase: &LodestoneUseCase,
        active: &ActiveProfile,
        lead: &Lead,
        providers: &mut Providers<B::Provider>,
        budget: &mut RunBudget,
        clock: &K,
    ) -> Result<LeadOutcome, CoreError> {
        let definition = active.revision.definition.definition();
        let calibrations = usecase.profile_calibrations(active).await?;
        let (signals, facts, dossier) = usecase.signals_with_facts(lead).await?;
        let overview = lead_document(lead.domain.as_str(), &facts);
        let contents: Vec<String> = signals.iter().map(render_signal).collect();
        let dossier_documents = dossier
            .as_ref()
            .map(|dossier| dossier.documents(PAGE_PROMPT_CHARS))
            .unwrap_or_default();
        let mut documents = Vec::with_capacity(signals.len() + dossier_documents.len() + 1);
        documents.push(UntrustedDocument {
            source: "lead",
            content: &overview,
        });
        documents.extend(signals.iter().zip(&contents).map(|(signal, content)| {
            UntrustedDocument {
                source: signal.provenance.as_str(),
                content,
            }
        }));
        documents.extend(dossier_documents.iter().map(|document| UntrustedDocument {
            source: &document.source,
            content: &document.content,
        }));
        let subject = Subject {
            active,
            definition,
            calibrations: &calibrations,
            lead,
            documents: &documents,
            at: clock.now(),
        };
        let at = subject.at;
        let revision = active.revision.number;
        let mut pass = match self
            .run_agents(usecase, &subject, None, providers, budget)
            .await?
        {
            PassOutcome::Done(pass) => pass,
            PassOutcome::BudgetExhausted => return Ok(LeadOutcome::BudgetExhausted),
            PassOutcome::CallFailed(failure) => {
                let record = failure.into_record(lead.id, AgentKind::Qualification, revision);
                let attempts = usecase.note_agent_failure(&record, at).await?;
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
        let mut judgement = judgement_of(definition, &facts, &scored(&pass), at);
        if escalated_revision.is_none()
            && let (Some(provider), Some(confidence)) =
                (definition.escalation.provider, judgement.confidence)
            && definition.escalation.triggers(confidence)
        {
            info!(lead_id = %lead.id, confidence, "grey-zone lead, escalating");
            match self
                .escalate(
                    usecase,
                    &subject,
                    ProviderId(provider),
                    &mut pass,
                    providers,
                    budget,
                )
                .await?
            {
                None => return Ok(LeadOutcome::BudgetExhausted),
                Some(true) => {
                    escalated_revision = Some(revision);
                    judgement = judgement_of(definition, &facts, &scored(&pass), at);
                    info!(
                        monotonic_counter.qualification_escalations = 1u64,
                        "lead escalated"
                    );
                }
                Some(false) => {}
            }
        }
        let valid = count(pass.opinions.iter().flatten().count());
        let invalid = count(pass.results.len()) - valid;
        let mut sources: Vec<String> = signals
            .iter()
            .map(|signal| signal.source.as_str().to_owned())
            .collect();
        sources.sort();
        sources.dedup();
        let completion = Completion {
            lead_id: lead.id,
            profile_revision: revision,
            sources,
            judgement,
            exclusions: exclusions(&pass.results, &definition.qualification_agents),
            scoring_version: SCORING_VERSION,
            dossier_at: dossier.as_ref().and_then(|dossier| dossier.content_at),
            escalated_revision,
            at,
        };
        usecase.store_evaluation(&pass.results, &completion).await?;
        Ok(LeadOutcome::Done { valid, invalid })
    }
}

fn wait_gate(wait: Duration, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if wait.is_zero() {
        return None;
    }
    chrono::Duration::from_std(wait)
        .ok()
        .and_then(|wait| now.checked_sub_signed(wait))
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
            .list_active_profiles()
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
        let mut budget = RunBudget::new(settings.budget_cents);
        let mut providers = Providers::new();
        let mut remaining = settings.batch_size;
        let now = clock.now();
        let rescoring = self
            .rescore_stale_leads(RESCORE_BATCH_SIZE, clock.now())
            .await?;
        report.rescored = rescoring.leads;
        'profiles: for active in &profiles {
            if remaining == 0 {
                break;
            }
            let gates = self.due_gates(active, now);
            let leads = self
                .due_leads(active, remaining, gates, clock.now())
                .await?;
            for lead in &leads {
                let outcome = evaluator
                    .evaluate_lead(self, active, lead, &mut providers, &mut budget, clock)
                    .await?;
                match outcome {
                    LeadOutcome::Done { valid, invalid } => {
                        report.leads += 1;
                        report.valid += valid;
                        report.invalid += invalid;
                        remaining = remaining.saturating_sub(1);
                    }
                    LeadOutcome::Deferred => {
                        report.deferred += 1;
                        remaining = remaining.saturating_sub(1);
                    }
                    LeadOutcome::BudgetExhausted => {
                        report.budget_exhausted = true;
                        warn!("qualification stopped, the run budget is exhausted");
                        break 'profiles;
                    }
                }
            }
        }
        report.backlog = self.due_backlog(&profiles, now, clock.now()).await? + rescoring.backlog;
        Ok(report)
    }

    pub(crate) fn due_gates(&self, active: &ActiveProfile, now: DateTime<Utc>) -> DueGates {
        let enrichment = &active.revision.definition.definition().enrichment;
        DueGates {
            dossier: wait_gate(self.company_wait, now),
            enrichment: wait_gate(self.enrichment_wait, now).filter(|_| enrichment.any_enabled()),
            http_probe: enrichment.http_probe.enabled,
            company_jobs: enrichment.company_jobs.enabled,
        }
    }

    async fn due_backlog(
        &self,
        profiles: &[ActiveProfile],
        gated_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut backlog = 0;
        for active in profiles {
            let gates = self.due_gates(active, gated_at);
            backlog += self.count_due_leads(active, gates, now).await?;
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
    pub(crate) async fn note_agent_failure(
        &self,
        failure: &AgentCallFailure,
        at: DateTime<Utc>,
    ) -> Result<u32, CoreError> {
        let mut repo = qualification_repository;
        repo.record_agent_failure(failure, at).await
    }

    #[transactional(qualification)]
    pub async fn lead_agent_failures(
        &self,
        lead_id: LeadId,
    ) -> Result<Vec<AgentFailure>, CoreError> {
        let mut repo = qualification_repository;
        repo.agent_failures(lead_id).await
    }

    #[transactional(qualification)]
    async fn store_evaluation(
        &self,
        results: &[QualificationResult],
        completion: &Completion,
    ) -> Result<(), CoreError> {
        let mut repo = qualification_repository;
        for result in results {
            repo.store_result(result).await?;
        }
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
