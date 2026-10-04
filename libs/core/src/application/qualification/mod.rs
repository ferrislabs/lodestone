use std::num::NonZeroU32;

use chrono::{DateTime, SecondsFormat, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::transactional;
use tracing::warn;

use crate::{
    application::{
        LodestoneUseCase,
        llm::gateway::{CallSettings, LlmCallError, Prompt, RunBudget, call_structured_cached},
        sources::SourceRunError,
    },
    domain::{
        job::ports::Clock,
        lead::{
            Assessment, AssessmentId, Evidence, Lead, LeadId, LeadStatus, Signal,
            ports::{AssessmentRepository, LeadRepository, SignalRepository},
        },
        llm::{
            LlmProvider, UntrustedDocument,
            cache::{CacheScope, LlmCache},
        },
        profile::ActiveProfile,
        prompt::{QUALIFICATION_V1, QualificationOutput},
        run::{Run, RunId, RunOutcome, ports::RunRepository},
        values::SourceName,
    },
};

const BUDGET_EXHAUSTED: &str = "llm budget exhausted";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualificationSettings {
    pub call: CallSettings,
    pub max_tokens: u32,
    pub budget_cents: NonZeroU32,
    pub batch_size: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QualificationReport {
    pub leads_considered: u64,
    pub qualified: u64,
    pub skipped_suppressed: u64,
    pub failed_calls: u64,
    pub budget_exhausted: bool,
}

impl QualificationReport {
    fn absorb(&mut self, other: &Self) {
        self.leads_considered += other.leads_considered;
        self.qualified += other.qualified;
        self.skipped_suppressed += other.skipped_suppressed;
        self.failed_calls += other.failed_calls;
        self.budget_exhausted |= other.budget_exhausted;
    }
}

struct QualificationLlm<'a, P, C> {
    provider: &'a P,
    cache: &'a C,
    settings: QualificationSettings,
}

pub fn render_signal(signal: &Signal) -> String {
    let raw = serde_json::to_string_pretty(&signal.raw).unwrap_or_else(|_| signal.raw.to_string());
    format!(
        "signal: {}\nvalue: {}\nsource: {}\nobserved: {}\n{}",
        signal.kind.as_str(),
        signal.value.as_deref().unwrap_or("-"),
        signal.source.as_str(),
        signal
            .observed_at
            .to_rfc3339_opts(SecondsFormat::Secs, true),
        raw,
    )
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn map_evidence(output: &QualificationOutput, signals: &[Signal]) -> Vec<Evidence> {
    output
        .evidence
        .iter()
        .filter_map(|quote| {
            let index = usize::try_from(quote.document_index).ok()?;
            let signal = signals.get(index)?;
            Some(Evidence {
                quote: quote.quote.clone(),
                provenance: signal.provenance.clone(),
            })
        })
        .collect()
}

impl LodestoneUseCase {
    pub async fn run_qualification<P, C, K>(
        &self,
        provider: &P,
        cache: &C,
        settings: QualificationSettings,
        clock: &K,
    ) -> Result<QualificationReport, SourceRunError>
    where
        P: LlmProvider,
        C: LlmCache,
        K: Clock,
    {
        let profiles = self.list_active_profiles().await?;
        let llm = QualificationLlm {
            provider,
            cache,
            settings,
        };
        let mut report = QualificationReport::default();
        let mut first_error: Option<CoreError> = None;
        for active in &profiles {
            let run = Run::start(
                RunId(generate_uuid_v7()),
                active.profile.id,
                SourceName::LlmQualification,
                clock.now(),
            );
            self.insert_qualification_run(&run).await?;
            let mut tally = QualificationReport::default();
            let result = self.qualify_profile(active, &llm, clock, &mut tally).await;
            let outcome = match &result {
                Ok(()) if tally.budget_exhausted => RunOutcome::Failed {
                    volume: tally.leads_considered,
                    signals: tally.qualified,
                    error: BUDGET_EXHAUSTED.to_owned(),
                },
                Ok(()) => RunOutcome::Succeeded {
                    volume: tally.leads_considered,
                    signals: tally.qualified,
                },
                Err(error) => RunOutcome::Failed {
                    volume: tally.leads_considered,
                    signals: tally.qualified,
                    error: error.to_string(),
                },
            };
            self.finish_qualification_run(run.id, &outcome, clock.now())
                .await?;
            report.absorb(&tally);
            if let Err(error) = result {
                first_error.get_or_insert(error);
            }
        }
        match first_error {
            Some(error) => Err(error.into()),
            None => Ok(report),
        }
    }

    async fn qualify_profile<P, C, K>(
        &self,
        active: &ActiveProfile,
        llm: &QualificationLlm<'_, P, C>,
        clock: &K,
        tally: &mut QualificationReport,
    ) -> Result<(), CoreError>
    where
        P: LlmProvider,
        C: LlmCache,
        K: Clock,
    {
        let definition = active.revision.definition.definition();
        let system = QUALIFICATION_V1.system(definition);
        let tag = QUALIFICATION_V1.tag();
        let settings = llm.settings;
        let leads = self
            .leads_to_qualify(
                active,
                definition.llm.qualification_threshold,
                &tag,
                settings.batch_size,
            )
            .await?;
        tally.leads_considered = count(leads.len());
        let mut budget = RunBudget::new(settings.budget_cents);
        for lead in &leads {
            if self.is_suppressed(&lead.domain, None).await? {
                tally.skipped_suppressed += 1;
                continue;
            }
            let signals = self.signals_of(lead.id).await?;
            let contents: Vec<String> = signals.iter().map(render_signal).collect();
            let documents: Vec<UntrustedDocument<'_>> = signals
                .iter()
                .zip(&contents)
                .map(|(signal, content)| UntrustedDocument {
                    source: signal.provenance.as_str(),
                    content,
                })
                .collect();
            let scope = CacheScope {
                profile_id: active.profile.id,
                revision: active.revision.number,
                prompt: tag.clone(),
                domain: lead.domain.clone(),
            };
            let prompt = Prompt {
                system: &system,
                instruction: QUALIFICATION_V1.instruction(),
                documents: &documents,
                max_tokens: settings.max_tokens,
            };
            let structured = match call_structured_cached::<QualificationOutput, _, _>(
                llm.provider,
                llm.cache,
                &scope,
                prompt,
                settings.call,
                &mut budget,
            )
            .await
            {
                Ok(structured) => structured,
                Err(LlmCallError::BudgetExhausted) => {
                    tally.budget_exhausted = true;
                    return Ok(());
                }
                Err(error) => {
                    warn!(domain = %lead.domain, %error, "qualification call failed, lead skipped");
                    tally.failed_calls += 1;
                    continue;
                }
            };
            let now = clock.now();
            let output = structured.value;
            let assessment = Assessment {
                id: AssessmentId(generate_uuid_v7()),
                lead_id: lead.id,
                profile_revision: active.revision.number,
                evidence: map_evidence(&output, &signals),
                summary: output.summary,
                approach_angle: output.approach_angle,
                model: structured.model,
                prompt_version: tag.clone(),
                draft: Some(output.draft_message),
                created_at: now,
            };
            if self.store_assessment(&assessment, now).await? {
                tally.qualified += 1;
            }
        }
        Ok(())
    }

    #[transactional(lead)]
    async fn leads_to_qualify(
        &self,
        active: &ActiveProfile,
        threshold: f64,
        prompt: &str,
        limit: u32,
    ) -> Result<Vec<Lead>, CoreError> {
        let mut repo = lead_repository;
        repo.list_to_qualify(
            active.profile.id,
            threshold,
            active.revision.number,
            prompt,
            limit,
        )
        .await
    }

    #[transactional(signal)]
    async fn signals_of(&self, lead_id: LeadId) -> Result<Vec<Signal>, CoreError> {
        let mut repo = signal_repository;
        repo.list_for_lead(lead_id).await
    }

    #[transactional(lead, assessment)]
    async fn store_assessment(
        &self,
        assessment: &Assessment,
        at: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let mut leads = lead_repository;
        let mut assessments = assessment_repository;
        let to = LeadStatus::New
            .move_to(LeadStatus::Qualified)
            .map_err(|error| CoreError::Internal(error.to_string()))?;
        if leads
            .update_status(assessment.lead_id, LeadStatus::New, to, at)
            .await?
            .is_none()
        {
            return Ok(false);
        }
        assessments.insert(assessment).await?;
        Ok(true)
    }

    #[transactional(run)]
    async fn insert_qualification_run(&self, run: &Run) -> Result<(), CoreError> {
        let mut repo = run_repository;
        repo.insert(run).await
    }

    #[transactional(run)]
    async fn finish_qualification_run(
        &self,
        id: RunId,
        outcome: &RunOutcome,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut repo = run_repository;
        repo.finish(id, outcome, at).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;
    use crate::domain::{
        lead::SignalId,
        values::{Provenance, SignalKind, UnitInterval},
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
        }
    }

    #[test]
    fn rendering_is_stable_plain_text() {
        assert_eq!(
            render_signal(&signal(Some("keycloak"))),
            "signal: product_detected\nvalue: keycloak\nsource: http_probe\nobserved: 2027-01-15T08:00:00Z\n{\n  \"version\": \"21.1\"\n}"
        );
    }

    #[test]
    fn a_missing_value_renders_a_dash() {
        assert!(render_signal(&signal(None)).contains("\nvalue: -\n"));
    }

    #[test]
    fn out_of_range_quotes_are_dropped() {
        use crate::domain::prompt::{DraftMessage, EvidenceQuote};
        let output = QualificationOutput {
            summary: String::new(),
            approach_angle: String::new(),
            evidence: vec![
                EvidenceQuote {
                    document_index: 0,
                    quote: "a".to_owned(),
                },
                EvidenceQuote {
                    document_index: 3,
                    quote: "b".to_owned(),
                },
            ],
            draft_message: DraftMessage {
                subject: String::new(),
                body: String::new(),
            },
        };
        let evidence = map_evidence(&output, &[signal(None)]);
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].quote, "a");
    }
}
