use std::{sync::Arc, time::Duration};

use chrono::Utc;
use common::CoreError;
use dns::HickoryResolver;
use fetch::PoliteFetcher;
use lodestone_core::{
    Clock, Job, JobQueue, LodestoneUseCase, SourceName,
    contacts::ContactRunSettings,
    infrastructure::{
        candidate_host::postgres::PgCandidateHosts, contact_target::postgres::PgContactTargets,
        ct_cursor::postgres::PgCtCursors, job::postgres::PgJobQueue,
    },
    jobs::{JobError, JobHandler, JobOutcome, SystemClock},
    sources::{SOURCE_JOB_KIND, source_job, source_of_job},
};
use sources::{
    contact_pages::PublicPageFinder, ct::CtSource, dns_cname::DnsCnameSource,
    http_probe::HttpProbeSource, security_txt::SecurityTxtFinder,
};
use tracing::info;

pub const WIRED_SOURCES: [SourceName; 5] = [
    SourceName::CertificateTransparency,
    SourceName::DnsCname,
    SourceName::HttpProbe,
    SourceName::PublicPage,
    SourceName::SecurityTxt,
];

pub type DnsCname = DnsCnameSource<HickoryResolver, PgCandidateHosts>;
pub type HttpProbe = HttpProbeSource<PoliteFetcher, PgCandidateHosts, LodestoneUseCase>;
pub type CertificateTransparency = CtSource<PoliteFetcher, PgCandidateHosts, PgCtCursors>;

pub struct ContactFinders {
    pub public_page: PublicPageFinder<PoliteFetcher>,
    pub security_txt: SecurityTxtFinder<PoliteFetcher>,
    pub targets: PgContactTargets,
    pub settings: ContactRunSettings,
}

pub struct WiredSources {
    pub certificate_transparency: CertificateTransparency,
    pub dns_cname: DnsCname,
    pub http_probe: HttpProbe,
    pub contacts: ContactFinders,
}

pub struct Intervals {
    pub certificate_transparency: Duration,
    pub other_sources: Duration,
}

pub struct JobRouter {
    usecase: LodestoneUseCase,
    sources: WiredSources,
    intervals: Intervals,
}

impl JobRouter {
    pub fn new(usecase: LodestoneUseCase, sources: WiredSources, intervals: Intervals) -> Self {
        Self {
            usecase,
            sources,
            intervals,
        }
    }

    async fn run_source(&self, job: &Job) -> Result<JobOutcome, JobError> {
        let source = source_of_job(job)?;
        let result = match source {
            SourceName::CertificateTransparency => self
                .usecase
                .run_source(&self.sources.certificate_transparency, &SystemClock)
                .await
                .map(|report| (report.items, report.signals)),
            SourceName::DnsCname => self
                .usecase
                .run_source(&self.sources.dns_cname, &SystemClock)
                .await
                .map(|report| (report.items, report.signals)),
            SourceName::HttpProbe => self
                .usecase
                .run_source(&self.sources.http_probe, &SystemClock)
                .await
                .map(|report| (report.items, report.signals)),
            SourceName::PublicPage => {
                let contacts = &self.sources.contacts;
                self.usecase
                    .run_contact_finder(
                        &contacts.public_page,
                        &contacts.targets,
                        contacts.settings,
                        &SystemClock,
                    )
                    .await
                    .map(|report| (report.domains, report.contacts))
            }
            SourceName::SecurityTxt => {
                let contacts = &self.sources.contacts;
                self.usecase
                    .run_contact_finder(
                        &contacts.security_txt,
                        &contacts.targets,
                        contacts.settings,
                        &SystemClock,
                    )
                    .await
                    .map(|report| (report.domains, report.contacts))
            }
            other => {
                return Err(JobError::Permanent(format!(
                    "source `{other}` is not wired"
                )));
            }
        };
        let interval = match source {
            SourceName::CertificateTransparency => self.intervals.certificate_transparency,
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
