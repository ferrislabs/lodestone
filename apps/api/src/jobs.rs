use std::{sync::Arc, time::Duration};

use chrono::Utc;
use common::CoreError;
use dns::HickoryResolver;
use fetch::PoliteFetcher;
use lodestone_core::{
    Clock, Job, JobQueue, LodestoneUseCase, SourceName,
    infrastructure::{candidate_host::postgres::PgCandidateHosts, job::postgres::PgJobQueue},
    jobs::{JobError, JobHandler, JobOutcome, SystemClock},
    sources::{SOURCE_JOB_KIND, source_job, source_of_job},
};
use sources::{dns_cname::DnsCnameSource, http_probe::HttpProbeSource};
use tracing::info;

pub const WIRED_SOURCES: [SourceName; 2] = [SourceName::DnsCname, SourceName::HttpProbe];

pub type DnsCname = DnsCnameSource<HickoryResolver, PgCandidateHosts>;
pub type HttpProbe = HttpProbeSource<PoliteFetcher, PgCandidateHosts, LodestoneUseCase>;

pub struct JobRouter {
    usecase: LodestoneUseCase,
    dns_cname: DnsCname,
    http_probe: HttpProbe,
    source_interval: Duration,
}

impl JobRouter {
    pub fn new(
        usecase: LodestoneUseCase,
        dns_cname: DnsCname,
        http_probe: HttpProbe,
        source_interval: Duration,
    ) -> Self {
        Self {
            usecase,
            dns_cname,
            http_probe,
            source_interval,
        }
    }

    async fn run_source(&self, job: &Job) -> Result<JobOutcome, JobError> {
        let source = source_of_job(job)?;
        let result = match source {
            SourceName::DnsCname => self.usecase.run_source(&self.dns_cname, &SystemClock).await,
            SourceName::HttpProbe => {
                self.usecase
                    .run_source(&self.http_probe, &SystemClock)
                    .await
            }
            other => {
                return Err(JobError::Permanent(format!(
                    "source `{other}` is not wired"
                )));
            }
        };
        let next_run = SystemClock.now()
            + chrono::Duration::from_std(self.source_interval)
                .unwrap_or(chrono::Duration::hours(1));
        match result {
            Ok(report) => {
                info!(%source, items = report.items, signals = report.signals, "source run finished");
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

impl JobHandler for JobRouter {
    async fn handle(&self, job: &Job) -> Result<JobOutcome, JobError> {
        match job.kind.as_str() {
            SOURCE_JOB_KIND => self.run_source(job).await,
            other => Err(JobError::Permanent(format!(
                "no handler for job kind `{other}`"
            ))),
        }
    }
}

pub async fn schedule_wired_sources(queue: &Arc<PgJobQueue>) -> Result<(), CoreError> {
    let now = Utc::now();
    for source in WIRED_SOURCES {
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
