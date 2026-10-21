use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::transactional;
use tracing::{info, warn};

use crate::{
    application::{
        LodestoneUseCase,
        evaluation::{EvaluationSettings, Loaded, ProviderBuilder, load_provider, render_signal},
        llm::gateway::{
            CallSettings, LlmCallError, Prompt, RunBudget, SchemaCall, call_with_schema,
        },
    },
    domain::{
        aggregation::{
            AggregatorOutput, EVIDENCE_RETRY, Facts, agent_source, build, check,
            instruction_with_dossier, missing_results, render_agent, render_scoring, retry_message,
            scoring_source, warnings, with_problem,
        },
        company::PAGE_PROMPT_CHARS,
        job::ports::Clock,
        lead::{
            Assessment, AssessmentId, Evidence, Lead, LeadStatus, QualityWarning, Signal,
            draft::{DraftReview, carry_draft},
            ports::{AssessmentRepository, LeadRepository},
        },
        llm::{
            LlmProvider, UntrustedDocument,
            cache::{CacheScope, LlmCache},
            provider::{ProviderId, SecretCipher, Tier},
        },
        profile::{ActiveProfile, definition::ProfileDefinition},
        prompt::check_evidence,
        qualification::{
            QualificationResult, ports::QualificationRepository, prompt::lead_document,
        },
        scoring_agent::ScoringResult,
        suppression::ports::SuppressionRepository,
        values::Provenance,
    },
};

const ATTEMPTS: usize = 2;
const CONTRACT_RETRY: &str = "Your previous answer broke the contract";
const REASON_NO_PROVIDER: &str = "fournisseur indisponible";
const REASON_CALL: &str = "échec de l'appel au modèle";
const REASON_CONTRACT: &str = "réponse non conforme au contrat";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AggregationReport {
    pub leads: u64,
    pub unverified: u64,
    pub failed: u64,
    pub backlog: u64,
    pub budget_exhausted: bool,
}

enum Outcome {
    Stored { verified: bool },
    Skipped,
    Failed(String),
    BudgetExhausted,
}

struct Candidate {
    output: AggregatorOutput,
    model: String,
}

fn agent_blocks(
    definition: &ProfileDefinition,
    results: &[QualificationResult],
) -> Vec<(String, String)> {
    definition
        .qualification_agents
        .iter()
        .filter(|agent| agent.enabled)
        .map(|agent| {
            let result = results.iter().find(|result| result.agent_id == agent.id);
            (agent_source(&agent.id), render_agent(agent, result))
        })
        .collect()
}

fn scoring_blocks(
    definition: &ProfileDefinition,
    results: &[ScoringResult],
) -> Vec<(String, String)> {
    definition
        .enabled_scoring_agents()
        .map(|agent| {
            let result = results.iter().find(|result| result.agent_id == agent.id);
            (scoring_source(&agent.id), render_scoring(agent, result))
        })
        .collect()
}

fn evidence_of(
    check: &crate::domain::prompt::EvidenceCheck<'_>,
    signals: &[Signal],
    quotable: &[UntrustedDocument<'_>],
) -> Vec<Evidence> {
    check
        .verified
        .iter()
        .filter_map(|quote| {
            let index = usize::try_from(quote.document_index).ok()?;
            let provenance = match signals.get(index) {
                Some(signal) => signal.provenance.clone(),
                None => Provenance::parse(quotable.get(index)?.source).ok()?,
            };
            Some(Evidence {
                quote: quote.quote.clone(),
                provenance,
            })
        })
        .collect()
}

struct Summariser<'a, P, C> {
    loaded: &'a Loaded<P>,
    cache: &'a C,
    settings: EvaluationSettings,
}

impl<P, C> Summariser<'_, P, C>
where
    P: LlmProvider,
    C: LlmCache,
{
    async fn summarise<K: Clock>(
        &self,
        usecase: &LodestoneUseCase,
        active: &ActiveProfile,
        lead: &Lead,
        budget: &mut RunBudget,
        clock: &K,
    ) -> Result<Outcome, CoreError> {
        let definition = active.revision.definition.definition();
        let results = usecase.lead_qualification_results(lead.id).await?;
        let missing = missing_results(definition, &results);
        if !missing.is_empty() {
            warn!(lead_id = %lead.id, missing = missing.len(), "aggregation skipped, agent results are missing");
            return Ok(Outcome::Skipped);
        }
        let (signals, facts, dossier) = usecase.signals_with_facts(lead).await?;
        if signals.is_empty() {
            return Ok(Outcome::Skipped);
        }
        let dossier_documents = dossier
            .as_ref()
            .map(|dossier| dossier.documents(PAGE_PROMPT_CHARS))
            .unwrap_or_default();
        let signal_contents: Vec<String> = signals.iter().map(render_signal).collect();
        let overview = lead_document(lead.domain.as_str(), &facts);
        let scoring_results = usecase.lead_scoring_results(lead.id).await?;
        let blocks = agent_blocks(definition, &results);
        let scoring_blocks = scoring_blocks(definition, &scoring_results);
        let mut documents: Vec<UntrustedDocument<'_>> = signals
            .iter()
            .zip(&signal_contents)
            .map(|(signal, content)| UntrustedDocument {
                source: signal.provenance.as_str(),
                content,
            })
            .collect();
        documents.extend(dossier_documents.iter().map(|document| UntrustedDocument {
            source: &document.source,
            content: &document.content,
        }));
        documents.push(UntrustedDocument {
            source: "lead",
            content: &overview,
        });
        documents.extend(
            blocks
                .iter()
                .chain(&scoring_blocks)
                .map(|(source, content)| UntrustedDocument { source, content }),
        );
        let evidence_documents = &documents[..signals.len() + dossier_documents.len()];
        let schema =
            AggregatorOutput::schema().map_err(|error| CoreError::Internal(error.to_string()))?;
        let prompt = build(definition, &definition.aggregator, &schema);
        let base = instruction_with_dossier(
            Facts {
                confidence: lead.confidence,
                verdict: lead.verdict,
                score: lead.llm_score,
            },
            signals.len(),
            dossier_documents.len(),
        );
        let scope = CacheScope {
            profile_id: active.profile.id,
            revision: active.revision.number,
            prompt: prompt.tag.clone(),
            domain: lead.domain.clone(),
            agent: None,
            sample: 0,
        };
        let mut current = base.clone();
        let mut candidate: Option<Candidate> = None;
        let mut reason = REASON_CALL.to_owned();
        for _ in 0..ATTEMPTS {
            let call = SchemaCall {
                scope: &scope,
                prompt: Prompt {
                    system: &prompt.system,
                    instruction: &current,
                    documents: &documents,
                    max_tokens: self.settings.max_tokens,
                    temperature: None,
                },
                schema: &schema,
            };
            let structured = match call_with_schema(
                &self.loaded.provider,
                self.cache,
                call,
                |value| {
                    AggregatorOutput::parse(value).is_ok_and(|output| {
                        check_evidence(&output.evidence, evidence_documents).is_sound()
                            && check(&output).is_empty()
                    })
                },
                CallSettings {
                    max_retries: self.loaded.max_retries,
                    pricing: self.loaded.pricing,
                    base_delay: self.settings.base_delay,
                },
                budget,
            )
            .await
            {
                Ok(structured) => structured,
                Err(LlmCallError::BudgetExhausted) => return Ok(Outcome::BudgetExhausted),
                Err(error) => {
                    warn!(
                        lead_id = %lead.id,
                        %error,
                        provider_message = error.provider_message(),
                        "aggregation call failed"
                    );
                    if let Some(message) = error.provider_message() {
                        reason = format!("{REASON_CALL} : {message}");
                    }
                    break;
                }
            };
            match AggregatorOutput::parse(&structured.value) {
                Err(problem) => {
                    reason = REASON_CONTRACT.to_owned();
                    warn!(lead_id = %lead.id, %problem, "aggregator answer broke the contract");
                    current = with_problem(
                        &base,
                        &format!(
                            "{CONTRACT_RETRY}: {problem}. Answer again with only the JSON object that follows the contract."
                        ),
                    );
                }
                Ok(output) => {
                    let sound = check_evidence(&output.evidence, evidence_documents).is_sound();
                    let violations = check(&output);
                    let mut problems: Vec<String> = Vec::new();
                    if !sound {
                        warn!(lead_id = %lead.id, "aggregator evidence is not verbatim, retrying");
                        problems.push(EVIDENCE_RETRY.to_owned());
                    }
                    if !violations.is_empty() {
                        warn!(lead_id = %lead.id, violations = violations.len(), "aggregator answer broke the output rules, retrying");
                        problems.push(retry_message(&violations));
                    }
                    candidate = Some(Candidate {
                        output,
                        model: structured.model,
                    });
                    if problems.is_empty() {
                        break;
                    }
                    current = with_problem(&base, &problems.join("\n\n"));
                }
            }
        }
        let Some(Candidate { output, model }) = candidate else {
            return Ok(Outcome::Failed(reason));
        };
        let checked = check_evidence(&output.evidence, evidence_documents);
        let verified = !checked.verified.is_empty();
        let mut quality_warnings = warnings(&check(&output));
        if verified && checked.rejected > 0 {
            quality_warnings.push(QualityWarning::DroppedQuotes);
        }
        let evidence = evidence_of(&checked, &signals, evidence_documents);
        let now = clock.now();
        let assessment = Assessment {
            id: AssessmentId(generate_uuid_v7()),
            lead_id: lead.id,
            profile_revision: active.revision.number,
            summary: output.summary,
            verdict_narrative: output.verdict_narrative,
            approach_angle: output.approach_angle,
            evidence,
            evidence_verified: verified,
            quality_warnings,
            provenance: self.loaded.provenance(
                &model,
                prompt.tag.clone(),
                self.settings.max_tokens,
            ),
            draft: Some(output.draft_message),
            draft_review: Some(DraftReview::pending()),
            created_at: now,
        };
        let Some(qualified_at) = lead.qualified_at else {
            return Ok(Outcome::Skipped);
        };
        match usecase.store_summary(&assessment, qualified_at, now).await {
            Ok(true) => Ok(Outcome::Stored { verified }),
            Ok(false) => Ok(Outcome::Skipped),
            Err(CoreError::Conflict(reason)) => {
                warn!(lead_id = %lead.id, %reason, "summary dropped, the lead changed meanwhile");
                Ok(Outcome::Skipped)
            }
            Err(error) => Err(error),
        }
    }
}

impl LodestoneUseCase {
    pub async fn summarise_due_leads<B, S, C, K>(
        &self,
        builder: &B,
        cipher: &S,
        cache: &C,
        settings: EvaluationSettings,
        clock: &K,
    ) -> Result<AggregationReport, CoreError>
    where
        B: ProviderBuilder,
        S: SecretCipher,
        C: LlmCache,
        K: Clock,
    {
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
        let mut report = AggregationReport::default();
        let mut budget = RunBudget::new(settings.budget_cents);
        let mut remaining = settings.batch_size;
        'profiles: for active in &profiles {
            if remaining == 0 {
                break;
            }
            let definition = active.revision.definition.definition();
            let leads = self.due_summaries(active, remaining, clock.now()).await?;
            if leads.is_empty() {
                continue;
            }
            let wanted = definition.aggregator.provider_id.map(ProviderId);
            let Some(loaded) = load_provider(
                self,
                builder,
                cipher,
                wanted,
                Tier::Qualification,
                "aggregator",
            )
            .await?
            else {
                warn!(profile = %active.profile.id.0, due = leads.len(), "aggregation skipped, the aggregator has no usable provider");
                report.failed += u64::try_from(leads.len()).unwrap_or(u64::MAX);
                for lead in &leads {
                    self.note_summary_failure(lead, REASON_NO_PROVIDER, clock.now())
                        .await?;
                }
                continue;
            };
            let summariser = Summariser {
                loaded: &loaded,
                cache,
                settings,
            };
            for lead in &leads {
                match summariser
                    .summarise(self, active, lead, &mut budget, clock)
                    .await?
                {
                    Outcome::Stored { verified } => {
                        info!(
                            monotonic_counter.aggregation_summaries = 1u64,
                            verified, "lead summary recorded"
                        );
                        report.leads += 1;
                        if !verified {
                            report.unverified += 1;
                        }
                        remaining = remaining.saturating_sub(1);
                    }
                    Outcome::Skipped => {}
                    Outcome::Failed(reason) => {
                        report.failed += 1;
                        self.note_summary_failure(lead, &reason, clock.now())
                            .await?;
                    }
                    Outcome::BudgetExhausted => {
                        report.budget_exhausted = true;
                        warn!("aggregation stopped, the run budget is exhausted");
                        break 'profiles;
                    }
                }
            }
        }
        for active in &profiles {
            report.backlog += self.count_due_summaries(active).await?;
        }
        Ok(report)
    }

    #[transactional(qualification)]
    async fn due_summaries(
        &self,
        active: &ActiveProfile,
        limit: u32,
        now: DateTime<Utc>,
    ) -> Result<Vec<Lead>, CoreError> {
        let mut repo = qualification_repository;
        repo.due_summaries(active.profile.id, active.revision.number, limit, now)
            .await
    }

    async fn note_summary_failure(
        &self,
        lead: &Lead,
        reason: &str,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let Some(qualified_at) = lead.qualified_at else {
            return Ok(());
        };
        if let Some(attempts) = self
            .record_summary_failure(lead.id, qualified_at, reason, at)
            .await?
        {
            warn!(lead_id = %lead.id, attempts, reason, "aggregation failed, next attempt delayed");
        }
        Ok(())
    }

    #[transactional(qualification)]
    async fn record_summary_failure(
        &self,
        lead_id: crate::domain::lead::LeadId,
        qualified_at: DateTime<Utc>,
        reason: &str,
        at: DateTime<Utc>,
    ) -> Result<Option<u32>, CoreError> {
        let mut repo = qualification_repository;
        repo.record_summary_failure(lead_id, qualified_at, reason, at)
            .await
    }

    #[transactional(lead, suppression, qualification)]
    pub async fn retry_lead_summary(
        &self,
        lead_id: crate::domain::lead::LeadId,
    ) -> Result<Lead, CoreError> {
        let mut leads = lead_repository;
        let mut suppression = suppression_repository;
        let mut qualification = qualification_repository;
        let lead = leads.find(lead_id).await?.ok_or(CoreError::NotFound)?;
        if suppression.is_domain_suppressed(&lead.domain).await? {
            return Err(CoreError::NotFound);
        }
        if !qualification.reset_summary(lead_id).await? {
            return Err(CoreError::Conflict(
                "lead has not been qualified yet".to_owned(),
            ));
        }
        Ok(lead)
    }

    #[transactional(qualification)]
    async fn count_due_summaries(&self, active: &ActiveProfile) -> Result<u64, CoreError> {
        let mut repo = qualification_repository;
        repo.count_due_summaries(active.profile.id, active.revision.number)
            .await
    }

    #[transactional(lead, assessment, qualification)]
    async fn store_summary(
        &self,
        assessment: &Assessment,
        qualified_at: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let mut leads = lead_repository;
        let mut assessments = assessment_repository;
        let mut qualification = qualification_repository;
        let Some(current) = leads.find(assessment.lead_id).await? else {
            return Ok(false);
        };
        let previous = assessments.list_for_lead(assessment.lead_id).await?;
        let assessment = &carry_draft(previous.first(), assessment.clone(), at);
        if !matches!(current.status, LeadStatus::New | LeadStatus::Qualified) {
            return Ok(false);
        }
        if !qualification
            .mark_summarised(assessment.lead_id, qualified_at, at)
            .await?
        {
            return Ok(false);
        }
        if current.status == LeadStatus::New {
            let to = LeadStatus::New
                .move_to(LeadStatus::Qualified)
                .map_err(|error| CoreError::Internal(error.to_string()))?;
            if leads
                .update_status(assessment.lead_id, LeadStatus::New, to, at)
                .await?
                .is_none()
            {
                return Err(CoreError::Conflict(
                    "lead status changed concurrently".to_owned(),
                ));
            }
        }
        assessments.insert(assessment).await?;
        Ok(true)
    }
}
