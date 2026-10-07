use std::{sync::Arc, time::Duration};

use chrono::Utc;
use common::CoreError;
use fetch::PoliteFetcher;
use lodestone_core::{
    CALIBRATION_JOB_KIND, Clock, Job, JobQueue, LodestoneUseCase, PartnerAward, SourceName,
    WEBHOOK_JOB_KIND,
    calibration::{calibration_job, is_recurring_calibration, next_calibration},
    infrastructure::{contact_target::postgres::PgAtsTargets, job::postgres::PgJobQueue},
    is_continuous,
    jobs::{JobError, JobHandler, JobOutcome, SystemClock},
    sources::{SOURCE_JOB_KIND, SourceRunError, source_job, source_of_job},
    triage::{CandidateSource, TriageReport},
};
use sources::{
    ats::AtsSource,
    boamp::BoampSource,
    decp::{DecpSource, partner_awards},
    france_travail::FranceTravailSource,
    ted::TedSource,
};
use tracing::info;

use crate::webhook::{WebhookSink, run_webhook_job};

pub const WIRED_SOURCES: [SourceName; 4] = [
    SourceName::Boamp,
    SourceName::Decp,
    SourceName::Ted,
    SourceName::Ats,
];

pub const FRANCE_TRAVAIL_SOURCE: SourceName = SourceName::FranceTravail;

pub const REMOVED_QUALIFICATION_JOB_KIND: &str = "llm.qualify";

pub type Boamp = BoampSource<PoliteFetcher>;
pub type Decp = DecpSource<PoliteFetcher>;
pub type Ted = TedSource<PoliteFetcher>;
pub type FranceTravail = FranceTravailSource<PoliteFetcher>;
pub type Ats = AtsSource<PoliteFetcher, PgAtsTargets>;

pub struct WiredSources {
    pub boamp: Boamp,
    pub decp: Decp,
    pub ted: Ted,
    pub france_travail: Option<FranceTravail>,
    pub ats: Ats,
}

pub struct Intervals {
    pub other_sources: Duration,
    pub decp: Duration,
}

pub struct JobRouter {
    usecase: LodestoneUseCase,
    sources: WiredSources,
    intervals: Intervals,
    webhook: Option<WebhookSink>,
}

impl JobRouter {
    pub fn new(usecase: LodestoneUseCase, sources: WiredSources, intervals: Intervals) -> Self {
        Self {
            usecase,
            sources,
            intervals,
            webhook: None,
        }
    }

    pub fn with_webhook(mut self, webhook: Option<WebhookSink>) -> Self {
        self.webhook = webhook;
        self
    }

    async fn triage_source<S: CandidateSource>(
        &self,
        source: &S,
        label: &str,
    ) -> Result<TriageReport, SourceRunError> {
        let report = self.usecase.run_triage(source, &SystemClock).await?;
        info!(
            source = label,
            candidates = report.candidates,
            accepted_by_rules = report.accepted_by_rules,
            accepted_ambiguous = report.accepted_ambiguous,
            rejected_by_rules = report.rejected_by_rules,
            "keyword sourcing finished"
        );
        Ok(report)
    }

    async fn run_boamp(&self) -> Result<(u64, u64), SourceRunError> {
        let report = self.triage_source(&self.sources.boamp, "boamp").await?;
        Ok((report.candidates, report.accepted()))
    }

    async fn run_ted(&self) -> Result<(u64, u64), SourceRunError> {
        let report = self.triage_source(&self.sources.ted, "ted").await?;
        Ok((report.candidates, report.accepted()))
    }

    async fn run_ats(&self) -> Result<(u64, u64), SourceRunError> {
        let report = self.triage_source(&self.sources.ats, "ats").await?;
        Ok((report.candidates, report.accepted()))
    }

    async fn run_france_travail(&self) -> Result<(u64, u64), SourceRunError> {
        let Some(source) = &self.sources.france_travail else {
            info!("france travail source is disabled, the stale job completes without work");
            return Ok((0, 0));
        };
        let report = self.triage_source(source, "france_travail").await?;
        Ok((report.candidates, report.accepted()))
    }

    async fn run_decp(&self) -> Result<(u64, u64), SourceRunError> {
        let report = self.triage_source(&self.sources.decp, "decp").await?;
        let contracts = self.sources.decp.take_awards();
        let mut partners = 0;
        if !contracts.is_empty() {
            for active in self.usecase.list_active_profiles().await? {
                let mut awards: Vec<PartnerAward> = Vec::new();
                for rules in active
                    .revision
                    .definition
                    .definition()
                    .enabled_keyword_rules(SourceName::Decp)
                {
                    for award in partner_awards(rules, &contracts) {
                        if !awards.iter().any(|known| {
                            known.holder == award.holder && known.contract_id == award.contract_id
                        }) {
                            awards.push(award);
                        }
                    }
                }
                if awards.is_empty() {
                    continue;
                }
                partners += self
                    .usecase
                    .record_partner_awards(active.profile.id, &awards, SystemClock.now())
                    .await?;
            }
        }
        info!(
            awarded_contracts = contracts.len(),
            partner_awards = partners,
            "decp partner awards recorded"
        );
        Ok((report.candidates, report.accepted()))
    }

    async fn run_calibration(&self, job: &Job) -> Result<JobOutcome, JobError> {
        let now = SystemClock.now();
        let recurring = is_recurring_calibration(job);
        let outcome = if recurring {
            JobOutcome::RescheduleAt(next_calibration(now))
        } else {
            JobOutcome::Done
        };
        match self.usecase.calibrate_profiles(now).await {
            Ok(report) => {
                info!(
                    profiles = report.profiles,
                    active_maps = report.agents_fitted,
                    leads_marked_stale = report.rescored,
                    "calibration run finished"
                );
                Ok(outcome)
            }
            Err(error) if recurring && job.attempts >= job.max_attempts.get() => {
                info!(%error, "calibration failed on its last attempt, scheduling the next run");
                Ok(outcome)
            }
            Err(error) => Err(JobError::Retryable(error.to_string())),
        }
    }

    async fn run_source(&self, job: &Job) -> Result<JobOutcome, JobError> {
        let source = source_of_job(job)?;
        if is_continuous(source) {
            return Err(JobError::Permanent(format!(
                "source `{source}` listens continuously and has no recurring job"
            )));
        }
        let result = match source {
            SourceName::Boamp => self.run_boamp().await,
            SourceName::Decp => self.run_decp().await,
            SourceName::Ted => self.run_ted().await,
            SourceName::FranceTravail => self.run_france_travail().await,
            SourceName::Ats => self.run_ats().await,
            other => {
                return Err(JobError::Permanent(format!(
                    "source `{other}` is not wired"
                )));
            }
        };
        let interval = match source {
            SourceName::Decp => self.intervals.decp,
            _ => self.intervals.other_sources,
        };
        let next_run = SystemClock.now()
            + chrono::Duration::from_std(interval).unwrap_or(chrono::Duration::hours(1));
        match result {
            Ok((items, signals)) => {
                info!(%source, items, signals, "source run finished");
                Ok(JobOutcome::RescheduleAt(next_run))
            }
            Err(error) if job.attempts >= job.max_attempts.get() => {
                info!(%source, %error, "source run failed on its last attempt, scheduling the next run");
                Ok(JobOutcome::RescheduleAt(next_run))
            }
            Err(error) => Err(error.into()),
        }
    }
}

pub fn removed_job_reason(kind: &str) -> Option<&'static str> {
    (kind == REMOVED_QUALIFICATION_JOB_KIND).then_some(
        "the llm.qualify job is removed, qualification runs in the qualification listener",
    )
}

impl JobHandler for JobRouter {
    async fn handle(&self, job: &Job) -> Result<JobOutcome, JobError> {
        if let Some(reason) = removed_job_reason(job.kind.as_str()) {
            return Err(JobError::Permanent(reason.to_owned()));
        }
        match job.kind.as_str() {
            SOURCE_JOB_KIND => self.run_source(job).await,
            WEBHOOK_JOB_KIND => run_webhook_job(&self.usecase, self.webhook.as_ref(), job).await,
            CALIBRATION_JOB_KIND => self.run_calibration(job).await,
            other => Err(JobError::Permanent(format!(
                "no handler for job kind `{other}`"
            ))),
        }
    }
}

pub fn scheduled_sources(france_travail_enabled: bool) -> Vec<SourceName> {
    let mut sources = WIRED_SOURCES.to_vec();
    if france_travail_enabled {
        sources.push(FRANCE_TRAVAIL_SOURCE);
    }
    sources
}

pub async fn schedule_wired_sources(
    queue: &Arc<PgJobQueue>,
    france_travail_enabled: bool,
) -> Result<(), CoreError> {
    let now = Utc::now();
    if !france_travail_enabled {
        info!(
            "france travail source disabled, set FRANCE_TRAVAIL_CLIENT_ID and FRANCE_TRAVAIL_CLIENT_SECRET to enable it"
        );
    }
    for source in scheduled_sources(france_travail_enabled) {
        if queue
            .enqueue(&source_job(source, now)?, now)
            .await?
            .is_some()
        {
            info!(%source, "scheduled first source run");
        }
    }
    Ok(())
}

pub async fn schedule_calibration(queue: &Arc<PgJobQueue>) -> Result<(), CoreError> {
    let now = Utc::now();
    if queue
        .enqueue(&calibration_job(now, true)?, now)
        .await?
        .is_some()
    {
        info!("scheduled first calibration run");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use lodestone_core::CONTINUOUS_SOURCES;

    use super::*;

    #[test]
    fn continuous_sources_are_never_scheduled() {
        for source in CONTINUOUS_SOURCES {
            assert!(!WIRED_SOURCES.contains(&source));
            assert!(!scheduled_sources(true).contains(&source));
        }
    }

    #[test]
    fn the_former_qualification_job_is_rejected_as_permanent_by_kind() {
        assert!(removed_job_reason("llm.qualify").is_some());
        assert!(removed_job_reason(SOURCE_JOB_KIND).is_none());
        assert!(removed_job_reason(WEBHOOK_JOB_KIND).is_none());
    }

    #[test]
    fn no_recurring_qualification_job_is_scheduled() {
        assert!(
            !scheduled_sources(true)
                .iter()
                .any(|source| source.as_str().contains("qualif"))
        );
    }

    #[test]
    fn the_calibration_job_is_recurring_deduplicated_and_not_a_source() {
        let now = Utc::now();
        let recurring = calibration_job(now, true).unwrap();
        assert_eq!(recurring.kind.as_str(), CALIBRATION_JOB_KIND);
        assert_eq!(recurring.dedup_key.as_deref(), Some("quality:calibrate"));
        assert_eq!(recurring.payload["recurring"], true);
        let manual = calibration_job(now, false).unwrap();
        assert_eq!(
            manual.dedup_key.as_deref(),
            Some("quality:calibrate:manual")
        );
        assert_eq!(manual.payload["recurring"], false);
        assert_ne!(recurring.dedup_key, manual.dedup_key);
        assert_eq!(next_calibration(now) - now, chrono::Duration::hours(24));
        assert!(
            !scheduled_sources(true)
                .iter()
                .any(|source| source.as_str().contains("calibrat"))
        );
    }

    #[test]
    fn the_boamp_source_job_is_wired_and_deduplicated() {
        assert!(WIRED_SOURCES.contains(&SourceName::Boamp));
        let job = source_job(SourceName::Boamp, Utc::now()).unwrap();
        assert_eq!(job.dedup_key.as_deref(), Some("source:boamp"));
        assert_eq!(job.payload["source"], "boamp");
    }

    #[test]
    fn the_decp_source_job_is_wired_and_deduplicated() {
        assert!(WIRED_SOURCES.contains(&SourceName::Decp));
        let job = source_job(SourceName::Decp, Utc::now()).unwrap();
        assert_eq!(job.dedup_key.as_deref(), Some("source:decp"));
        assert_eq!(job.payload["source"], "decp");
    }

    #[test]
    fn the_ted_source_job_is_wired_and_deduplicated() {
        assert!(WIRED_SOURCES.contains(&SourceName::Ted));
        let job = source_job(SourceName::Ted, Utc::now()).unwrap();
        assert_eq!(job.dedup_key.as_deref(), Some("source:ted"));
        assert_eq!(job.payload["source"], "ted");
    }

    #[test]
    fn the_ats_source_job_is_wired_and_deduplicated() {
        assert!(WIRED_SOURCES.contains(&SourceName::Ats));
        assert!(scheduled_sources(false).contains(&SourceName::Ats));
        let job = source_job(SourceName::Ats, Utc::now()).unwrap();
        assert_eq!(job.dedup_key.as_deref(), Some("source:ats"));
        assert_eq!(job.payload["source"], "ats");
    }

    #[test]
    fn the_france_travail_job_is_scheduled_only_with_credentials() {
        assert!(!WIRED_SOURCES.contains(&SourceName::FranceTravail));
        assert!(!scheduled_sources(false).contains(&SourceName::FranceTravail));
        assert!(scheduled_sources(true).contains(&SourceName::FranceTravail));
        assert_eq!(scheduled_sources(true).len(), WIRED_SOURCES.len() + 1);
        let job = source_job(SourceName::FranceTravail, Utc::now()).unwrap();
        assert_eq!(job.dedup_key.as_deref(), Some("source:france_travail"));
        assert_eq!(job.payload["source"], "france_travail");
    }
}
