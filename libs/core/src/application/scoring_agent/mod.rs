use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use futures_util::{StreamExt, stream};
use lodestone_macros::transactional;
use serde_json::json;
use tracing::{info, warn};

use crate::{
    application::{
        LodestoneUseCase,
        evaluation::{
            CALL_FAILURE_PREFIX, CallFailure, EvaluationSettings, LeadMaterial, LeadStep, Loaded,
            ProviderBuilder, Providers, count, lead_step, load_cached,
        },
        llm::gateway::{
            CallSettings, LlmCallError, Prompt, RunBudget, SchemaCall, Structured,
            call_with_schema_until,
        },
        rescoring::stored_opinions,
        timeline::{
            AgentTrace, BUDGET_EXHAUSTED, agent_entries, agent_interrupted, agent_span,
            agent_started, combined, in_flight::Subject as Running, round_key,
        },
    },
    domain::{
        agent_work::{AgentWork, CancelReason, WorkKind, WorkTarget, ports::AgentWorkRepository},
        job::ports::Clock,
        lead::{Lead, LeadId},
        llm::{
            RunProvenance, UntrustedDocument,
            cache::{CacheScope, LlmCache},
            provenance::DEFAULT_SAMPLES,
            provider::{ProviderId, SecretCipher, Tier},
        },
        profile::{
            ActiveProfile,
            contract::scoring_contract_schema,
            definition::{ProfileDefinition, QualificationAgent, ScoringAgent},
        },
        qualification::{
            AgentKind, EvidenceStrength, Exclusion, Judgement, QualificationResult, ResultStatus,
            Verdict, exclusions, judge,
            prompt::{AgentPrompt, with_ungrounded},
        },
        scoring_agent::{
            ScoringOutput, ScoringResult, ScoringResultId, ScoringViolations, decide,
            ports::{ScoringCompletion, ScoringRepository},
            prompt::{
                INDICATORS_SOURCE, build, render_indicators, render_report, render_report_evidence,
                report_source, with_violations,
            },
            scoring_sample_record, validate_scoring, vote_scoring,
        },
        timeline::{
            EventActor, EventStatus, TimelineEntry, TimelineStep, ports::TimelineRepository,
        },
    },
};

mod trial;

pub(crate) use trial::{Scored, Scoring, score_in_memory};

const PROVIDER_UNAVAILABLE: &str = "fournisseur indisponible";
const NO_REPORT: &str = "aucun rapport de qualification valide";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScoringReport {
    pub leads: u64,
    pub valid: u64,
    pub invalid: u64,
    pub deferred: u64,
    pub stopped: u64,
    pub backlog: u64,
    pub budget_exhausted: bool,
}

enum SampleOutcome {
    Valid {
        output: ScoringOutput,
        model: String,
    },
    Invalid {
        error: String,
    },
    CallFailed {
        error: String,
    },
    BudgetExhausted,
    Stopped(CancelReason),
}

enum AgentOutcome {
    Result(Box<ScoringResult>),
    CallFailed(CallFailure),
    BudgetExhausted,
    Stopped(CancelReason),
}

enum LeadOutcome {
    Done { valid: u64, invalid: u64 },
    Deferred,
    BudgetExhausted,
    Stopped,
}

enum AgentStep {
    Unreported(Box<ScoringResult>),
    Ran(AgentOutcome),
}

struct Subject<'a> {
    active: &'a ActiveProfile,
    lead: &'a Lead,
    documents: &'a [UntrustedDocument<'a>],
    grounding: &'a [UntrustedDocument<'a>],
    fresh: bool,
    at: DateTime<Utc>,
}

struct AgentRun<'a, P> {
    loaded: &'a Loaded<P>,
    subject: &'a Subject<'a>,
    agent: &'a ScoringAgent,
    prompt: &'a AgentPrompt,
    work: &'a AgentWork,
}

fn scoring_works(usecase: &LodestoneUseCase, active: &ActiveProfile) -> Vec<AgentWork> {
    let mut works: Vec<AgentWork> = active
        .revision
        .definition
        .definition()
        .enabled_scoring_agents()
        .map(|agent| usecase.agent_work(active, WorkTarget::new(WorkKind::Scoring, &agent.id)))
        .collect();
    works.push(usecase.agent_work(active, WorkTarget::aggregator()));
    works
}

struct ScoringInputs {
    reports: Vec<(String, String)>,
    indicators: String,
    evidence_texts: Vec<(String, String)>,
}

fn scoring_inputs(
    definition: &ProfileDefinition,
    qualification: &[QualificationResult],
    judgement: &Judgement,
    found: &[Exclusion],
) -> ScoringInputs {
    let enabled = || {
        definition
            .qualification_agents
            .iter()
            .filter(|agent| agent.enabled)
    };
    let result_of = |agent: &QualificationAgent| {
        qualification
            .iter()
            .find(|result| result.agent_id == agent.id)
    };
    ScoringInputs {
        reports: enabled()
            .map(|agent| {
                (
                    report_source(agent),
                    render_report(agent, result_of(agent), found),
                )
            })
            .collect(),
        indicators: render_indicators(judgement.opinion, &judgement.evidence, judgement.confidence),
        evidence_texts: enabled()
            .map(|agent| {
                (
                    report_source(agent),
                    render_report_evidence(agent, result_of(agent)),
                )
            })
            .collect(),
    }
}

type ScoringDocuments<'a> = (Vec<UntrustedDocument<'a>>, Vec<UntrustedDocument<'a>>);

impl ScoringInputs {
    fn documents<'a>(&'a self, base: &[UntrustedDocument<'a>]) -> ScoringDocuments<'a> {
        let mut documents = Vec::with_capacity(base.len() + self.reports.len() + 1);
        documents.extend_from_slice(base);
        documents.extend(
            self.reports
                .iter()
                .map(|(source, content)| UntrustedDocument { source, content }),
        );
        documents.push(UntrustedDocument {
            source: INDICATORS_SOURCE,
            content: &self.indicators,
        });
        let mut grounding = base.to_vec();
        grounding.extend(
            self.evidence_texts
                .iter()
                .map(|(source, content)| UntrustedDocument { source, content }),
        );
        (documents, grounding)
    }
}

struct Scorer<'a, B, S, C> {
    builder: &'a B,
    cipher: &'a S,
    cache: &'a C,
    settings: EvaluationSettings,
}

pub(crate) fn scoring_prompt_version(prompt: &AgentPrompt) -> String {
    format!("scoring-agent@{}", prompt.hash)
}

fn failed(
    subject: &Subject<'_>,
    agent: &ScoringAgent,
    provenance: RunProvenance,
    error: String,
    raw_samples: serde_json::Value,
) -> ScoringResult {
    ScoringResult {
        id: ScoringResultId(generate_uuid_v7()),
        lead_id: subject.lead.id,
        profile_id: subject.active.profile.id,
        profile_revision: subject.active.revision.number,
        agent_id: agent.id.clone(),
        provenance,
        status: ResultStatus::Invalid,
        criteria: json!({}),
        score: None,
        verdict: None,
        reasons: Vec::new(),
        error: Some(error),
        raw_samples,
        created_at: subject.at,
    }
}

fn succeeded(
    subject: &Subject<'_>,
    agent: &ScoringAgent,
    provenance: RunProvenance,
    output: ScoringOutput,
    raw_samples: serde_json::Value,
) -> ScoringResult {
    ScoringResult {
        id: ScoringResultId(generate_uuid_v7()),
        lead_id: subject.lead.id,
        profile_id: subject.active.profile.id,
        profile_revision: subject.active.revision.number,
        agent_id: agent.id.clone(),
        provenance,
        status: ResultStatus::Valid,
        criteria: output.criteria.to_json(),
        score: Some(output.score),
        verdict: Some(output.verdict),
        reasons: output.reasons,
        error: None,
        raw_samples,
        created_at: subject.at,
    }
}

fn sampled_temperature(agent: &ScoringAgent) -> Option<f64> {
    (agent.samples > 1).then_some(agent.sampling_temperature)
}

impl<B, S, C> Scorer<'_, B, S, C>
where
    B: ProviderBuilder,
    S: SecretCipher,
    C: LlmCache,
{
    async fn ask(
        &self,
        run: &AgentRun<'_, B::Provider>,
        instruction: &str,
        sample: u32,
        budget: &RunBudget,
    ) -> Result<Structured<serde_json::Value>, LlmCallError> {
        let AgentRun {
            loaded,
            subject,
            agent,
            prompt,
            work,
        } = *run;
        let schema = scoring_contract_schema(agent);
        let scope = CacheScope {
            profile_id: subject.active.profile.id,
            revision: subject.active.revision.number,
            prompt: format!("scoring@{}", prompt.hash),
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
                temperature: sampled_temperature(agent),
            },
            schema: &schema,
        };
        call_with_schema_until(
            &loaded.provider,
            self.cache,
            call,
            |output| validate_scoring(agent, output, subject.grounding).is_ok(),
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
        sample: u32,
        budget: &RunBudget,
    ) -> SampleOutcome {
        let AgentRun {
            subject,
            agent,
            prompt,
            ..
        } = *run;
        let mut instruction = prompt.instruction.clone();
        let mut last: Option<ScoringViolations> = None;
        let mut fallback: Option<(ScoringOutput, String)> = None;
        for attempt in 0..2 {
            let structured = match self.ask(run, &instruction, sample, budget).await {
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
                        "scoring call failed"
                    );
                    let error = format!("{CALL_FAILURE_PREFIX} : {error}");
                    return if call_failure {
                        SampleOutcome::CallFailed { error }
                    } else {
                        SampleOutcome::Invalid { error }
                    };
                }
            };
            match validate_scoring(agent, &structured.value, subject.grounding) {
                Ok(output) => {
                    let ungrounded = output.criteria.ungrounded();
                    if attempt == 0 && !ungrounded.is_empty() {
                        warn!(
                            agent = agent.id.as_str(),
                            criteria = ungrounded.len(),
                            "scoring criteria without a verifiable quote, retrying"
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
                    warn!(agent = agent.id.as_str(), %violations, "scoring answer broke the contract");
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

    async fn score_agent(
        &self,
        usecase: &LodestoneUseCase,
        subject: &Subject<'_>,
        agent: &ScoringAgent,
        work: &AgentWork,
        providers: &Providers<B::Provider>,
        budget: &RunBudget,
    ) -> Result<AgentOutcome, CoreError> {
        let definition = subject.active.revision.definition.definition();
        let schema = scoring_contract_schema(agent);
        let prompt = build(definition, agent, &schema);
        let version = scoring_prompt_version(&prompt);
        let wanted = agent.provider_id.map(ProviderId);
        let Some(loaded) = load_cached(
            usecase,
            self.builder,
            self.cipher,
            wanted,
            Tier::Qualification,
            agent.id.as_str(),
            providers,
        )
        .await?
        else {
            let result = failed(
                subject,
                agent,
                RunProvenance::unresolved(wanted, version),
                PROVIDER_UNAVAILABLE.to_owned(),
                json!([]),
            );
            return Ok(AgentOutcome::Result(Box::new(result)));
        };
        let loaded = loaded.as_ref();
        let run = &AgentRun {
            loaded,
            subject,
            agent,
            prompt: &prompt,
            work,
        };
        let samples = u32::from(agent.samples).max(1);
        let mut valid: Vec<ScoringOutput> = Vec::new();
        let mut records: Vec<serde_json::Value> = Vec::new();
        let mut model: Option<String> = None;
        let mut first_error: Option<String> = None;
        let mut call_errors: Vec<String> = Vec::new();
        let mut outcomes = stream::iter(0..samples)
            .map(move |sample| async move {
                let outcome = match work.stop_reason() {
                    Some(reason) => SampleOutcome::Stopped(reason),
                    None => self.run_sample(run, sample, budget).await,
                };
                (sample, outcome)
            })
            .buffered(self.settings.concurrency.get());
        while let Some((sample, outcome)) = outcomes.next().await {
            match outcome {
                SampleOutcome::BudgetExhausted => return Ok(AgentOutcome::BudgetExhausted),
                SampleOutcome::Stopped(reason) => return Ok(AgentOutcome::Stopped(reason)),
                SampleOutcome::CallFailed { error } => {
                    warn!(
                        agent = agent.id.as_str(),
                        sample, "scoring sample dropped, its call failed"
                    );
                    records.push(scoring_sample_record(sample, Err(&error)));
                    call_errors.push(error);
                }
                SampleOutcome::Valid {
                    output,
                    model: answered,
                } => {
                    records.push(scoring_sample_record(sample, Ok(&output)));
                    model.get_or_insert(answered);
                    valid.push(output);
                }
                SampleOutcome::Invalid { error } => {
                    warn!(agent = agent.id.as_str(), sample, "scoring sample dropped");
                    records.push(scoring_sample_record(sample, Err(&error)));
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
        let raw_samples = if samples > 1 {
            serde_json::Value::Array(records)
        } else {
            json!([])
        };
        let valid_samples = u32::try_from(valid.len().max(1)).unwrap_or(samples);
        let Some(output) = vote_scoring(agent, &valid) else {
            let result = failed(
                subject,
                agent,
                loaded.sampled_provenance(
                    &loaded.model,
                    version,
                    self.settings.max_tokens,
                    sampled_temperature(agent),
                    DEFAULT_SAMPLES,
                ),
                first_error.unwrap_or_default(),
                raw_samples,
            );
            return Ok(AgentOutcome::Result(Box::new(result)));
        };
        let recorded = if samples > 1 {
            valid_samples
        } else {
            DEFAULT_SAMPLES
        };
        let result = succeeded(
            subject,
            agent,
            loaded.sampled_provenance(
                model.as_deref().unwrap_or(&loaded.model),
                version,
                self.settings.max_tokens,
                sampled_temperature(agent),
                recorded,
            ),
            output,
            raw_samples,
        );
        Ok(AgentOutcome::Result(Box::new(result)))
    }

    async fn score_lead<K: Clock>(
        &self,
        usecase: &LodestoneUseCase,
        active: &ActiveProfile,
        lead: &Lead,
        providers: &Providers<B::Provider>,
        budget: &RunBudget,
        clock: &K,
    ) -> Result<LeadOutcome, CoreError> {
        let definition = active.revision.definition.definition();
        let Some(qualified_at) = lead.qualified_at else {
            return Ok(LeadOutcome::Done {
                valid: 0,
                invalid: 0,
            });
        };
        let at = clock.now();
        let qualification = usecase.lead_qualification_results(lead.id).await?;
        let material = LeadMaterial::load(usecase, lead).await?;
        let calibrations = usecase.profile_calibrations(active).await?;
        let (opinions, _) = stored_opinions(definition, &qualification, &calibrations);
        let judgement = judge(
            &opinions,
            EvidenceStrength::compute(&material.facts, &definition.evidence, at),
            definition.verdict,
            &definition.evidence,
        );
        let found = exclusions(&qualification, &definition.qualification_agents);
        let base = material.documents();
        let inputs = scoring_inputs(definition, &qualification, &judgement, &found);
        let (documents, grounding) = inputs.documents(&base);
        let subject = Subject {
            active,
            lead,
            documents: &documents,
            grounding: &grounding,
            fresh: usecase.lead_fresh_calls(lead.id).await?,
            at,
        };
        let reported = qualification
            .iter()
            .any(|result| result.status == ResultStatus::Valid);
        let mut results: Vec<ScoringResult> = Vec::new();
        let mut entries: Vec<TimelineEntry> = Vec::new();
        let round = &round_key(Some(qualified_at));
        let subject = &subject;
        let agents: &Vec<&ScoringAgent> = &definition.enabled_scoring_agents().collect();
        let mut steps = stream::iter(0..agents.len())
            .map(move |index| async move {
                let agent = agents[index];
                let work =
                    usecase.agent_work(active, WorkTarget::new(WorkKind::Scoring, &agent.id));
                let started = clock.now();
                let span = agent_span(TimelineStep::Scoring, agent.id.as_str(), round, started);
                let step = if let Some(reason) = work.stop_reason() {
                    AgentStep::Ran(AgentOutcome::Stopped(reason))
                } else if !reported {
                    AgentStep::Unreported(Box::new(failed(
                        subject,
                        agent,
                        RunProvenance::unresolved(agent.provider_id.map(ProviderId), String::new()),
                        NO_REPORT.to_owned(),
                        json!([]),
                    )))
                } else {
                    let _running = usecase.enter_step(
                        Running::Lead(lead.id),
                        TimelineStep::Scoring,
                        EventActor::agent(agent.id.as_str(), &agent.name),
                        started,
                    );
                    usecase
                        .record_timeline(
                            lead.id,
                            &agent_started(
                                TimelineStep::Scoring,
                                (agent.id.as_str(), &agent.name),
                                &span,
                                started,
                            ),
                        )
                        .await?;
                    AgentStep::Ran(
                        self.score_agent(usecase, subject, agent, &work, providers, budget)
                            .await?,
                    )
                };
                Ok::<_, CoreError>((agent, work, step, (span, started, clock.now())))
            })
            .buffered(self.settings.concurrency.get());
        while let Some(next) = steps.next().await {
            let (agent, work, step, (span, started, finished)) = next?;
            let trace = |result: &ScoringResult| {
                agent_entries(AgentTrace {
                    step: TimelineStep::Scoring,
                    agent: (agent.id.as_str(), &agent.name),
                    provenance: &result.provenance,
                    valid: result.status == ResultStatus::Valid,
                    error: result.error.as_deref(),
                    raw_samples: &result.raw_samples,
                    detail: json!({
                        "score": result.score,
                        "verdict": result.verdict.as_ref().map(|verdict| verdict.choice.as_str()),
                        "samples": result.provenance.samples,
                    }),
                    span: &span,
                    started_at: started,
                    finished_at: finished,
                })
            };
            let outcome = match step {
                AgentStep::Unreported(result) => {
                    entries.extend(trace(&result));
                    results.push(*result);
                    continue;
                }
                AgentStep::Ran(outcome) => outcome,
            };
            let interrupted = |ending: (EventStatus, Option<&str>)| {
                agent_interrupted(
                    TimelineStep::Scoring,
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
                            lead.id,
                            &interrupted((EventStatus::Skipped, Some(BUDGET_EXHAUSTED))),
                        )
                        .await?;
                    return Ok(LeadOutcome::BudgetExhausted);
                }
                AgentOutcome::Stopped(reason) => {
                    usecase
                        .record_timeline(
                            lead.id,
                            &interrupted((EventStatus::Skipped, Some(reason.as_str()))),
                        )
                        .await?;
                    usecase.note_stop(lead.id, &work, reason, at).await?;
                    return Ok(LeadOutcome::Stopped);
                }
                AgentOutcome::CallFailed(failure) => {
                    usecase
                        .record_timeline(
                            lead.id,
                            &interrupted((EventStatus::Failed, Some(&failure.error))),
                        )
                        .await?;
                    let record =
                        failure.into_record(lead.id, AgentKind::Scoring, active.revision.number);
                    let attempts = usecase.note_agent_failure(&record, clock.now()).await?;
                    warn!(
                        lead_id = %lead.id,
                        agent = record.agent_id,
                        attempts,
                        error = record.error,
                        "scoring deferred, a model call failed"
                    );
                    return Ok(LeadOutcome::Deferred);
                }
                AgentOutcome::Result(result) => {
                    info!(
                        monotonic_counter.scoring_results = 1u64,
                        agent = agent.id.as_str(),
                        status = result.status.as_str(),
                        "scoring result recorded"
                    );
                    entries.extend(trace(&result));
                    results.push(*result);
                }
            }
        }
        let valid = count(
            results
                .iter()
                .filter(|result| result.status == ResultStatus::Valid)
                .count(),
        );
        let invalid = count(results.len()) - valid;
        if let Some(work) = scoring_works(usecase, active)
            .into_iter()
            .find(|work| !work.is_live())
            && let Some(reason) = work.stop_reason()
        {
            usecase.note_stop(lead.id, &work, reason, at).await?;
            return Ok(LeadOutcome::Stopped);
        }
        let decision = decide(&definition.scoring_agents, &results, &calibrations, found);
        entries.push(combined(
            "scoring",
            round,
            clock.now(),
            json!({
                "source": "scoring",
                "score": decision.score,
                "verdict": decision.verdict.map(Verdict::as_str),
                "confidence": decision.confidence,
                "reason": decision.reason_text(),
                "contributors": decision.contributors,
            }),
        ));
        let completion = ScoringCompletion {
            lead_id: lead.id,
            profile_revision: active.revision.number,
            qualified_at,
            decision,
        };
        if !usecase
            .store_scoring(&results, &completion, &entries)
            .await?
        {
            warn!(lead_id = %lead.id, "scoring dropped, the lead was qualified again meanwhile");
        }
        Ok(LeadOutcome::Done { valid, invalid })
    }
}

impl LodestoneUseCase {
    pub async fn score_due_leads<B, S, C, K>(
        &self,
        builder: &B,
        cipher: &S,
        cache: &C,
        settings: EvaluationSettings,
        clock: &K,
    ) -> Result<ScoringReport, CoreError>
    where
        B: ProviderBuilder,
        S: SecretCipher,
        C: LlmCache,
        K: Clock,
    {
        let scorer = Scorer {
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
        let mut report = ScoringReport::default();
        let budget = &RunBudget::new(settings.budget_cents);
        let providers = &Providers::new();
        let scorer = &scorer;
        let mut remaining = settings.batch_size;
        for active in &profiles {
            if remaining == 0 || report.budget_exhausted {
                break;
            }
            let leads = self
                .due_scoring_leads(active, remaining, clock.now())
                .await?;
            let works = &scoring_works(self, active);
            let halted = &AtomicBool::new(false);
            let leads = &leads;
            let mut steps = stream::iter(0..leads.len())
                .map(move |index| async move {
                    if let Some(step) = lead_step(halted, works) {
                        return Ok(step);
                    }
                    let lead = &leads[index];
                    scorer
                        .score_lead(self, active, lead, providers, budget, clock)
                        .await
                        .map(LeadStep::Ran)
                })
                .buffered(settings.concurrency.get());
            while let Some(step) = steps.next().await {
                match step? {
                    LeadStep::Skipped => {}
                    LeadStep::Halted(reason) => {
                        if !halted.swap(true, Ordering::AcqRel) {
                            info!(profile = %active.profile.id, reason = reason.as_str(), "scoring pass stopped for the profile");
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
                            warn!("scoring stopped, the run budget is exhausted");
                        }
                        report.budget_exhausted = true;
                        halted.store(true, Ordering::Release);
                    }
                }
            }
        }
        for active in &profiles {
            report.backlog += self.count_due_scoring(active, clock.now()).await?;
        }
        Ok(report)
    }

    #[transactional(scoring_agent)]
    async fn due_scoring_leads(
        &self,
        active: &ActiveProfile,
        limit: u32,
        now: DateTime<Utc>,
    ) -> Result<Vec<Lead>, CoreError> {
        let mut repo = scoring_agent_repository;
        repo.due_leads(active.profile.id, active.revision.number, limit, now)
            .await
    }

    #[transactional(scoring_agent)]
    async fn count_due_scoring(
        &self,
        active: &ActiveProfile,
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut repo = scoring_agent_repository;
        repo.count_due(active.profile.id, active.revision.number, now)
            .await
    }

    #[transactional(scoring_agent, agent_work, timeline)]
    async fn store_scoring(
        &self,
        results: &[ScoringResult],
        completion: &ScoringCompletion,
        entries: &[TimelineEntry],
    ) -> Result<bool, CoreError> {
        let mut repo = scoring_agent_repository;
        let mut stops = agent_work_repository;
        let mut timeline = timeline_repository;
        for result in results {
            repo.store_result(result).await?;
        }
        stops
            .clear_stops(completion.lead_id, &[WorkKind::Scoring])
            .await?;
        let completed = repo.complete(completion).await?;
        if completed {
            for entry in entries {
                timeline.record(completion.lead_id, entry).await?;
            }
        }
        Ok(completed)
    }

    #[transactional(scoring_agent)]
    pub async fn lead_scoring_results(
        &self,
        lead_id: LeadId,
    ) -> Result<Vec<ScoringResult>, CoreError> {
        let mut repo = scoring_agent_repository;
        repo.current_results(lead_id).await
    }
}
