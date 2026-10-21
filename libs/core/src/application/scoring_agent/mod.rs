use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::transactional;
use tracing::{info, warn};

use crate::{
    application::{
        LodestoneUseCase,
        evaluation::{
            CALL_FAILURE_PREFIX, CallFailure, EvaluationSettings, Loaded, ProviderBuilder,
            Providers, count, load_cached, render_signal,
        },
        llm::gateway::{
            CallSettings, LlmCallError, Prompt, RunBudget, SchemaCall, Structured, call_with_schema,
        },
        rescoring::stored_opinions,
    },
    domain::{
        company::PAGE_PROMPT_CHARS,
        job::ports::Clock,
        lead::{Lead, LeadId},
        llm::{
            RunProvenance, UntrustedDocument,
            cache::{CacheScope, LlmCache},
            provenance::DEFAULT_SAMPLES,
            provider::{ProviderId, SecretCipher, Tier},
        },
        profile::{ActiveProfile, contract::scoring_contract_schema, definition::ScoringAgent},
        qualification::{
            AgentKind, EvidenceStrength, ResultStatus, exclusions, judge,
            prompt::{AgentPrompt, lead_document, with_ungrounded},
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
    },
};

const PROVIDER_UNAVAILABLE: &str = "fournisseur indisponible";
const NO_REPORT: &str = "aucun rapport de qualification valide";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScoringReport {
    pub leads: u64,
    pub valid: u64,
    pub invalid: u64,
    pub deferred: u64,
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
}

enum AgentOutcome {
    Result(Box<ScoringResult>),
    CallFailed(CallFailure),
    BudgetExhausted,
}

enum LeadOutcome {
    Done { valid: u64, invalid: u64 },
    Deferred,
    BudgetExhausted,
}

struct Subject<'a> {
    active: &'a ActiveProfile,
    lead: &'a Lead,
    documents: &'a [UntrustedDocument<'a>],
    grounding: &'a [UntrustedDocument<'a>],
    at: DateTime<Utc>,
}

struct AgentRun<'a, P> {
    loaded: &'a Loaded<P>,
    subject: &'a Subject<'a>,
    agent: &'a ScoringAgent,
    prompt: &'a AgentPrompt,
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
        criteria: serde_json::json!({}),
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
        budget: &mut RunBudget,
    ) -> Result<Structured<serde_json::Value>, LlmCallError> {
        let AgentRun {
            loaded,
            subject,
            agent,
            prompt,
        } = *run;
        let schema = scoring_contract_schema(agent);
        let scope = CacheScope {
            profile_id: subject.active.profile.id,
            revision: subject.active.revision.number,
            prompt: format!("scoring@{}", prompt.hash),
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
                temperature: sampled_temperature(agent),
            },
            schema: &schema,
        };
        call_with_schema(
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
        )
        .await
    }

    async fn run_sample(
        &self,
        run: &AgentRun<'_, B::Provider>,
        sample: u32,
        budget: &mut RunBudget,
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
        providers: &mut Providers<B::Provider>,
        budget: &mut RunBudget,
    ) -> Result<AgentOutcome, CoreError> {
        let definition = subject.active.revision.definition.definition();
        let schema = scoring_contract_schema(agent);
        let prompt = build(definition, agent, &schema);
        let version = scoring_prompt_version(&prompt);
        let wanted = agent.provider_id.map(ProviderId);
        let Some(index) = load_cached(
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
                serde_json::json!([]),
            );
            return Ok(AgentOutcome::Result(Box::new(result)));
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
        let samples = u32::from(agent.samples).max(1);
        let mut valid: Vec<ScoringOutput> = Vec::new();
        let mut records: Vec<serde_json::Value> = Vec::new();
        let mut model: Option<String> = None;
        let mut first_error: Option<String> = None;
        let mut call_errors: Vec<String> = Vec::new();
        for sample in 0..samples {
            match self.run_sample(&run, sample, budget).await {
                SampleOutcome::BudgetExhausted => return Ok(AgentOutcome::BudgetExhausted),
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
            serde_json::json!([])
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
        providers: &mut Providers<B::Provider>,
        budget: &mut RunBudget,
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
        let (signals, facts, dossier) = usecase.signals_with_facts(lead).await?;
        let calibrations = usecase.profile_calibrations(active).await?;
        let (opinions, _) = stored_opinions(definition, &qualification, &calibrations);
        let judgement = judge(
            &opinions,
            EvidenceStrength::compute(&facts, &definition.evidence, at),
            definition.verdict,
            &definition.evidence,
        );
        let found = exclusions(&qualification, &definition.qualification_agents);
        let overview = lead_document(lead.domain.as_str(), &facts);
        let contents: Vec<String> = signals.iter().map(render_signal).collect();
        let dossier_documents = dossier
            .as_ref()
            .map(|dossier| dossier.documents(PAGE_PROMPT_CHARS))
            .unwrap_or_default();
        let reports: Vec<(String, String)> = definition
            .qualification_agents
            .iter()
            .filter(|agent| agent.enabled)
            .map(|agent| {
                let result = qualification
                    .iter()
                    .find(|result| result.agent_id == agent.id);
                (report_source(agent), render_report(agent, result, &found))
            })
            .collect();
        let indicators =
            render_indicators(judgement.opinion, &judgement.evidence, judgement.confidence);
        let mut documents =
            Vec::with_capacity(signals.len() + dossier_documents.len() + reports.len() + 2);
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
        documents.extend(
            reports
                .iter()
                .map(|(source, content)| UntrustedDocument { source, content }),
        );
        documents.push(UntrustedDocument {
            source: INDICATORS_SOURCE,
            content: &indicators,
        });
        let evidence_texts: Vec<(String, String)> = definition
            .qualification_agents
            .iter()
            .filter(|agent| agent.enabled)
            .map(|agent| {
                let result = qualification
                    .iter()
                    .find(|result| result.agent_id == agent.id);
                (report_source(agent), render_report_evidence(agent, result))
            })
            .collect();
        let grounding_count = 1 + signals.len() + dossier_documents.len();
        let mut grounding: Vec<UntrustedDocument<'_>> = documents[..grounding_count].to_vec();
        grounding.extend(
            evidence_texts
                .iter()
                .map(|(source, content)| UntrustedDocument { source, content }),
        );
        let subject = Subject {
            active,
            lead,
            documents: &documents,
            grounding: &grounding,
            at,
        };
        let reported = qualification
            .iter()
            .any(|result| result.status == ResultStatus::Valid);
        let mut results: Vec<ScoringResult> = Vec::new();
        for agent in definition.enabled_scoring_agents() {
            if !reported {
                results.push(failed(
                    &subject,
                    agent,
                    RunProvenance::unresolved(agent.provider_id.map(ProviderId), String::new()),
                    NO_REPORT.to_owned(),
                    serde_json::json!([]),
                ));
                continue;
            }
            match self
                .score_agent(usecase, &subject, agent, providers, budget)
                .await?
            {
                AgentOutcome::BudgetExhausted => return Ok(LeadOutcome::BudgetExhausted),
                AgentOutcome::CallFailed(failure) => {
                    let record =
                        failure.into_record(lead.id, AgentKind::Scoring, active.revision.number);
                    let attempts = usecase.note_agent_failure(&record, at).await?;
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
        let decision = decide(&definition.scoring_agents, &results, &calibrations, found);
        let completion = ScoringCompletion {
            lead_id: lead.id,
            profile_revision: active.revision.number,
            qualified_at,
            decision,
        };
        if !usecase.store_scoring(&results, &completion).await? {
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
        let mut report = ScoringReport::default();
        let mut budget = RunBudget::new(settings.budget_cents);
        let mut providers = Providers::new();
        let mut remaining = settings.batch_size;
        'profiles: for active in &profiles {
            if remaining == 0 {
                break;
            }
            let leads = self
                .due_scoring_leads(active, remaining, clock.now())
                .await?;
            for lead in &leads {
                match scorer
                    .score_lead(self, active, lead, &mut providers, &mut budget, clock)
                    .await?
                {
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
                        warn!("scoring stopped, the run budget is exhausted");
                        break 'profiles;
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

    #[transactional(scoring_agent)]
    async fn store_scoring(
        &self,
        results: &[ScoringResult],
        completion: &ScoringCompletion,
    ) -> Result<bool, CoreError> {
        let mut repo = scoring_agent_repository;
        for result in results {
            repo.store_result(result).await?;
        }
        repo.complete(completion).await
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
