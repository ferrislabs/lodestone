use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Duration as Delta, Utc};
use common::CoreError;
use fetch::PoliteFetcher;
use lodestone_core::{
    CALIBRATION_JOB_KIND, Clock, Job, JobQueue, LodestoneUseCase, PartnerAward, SourceName,
    WEBHOOK_JOB_KIND,
    calibration::{calibration_job, is_recurring_calibration, next_calibration},
    company::{PAGE_RECHECK_JOB_KIND, page_recheck_job},
    infrastructure::{contact_target::postgres::PgAtsTargets, job::postgres::PgJobQueue},
    is_continuous,
    jobs::{JobError, JobHandler, JobOutcome, SystemClock},
    sources::{SOURCE_JOB_KIND, SourceRunError, source_job, source_of_job},
    triage::{CandidateSource, TriageReport},
};
use sources::{
    ats::AtsSource,
    boamp::BoampSource,
    company::CompanyResearch,
    decp::{DecpSource, partner_awards},
    france_travail::FranceTravailSource,
    hacker_news::HackerNewsSource,
    ted::TedSource,
};
use tracing::info;

use crate::{
    providers::{CatalogAccess, NOT_READY_RECHECK, blocking_provider},
    webhook::{WebhookSink, run_webhook_job},
};

pub const WIRED_SOURCES: [SourceName; 6] = [
    SourceName::Boamp,
    SourceName::Decp,
    SourceName::Ted,
    SourceName::FranceTravail,
    SourceName::Ats,
    SourceName::HackerNewsHiring,
];

pub const REMOVED_QUALIFICATION_JOB_KIND: &str = "llm.qualify";
pub const PAGE_RECHECK_BATCH: u32 = 20;
const PAGE_RECHECK_PAUSE: Delta = Delta::minutes(1);

pub type Boamp = BoampSource<PoliteFetcher>;
pub type Decp = DecpSource<PoliteFetcher>;
pub type Ted = TedSource<PoliteFetcher>;
pub type FranceTravail = FranceTravailSource<PoliteFetcher, CatalogAccess>;
pub type HackerNews = HackerNewsSource<PoliteFetcher>;
pub type Ats = AtsSource<PoliteFetcher, PgAtsTargets>;
pub type CompanyPages = CompanyResearch<PoliteFetcher>;

pub struct WiredSources {
    pub boamp: Boamp,
    pub decp: Decp,
    pub ted: Ted,
    pub france_travail: FranceTravail,
    pub ats: Ats,
    pub hacker_news: HackerNews,
}

pub struct Intervals {
    pub other_sources: Duration,
    pub decp: Duration,
    pub hacker_news: Duration,
}

pub struct JobRouter {
    usecase: LodestoneUseCase,
    sources: WiredSources,
    intervals: Intervals,
    webhook: Option<WebhookSink>,
    pages: Option<CompanyPages>,
}

impl JobRouter {
    pub fn new(usecase: LodestoneUseCase, sources: WiredSources, intervals: Intervals) -> Self {
        Self {
            usecase,
            sources,
            intervals,
            webhook: None,
            pages: None,
        }
    }

    pub fn with_page_verifier(mut self, pages: CompanyPages) -> Self {
        self.pages = Some(pages);
        self
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
            rejected_by_filters = report.rejected_by_filters,
            skipped_no_lead = report.skipped_no_lead,
            cancelled = report.cancelled,
            "keyword sourcing finished"
        );
        Ok(report)
    }

    async fn run_boamp(&self) -> Result<TriageReport, SourceRunError> {
        self.triage_source(&self.sources.boamp, "boamp").await
    }

    async fn run_ted(&self) -> Result<TriageReport, SourceRunError> {
        self.triage_source(&self.sources.ted, "ted").await
    }

    async fn run_ats(&self) -> Result<TriageReport, SourceRunError> {
        self.triage_source(&self.sources.ats, "ats").await
    }

    async fn run_hacker_news(&self) -> Result<TriageReport, SourceRunError> {
        self.triage_source(&self.sources.hacker_news, "hacker_news_hiring")
            .await
    }

    async fn run_france_travail(&self) -> Result<TriageReport, SourceRunError> {
        self.triage_source(&self.sources.france_travail, "france_travail")
            .await
    }

    async fn run_decp(&self) -> Result<TriageReport, SourceRunError> {
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
                    .enabled_search_agents(SourceName::Decp)
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
        Ok(report)
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

    async fn run_page_recheck(&self) -> Result<JobOutcome, JobError> {
        let Some(pages) = &self.pages else {
            return Err(JobError::Permanent(
                "no page verifier is wired for the dossier page recheck".to_owned(),
            ));
        };
        let report = self
            .usecase
            .recheck_company_pages(pages, PAGE_RECHECK_BATCH, &SystemClock)
            .await
            .map_err(JobError::from)?;
        info!(
            dossiers = report.dossiers,
            changed = report.changed,
            remaining = report.remaining,
            unconfirmed_links = report.unconfirmed_links.total(),
            unconfirmed_by_reason = %report.unconfirmed_links,
            "dossier page recheck batch finished"
        );
        Ok(recheck_outcome(
            report.remaining,
            report.retry_after,
            SystemClock.now(),
        ))
    }

    async fn run_source(&self, job: &Job) -> Result<JobOutcome, JobError> {
        let source = source_of_job(job)?;
        if is_continuous(source) {
            return Err(JobError::Permanent(format!(
                "source `{source}` listens continuously and has no recurring job"
            )));
        }
        let interval = match source {
            SourceName::Decp => self.intervals.decp,
            SourceName::HackerNewsHiring => self.intervals.hacker_news,
            _ => self.intervals.other_sources,
        };
        if let Some(provider) = blocking_provider(&self.usecase, source.as_str())
            .await
            .map_err(|error| JobError::Retryable(error.to_string()))?
        {
            info!(%source, provider, "provider not ready, the source run is skipped");
            return Ok(JobOutcome::RescheduleAt(
                SystemClock.now() + recheck_delay(interval),
            ));
        }
        let result = match source {
            SourceName::Boamp => self.run_boamp().await,
            SourceName::Decp => self.run_decp().await,
            SourceName::Ted => self.run_ted().await,
            SourceName::FranceTravail => self.run_france_travail().await,
            SourceName::Ats => self.run_ats().await,
            SourceName::HackerNewsHiring => self.run_hacker_news().await,
            other => {
                return Err(JobError::Permanent(format!(
                    "source `{other}` is not wired"
                )));
            }
        };
        let next_run = SystemClock.now() + delay_of(interval);
        match result {
            Ok(report) => {
                if let Some(reason) = report.all_cancelled()
                    && !self
                        .usecase
                        .has_source_targets(source)
                        .await
                        .map_err(|error| JobError::Retryable(error.to_string()))?
                {
                    info!(%source, reason = reason.as_str(), "source run stopped, no agent serves it any more");
                    return Ok(JobOutcome::Cancelled(reason));
                }
                info!(%source, items = report.candidates, signals = report.accepted(), "source run finished");
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

fn delay_of(interval: Duration) -> Delta {
    Delta::from_std(interval).unwrap_or(Delta::hours(1))
}

pub fn recheck_delay(interval: Duration) -> Delta {
    delay_of(interval.min(NOT_READY_RECHECK))
}

pub fn recheck_outcome(
    remaining: u64,
    retry_after: Option<Duration>,
    now: DateTime<Utc>,
) -> JobOutcome {
    match retry_after {
        Some(wait) => JobOutcome::RescheduleAt(now + delay_of(wait).max(PAGE_RECHECK_PAUSE)),
        None if remaining > 0 => JobOutcome::RescheduleAt(now),
        None => JobOutcome::Done,
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
            PAGE_RECHECK_JOB_KIND => self.run_page_recheck().await,
            other => Err(JobError::Permanent(format!(
                "no handler for job kind `{other}`"
            ))),
        }
    }
}

pub fn scheduled_sources() -> Vec<SourceName> {
    WIRED_SOURCES.to_vec()
}

pub async fn schedule_wired_sources(queue: &Arc<PgJobQueue>) -> Result<(), CoreError> {
    let now = Utc::now();
    for source in scheduled_sources() {
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

pub async fn schedule_page_recheck(
    queue: &Arc<PgJobQueue>,
    usecase: &LodestoneUseCase,
) -> Result<(), CoreError> {
    let unchecked = usecase.count_unchecked_company_pages().await?;
    if unchecked == 0 {
        return Ok(());
    }
    let now = Utc::now();
    if queue.enqueue(&page_recheck_job(now)?, now).await?.is_some() {
        info!(unchecked, "scheduled the recheck of stored dossier pages");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use lodestone_core::{CONTINUOUS_SOURCES, HACKER_NEWS_HIRING, provider_key_of_source};

    use super::*;

    #[test]
    fn continuous_sources_are_never_scheduled() {
        for source in CONTINUOUS_SOURCES {
            assert!(!WIRED_SOURCES.contains(&source));
            assert!(!scheduled_sources().contains(&source));
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
            !scheduled_sources()
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
        assert_eq!(next_calibration(now) - now, Delta::hours(24));
        assert!(
            !scheduled_sources()
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
        assert!(scheduled_sources().contains(&SourceName::Ats));
        let job = source_job(SourceName::Ats, Utc::now()).unwrap();
        assert_eq!(job.dedup_key.as_deref(), Some("source:ats"));
        assert_eq!(job.payload["source"], "ats");
    }

    #[test]
    fn the_france_travail_job_is_scheduled_and_gated_by_its_provider() {
        assert!(WIRED_SOURCES.contains(&SourceName::FranceTravail));
        assert!(scheduled_sources().contains(&SourceName::FranceTravail));
        assert_eq!(scheduled_sources().len(), WIRED_SOURCES.len());
        let job = source_job(SourceName::FranceTravail, Utc::now()).unwrap();
        assert_eq!(job.dedup_key.as_deref(), Some("source:france_travail"));
        assert_eq!(job.payload["source"], "france_travail");
    }

    #[test]
    fn the_hacker_news_job_is_scheduled_and_gated_by_its_provider() {
        assert!(WIRED_SOURCES.contains(&SourceName::HackerNewsHiring));
        assert!(scheduled_sources().contains(&SourceName::HackerNewsHiring));
        let job = source_job(SourceName::HackerNewsHiring, Utc::now()).unwrap();
        assert_eq!(job.dedup_key.as_deref(), Some("source:hacker_news_hiring"));
        assert_eq!(job.payload["source"], "hacker_news_hiring");
        assert_eq!(
            provider_key_of_source(SourceName::HackerNewsHiring.as_str()),
            Some(HACKER_NEWS_HIRING)
        );
    }

    #[test]
    fn the_page_recheck_job_is_deduplicated_and_runs_until_no_page_is_left() {
        let now = Utc::now();
        let job = page_recheck_job(now).unwrap();
        assert_eq!(job.kind.as_str(), PAGE_RECHECK_JOB_KIND);
        assert_eq!(job.dedup_key.as_deref(), Some("company:recheck_pages"));
        assert_eq!(recheck_outcome(0, None, now), JobOutcome::Done);
        assert_eq!(recheck_outcome(3, None, now), JobOutcome::RescheduleAt(now));
        assert_eq!(
            recheck_outcome(3, Some(Duration::from_secs(5)), now),
            JobOutcome::RescheduleAt(now + Delta::minutes(1))
        );
        assert_eq!(
            recheck_outcome(3, Some(Duration::from_secs(600)), now),
            JobOutcome::RescheduleAt(now + Delta::minutes(10))
        );
    }

    #[test]
    fn a_blocked_source_is_rechecked_sooner_than_its_interval() {
        assert_eq!(
            recheck_delay(Duration::from_secs(6 * 3600)),
            Delta::minutes(5)
        );
        assert_eq!(recheck_delay(Duration::from_secs(60)), Delta::minutes(1));
    }
}
