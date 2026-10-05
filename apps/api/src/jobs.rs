use std::{
    num::{NonZeroU16, NonZeroU32},
    sync::Arc,
    time::Duration,
};

use chrono::Utc;
use common::CoreError;
use dns::HickoryResolver;
use fetch::PoliteFetcher;
use llm::any::AnyProvider;
use lodestone_core::{
    Clock, Job, JobKind, JobQueue, LodestoneUseCase, NewJob, SourceName, Tier, TierError,
    TokenPricing,
    contacts::ContactRunSettings,
    infrastructure::{
        candidate_host::postgres::PgCandidateHosts, contact_target::postgres::PgContactTargets,
        crypto::AesGcmCipher, ct_cursor::postgres::PgCtCursors, job::postgres::PgJobQueue,
        llm_cache::postgres::PgLlmCache,
    },
    jobs::{JobError, JobHandler, JobOutcome, SystemClock},
    llm::gateway::CallSettings,
    qualification::QualificationSettings,
    sources::{SOURCE_JOB_KIND, SourceRunError, source_job, source_of_job},
    triage::{CandidateSource, TriageReport, TriageSettings},
};
use sources::{
    boamp::BoampSource,
    contact_pages::PublicPageFinder,
    ct::CtSource,
    decp::{DecpSource, partner_awards},
    dns_cname::DnsCnameSource,
    france_travail::FranceTravailSource,
    http_probe::HttpProbeSource,
    role_guess::RoleGuessFinder,
    security_txt::SecurityTxtFinder,
    ted::TedSource,
};
use tracing::{info, warn};

pub const WIRED_SOURCES: [SourceName; 9] = [
    SourceName::CertificateTransparency,
    SourceName::DnsCname,
    SourceName::HttpProbe,
    SourceName::PublicPage,
    SourceName::SecurityTxt,
    SourceName::RoleGuess,
    SourceName::Boamp,
    SourceName::Decp,
    SourceName::Ted,
];

pub const FRANCE_TRAVAIL_SOURCE: SourceName = SourceName::FranceTravail;

pub const QUALIFICATION_JOB_KIND: &str = "llm.qualify";
pub const QUALIFICATION_DEDUP_KEY: &str = "llm:qualify";
const QUALIFICATION_MAX_ATTEMPTS: NonZeroU16 = NonZeroU16::new(3).unwrap();
const QUALIFICATION_MAX_TOKENS: u32 = 4096;
const QUALIFICATION_BATCH_SIZE: u32 = 20;
const QUALIFICATION_BASE_DELAY: Duration = Duration::from_secs(2);
const TRIAGE_MAX_TOKENS: u32 = 1024;
const TRIAGE_BASE_DELAY: Duration = Duration::from_secs(2);

pub type DnsCname = DnsCnameSource<HickoryResolver, PgCandidateHosts>;
pub type HttpProbe = HttpProbeSource<PoliteFetcher, PgCandidateHosts, LodestoneUseCase>;
pub type Boamp = BoampSource<PoliteFetcher>;
pub type Decp = DecpSource<PoliteFetcher>;
pub type Ted = TedSource<PoliteFetcher>;
pub type FranceTravail = FranceTravailSource<PoliteFetcher>;
pub type CertificateTransparency = CtSource<PoliteFetcher, PgCandidateHosts, PgCtCursors>;

pub struct ContactFinders {
    pub public_page: PublicPageFinder<PoliteFetcher>,
    pub security_txt: SecurityTxtFinder<PoliteFetcher>,
    pub role_guess: RoleGuessFinder<HickoryResolver>,
    pub targets: PgContactTargets,
    pub settings: ContactRunSettings,
}

pub struct WiredSources {
    pub certificate_transparency: CertificateTransparency,
    pub dns_cname: DnsCname,
    pub http_probe: HttpProbe,
    pub boamp: Boamp,
    pub decp: Decp,
    pub ted: Ted,
    pub france_travail: Option<FranceTravail>,
    pub contacts: ContactFinders,
}

pub struct Intervals {
    pub certificate_transparency: Duration,
    pub other_sources: Duration,
    pub decp: Duration,
}

pub struct LlmRuntime {
    pub cipher: AesGcmCipher,
    pub cache: PgLlmCache,
    pub budget_cents: NonZeroU32,
}

pub struct JobRouter {
    usecase: LodestoneUseCase,
    sources: WiredSources,
    intervals: Intervals,
    llm: LlmRuntime,
}

pub fn qualification_job(run_at: chrono::DateTime<Utc>) -> Result<NewJob, CoreError> {
    let kind = JobKind::parse(QUALIFICATION_JOB_KIND)
        .map_err(|_| CoreError::Internal("invalid qualification job kind".to_owned()))?;
    Ok(NewJob {
        kind,
        payload: Default::default(),
        run_at,
        max_attempts: QUALIFICATION_MAX_ATTEMPTS,
        dedup_key: Some(QUALIFICATION_DEDUP_KEY.to_owned()),
    })
}

impl JobRouter {
    pub fn new(
        usecase: LodestoneUseCase,
        sources: WiredSources,
        intervals: Intervals,
        llm: LlmRuntime,
    ) -> Self {
        Self {
            usecase,
            sources,
            intervals,
            llm,
        }
    }

    fn next_run(&self) -> chrono::DateTime<Utc> {
        SystemClock.now()
            + chrono::Duration::from_std(self.intervals.other_sources)
                .unwrap_or(chrono::Duration::hours(1))
    }

    async fn run_qualification(&self, job: &Job) -> Result<JobOutcome, JobError> {
        let next_run = self.next_run();
        let resolved = match self
            .usecase
            .resolve_llm_tier(Tier::Qualification, &self.llm.cipher)
            .await
        {
            Ok(resolved) => resolved,
            Err(
                error @ (TierError::Unassigned(_) | TierError::Disabled(_) | TierError::Cipher(_)),
            ) => {
                warn!(%error, "qualification skipped, no usable provider for the tier");
                return Ok(JobOutcome::RescheduleAt(next_run));
            }
            Err(error) => return Err(JobError::Retryable(error.to_string())),
        };
        let provider = match AnyProvider::from_definition(&resolved.definition) {
            Ok(provider) => provider,
            Err(error) => {
                warn!(%error, "qualification skipped, provider could not be built");
                return Ok(JobOutcome::RescheduleAt(next_run));
            }
        };
        let settings = QualificationSettings {
            call: CallSettings {
                max_retries: resolved.max_retries,
                pricing: resolved.pricing,
                base_delay: QUALIFICATION_BASE_DELAY,
            },
            max_tokens: QUALIFICATION_MAX_TOKENS,
            budget_cents: self.llm.budget_cents,
            batch_size: QUALIFICATION_BATCH_SIZE,
        };
        match self
            .usecase
            .run_qualification(&provider, &self.llm.cache, settings, &SystemClock)
            .await
        {
            Ok(report) => {
                info!(
                    considered = report.leads_considered,
                    qualified = report.qualified,
                    skipped_suppressed = report.skipped_suppressed,
                    failed_calls = report.failed_calls,
                    budget_exhausted = report.budget_exhausted,
                    "qualification run finished"
                );
                Ok(JobOutcome::RescheduleAt(next_run))
            }
            Err(error) if job.attempts >= job.max_attempts.get() => {
                info!(%error, "qualification failed on its last attempt, scheduling the next run");
                Ok(JobOutcome::RescheduleAt(next_run))
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn triage_source<S: CandidateSource>(
        &self,
        source: &S,
        label: &str,
    ) -> Result<TriageReport, SourceRunError> {
        let resolved = match self
            .usecase
            .resolve_llm_tier(Tier::Triage, &self.llm.cipher)
            .await
        {
            Ok(resolved) => Some(resolved),
            Err(error) => {
                warn!(%error, source = label, "triage has no usable provider, ambiguous candidates are deferred");
                None
            }
        };
        let provider = resolved.as_ref().and_then(|resolved| {
            match AnyProvider::from_definition(&resolved.definition) {
                Ok(provider) => Some(provider),
                Err(error) => {
                    warn!(%error, source = label, "triage provider could not be built, ambiguous candidates are deferred");
                    None
                }
            }
        });
        let (max_retries, pricing) = resolved.as_ref().map_or(
            (
                0,
                TokenPricing {
                    input_cents_per_mtok: 0,
                    output_cents_per_mtok: 0,
                },
            ),
            |resolved| (resolved.max_retries, resolved.pricing),
        );
        let settings = TriageSettings {
            call: CallSettings {
                max_retries,
                pricing,
                base_delay: TRIAGE_BASE_DELAY,
            },
            max_tokens: TRIAGE_MAX_TOKENS,
            budget_cents: self.llm.budget_cents,
        };
        let report = self
            .usecase
            .run_triage(
                source,
                provider.as_ref(),
                &self.llm.cache,
                settings,
                &SystemClock,
            )
            .await?;
        info!(
            source = label,
            candidates = report.candidates,
            accepted_by_rules = report.accepted_by_rules,
            rejected_by_rules = report.rejected_by_rules,
            sent_to_llm = report.sent_to_llm,
            accepted_by_llm = report.accepted_by_llm,
            deferred = report.deferred,
            budget_exhausted = report.budget_exhausted,
            "triage finished"
        );
        Ok(report)
    }

    async fn run_boamp(&self) -> Result<(u64, u64), SourceRunError> {
        let report = self.triage_source(&self.sources.boamp, "boamp").await?;
        Ok((
            report.candidates,
            report.accepted_by_rules + report.accepted_by_llm,
        ))
    }

    async fn run_ted(&self) -> Result<(u64, u64), SourceRunError> {
        let report = self.triage_source(&self.sources.ted, "ted").await?;
        Ok((
            report.candidates,
            report.accepted_by_rules + report.accepted_by_llm,
        ))
    }

    async fn run_france_travail(&self) -> Result<(u64, u64), SourceRunError> {
        let Some(source) = &self.sources.france_travail else {
            info!("france travail source is disabled, the stale job completes without work");
            return Ok((0, 0));
        };
        let report = self.triage_source(source, "france_travail").await?;
        Ok((
            report.candidates,
            report.accepted_by_rules + report.accepted_by_llm,
        ))
    }

    async fn run_decp(&self) -> Result<(u64, u64), SourceRunError> {
        let report = self.triage_source(&self.sources.decp, "decp").await?;
        let contracts = self.sources.decp.take_awards();
        let mut partners = 0;
        if !contracts.is_empty() {
            for active in self.usecase.list_active_profiles().await? {
                let Some(rules) = self
                    .sources
                    .decp
                    .rules(active.revision.definition.definition())
                else {
                    continue;
                };
                let awards = partner_awards(rules, &contracts);
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
        Ok((
            report.candidates,
            report.accepted_by_rules + report.accepted_by_llm,
        ))
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
            SourceName::RoleGuess => {
                let contacts = &self.sources.contacts;
                self.usecase
                    .run_contact_finder(
                        &contacts.role_guess,
                        &contacts.targets,
                        contacts.settings,
                        &SystemClock,
                    )
                    .await
                    .map(|report| (report.domains, report.contacts))
            }
            SourceName::Boamp => self.run_boamp().await,
            SourceName::Decp => self.run_decp().await,
            SourceName::Ted => self.run_ted().await,
            SourceName::FranceTravail => self.run_france_travail().await,
            other => {
                return Err(JobError::Permanent(format!(
                    "source `{other}` is not wired"
                )));
            }
        };
        let interval = match source {
            SourceName::CertificateTransparency => self.intervals.certificate_transparency,
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

impl JobHandler for JobRouter {
    async fn handle(&self, job: &Job) -> Result<JobOutcome, JobError> {
        match job.kind.as_str() {
            SOURCE_JOB_KIND => self.run_source(job).await,
            QUALIFICATION_JOB_KIND => self.run_qualification(job).await,
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
    if queue
        .enqueue(&qualification_job(now)?, now)
        .await?
        .is_some()
    {
        info!("scheduled first qualification run");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_qualification_job_is_recurring_and_deduplicated() {
        let job = qualification_job(Utc::now()).unwrap();
        assert_eq!(QUALIFICATION_JOB_KIND, "llm.qualify");
        assert_eq!(job.kind.as_str(), "llm.qualify");
        assert_eq!(job.dedup_key.as_deref(), Some("llm:qualify"));
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
