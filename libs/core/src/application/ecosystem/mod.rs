use std::time::{Duration as StdDuration, Instant};

use chrono::{DateTime, Duration, Utc};
use common::CoreError;
use lodestone_macros::transactional;
use thiserror::Error;
use tracing::{debug, info};

use crate::{
    application::{LodestoneUseCase, sources::SourceRunError},
    domain::{
        NormalizedDomain,
        company::bounded_error,
        ecosystem::{
            DomainEcosystem, DueDomain, EcosystemFinding, EcosystemModule, InspectionSettings,
            InspectionTarget, ModuleOutcome, ModuleRun, ModuleRunner, ModuleStats, ModuleStatus,
            STATS_WINDOW_DAYS, hosting_platform, ports::EcosystemRepository,
        },
        job::ports::Clock,
        lead::{Lead, LeadId, ports::LeadRepository},
        profile::{
            ActiveProfile, ProfileId, definition::ProfileDefinition, ports::ProfileRepository,
        },
        source::SourceError,
        suppression::ports::SuppressionRepository,
    },
};

pub const ECOSYSTEM_LISTENER: &str = "ecosystem_inspection";
const MIN_RATE_LIMIT_PAUSE: Duration = Duration::minutes(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectionRunSettings {
    pub batch_size: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InspectionReport {
    pub domains: u64,
    pub findings: u64,
    pub failed: u64,
    pub not_applicable: u64,
    pub backlog: u64,
    pub rate_limited: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DryRun {
    pub module: EcosystemModule,
    pub domain: NormalizedDomain,
    pub status: ModuleStatus,
    pub findings: Vec<EcosystemFinding>,
    pub hosting_platform: Option<&'static str>,
    pub error: Option<String>,
    pub duration: StdDuration,
}

#[derive(Debug, Error)]
pub enum DryRunError {
    #[error("the module `{0}` is not available yet")]
    Unavailable(&'static str),
    #[error("rate limited, retry after {retry_after:?}")]
    RateLimited { retry_after: StdDuration },
    #[error(transparent)]
    Core(#[from] CoreError),
}

fn pause_of(retry_after: StdDuration) -> Duration {
    Duration::from_std(retry_after)
        .unwrap_or(MIN_RATE_LIMIT_PAUSE)
        .max(MIN_RATE_LIMIT_PAUSE)
}

fn not_applicable(module: EcosystemModule, domain: &NormalizedDomain) -> bool {
    module.reads_site() && hosting_platform(domain).is_some()
}

fn inspected_profiles(profiles: &[ActiveProfile]) -> Vec<&ActiveProfile> {
    profiles
        .iter()
        .filter(|active| active.revision.definition.definition().inspection.enabled)
        .collect()
}

fn settings_for(inspected: &[&ActiveProfile], due: &DueDomain) -> InspectionSettings {
    InspectionSettings::of(
        inspected
            .iter()
            .filter(|active| due.profiles.contains(&active.profile.id))
            .map(|active| active.revision.definition.definition()),
    )
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

impl LodestoneUseCase {
    pub async fn has_inspected_profile(&self) -> Result<bool, CoreError> {
        Ok(!inspected_profiles(&self.list_active_profiles().await?).is_empty())
    }

    pub async fn inspect_due_domains<R, C>(
        &self,
        runners: &[R],
        settings: InspectionRunSettings,
        clock: &C,
    ) -> Result<InspectionReport, SourceRunError>
    where
        R: ModuleRunner,
        C: Clock,
    {
        let mut report = InspectionReport::default();
        let profiles = self.list_active_profiles().await?;
        let inspected = inspected_profiles(&profiles);
        if inspected.is_empty() {
            return Ok(report);
        }
        let ids: Vec<ProfileId> = inspected.iter().map(|active| active.profile.id).collect();
        for runner in runners {
            let module = runner.module();
            let due = self
                .due_ecosystem_domains(module, &ids, clock.now(), settings.batch_size)
                .await?;
            let mut run = ModuleRun {
                module,
                ran_at: clock.now(),
                leads_inspected: 0,
                findings_count: 0,
                latest_example: None,
            };
            for item in &due {
                let now = clock.now();
                let outcome = if not_applicable(module, &item.domain) {
                    ModuleOutcome::NotApplicable
                } else {
                    let merged = settings_for(&inspected, item);
                    let dossier = if module == EcosystemModule::OfficialIdentity {
                        self.company_dossier(&item.domain).await?
                    } else {
                        None
                    };
                    let target = InspectionTarget {
                        domain: &item.domain,
                        settings: &merged,
                        dossier: dossier.as_ref(),
                        now,
                    };
                    match runner.inspect(&target).await {
                        Ok(findings) => ModuleOutcome::Done(findings),
                        Err(SourceError::RateLimited { retry_after }) => {
                            report.rate_limited = true;
                            self.postpone_ecosystem(
                                &item.domain,
                                module,
                                now + pause_of(retry_after),
                            )
                            .await?;
                            info!(module = module.as_str(), domain = %item.domain, ?retry_after, "ecosystem inspection paused, host rate limited");
                            break;
                        }
                        Err(error) => {
                            debug!(module = module.as_str(), domain = %item.domain, %error, "ecosystem module failed");
                            ModuleOutcome::Failed(bounded_error(&error.to_string()))
                        }
                    }
                };
                let next_run = match outcome {
                    ModuleOutcome::Failed(_) => now + module.failure_retry(),
                    ModuleOutcome::Done(_) | ModuleOutcome::NotApplicable => {
                        now + module.refresh_interval()
                    }
                };
                self.save_ecosystem_outcome(&item.domain, module, &outcome, now, next_run)
                    .await?;
                report.domains += 1;
                match &outcome {
                    ModuleOutcome::Done(findings) => {
                        report.findings += count(findings.len());
                        run.findings_count += count(findings.len());
                        if let Some(first) = findings.first() {
                            run.latest_example = Some(first.example(&item.domain));
                        }
                    }
                    ModuleOutcome::NotApplicable => report.not_applicable += 1,
                    ModuleOutcome::Failed(_) => report.failed += 1,
                }
                run.leads_inspected += item.leads;
            }
            if run.leads_inspected > 0 {
                self.record_module_run(&run).await?;
                info!(
                    module = module.as_str(),
                    leads = run.leads_inspected,
                    findings = run.findings_count,
                    "ecosystem module pass finished"
                );
            }
        }
        let modules: Vec<EcosystemModule> = runners.iter().map(ModuleRunner::module).collect();
        report.backlog = self
            .count_due_ecosystem(&modules, &ids, clock.now())
            .await?;
        Ok(report)
    }

    pub async fn dry_run_ecosystem_module<R: ModuleRunner>(
        &self,
        runners: &[R],
        lead_id: LeadId,
        module: EcosystemModule,
        now: DateTime<Utc>,
    ) -> Result<DryRun, DryRunError> {
        let (lead, definition) = self.lead_for_inspection(lead_id).await?;
        let runner = runners
            .iter()
            .find(|runner| runner.module() == module)
            .ok_or(DryRunError::Unavailable(module.as_str()))?;
        let started = Instant::now();
        let platform = hosting_platform(&lead.domain);
        let mut dry_run = DryRun {
            module,
            domain: lead.domain.clone(),
            status: ModuleStatus::NotApplicable,
            findings: Vec::new(),
            hosting_platform: platform,
            error: None,
            duration: StdDuration::ZERO,
        };
        if not_applicable(module, &lead.domain) {
            dry_run.duration = started.elapsed();
            return Ok(dry_run);
        }
        let settings = definition
            .as_ref()
            .map(|definition| InspectionSettings::of([definition]))
            .unwrap_or_default();
        let target = InspectionTarget {
            domain: &lead.domain,
            settings: &settings,
            dossier: None,
            now,
        };
        match runner.inspect(&target).await {
            Ok(findings) => {
                dry_run.status = ModuleStatus::Done;
                dry_run.findings = findings;
            }
            Err(SourceError::RateLimited { retry_after }) => {
                return Err(DryRunError::RateLimited { retry_after });
            }
            Err(error) => {
                dry_run.status = ModuleStatus::Failed;
                dry_run.error = Some(bounded_error(&error.to_string()));
            }
        }
        dry_run.duration = started.elapsed();
        Ok(dry_run)
    }

    pub async fn ecosystem_stats(&self, now: DateTime<Utc>) -> Result<Vec<ModuleStats>, CoreError> {
        self.module_stats(now - Duration::days(STATS_WINDOW_DAYS))
            .await
    }

    #[transactional(lead, suppression, profile)]
    async fn lead_for_inspection(
        &self,
        lead_id: LeadId,
    ) -> Result<(Lead, Option<ProfileDefinition>), CoreError> {
        let mut leads = lead_repository;
        let mut suppression = suppression_repository;
        let mut profiles = profile_repository;
        let lead = leads.find(lead_id).await?.ok_or(CoreError::NotFound)?;
        if suppression.is_domain_suppressed(&lead.domain).await? {
            return Err(CoreError::NotFound);
        }
        let definition = profiles
            .list_revisions(lead.profile_id)
            .await?
            .into_iter()
            .max_by_key(|revision| revision.number)
            .map(|revision| revision.definition.definition().clone());
        Ok((lead, definition))
    }

    #[transactional(ecosystem)]
    async fn due_ecosystem_domains(
        &self,
        module: EcosystemModule,
        profiles: &[ProfileId],
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<DueDomain>, CoreError> {
        let mut repo = ecosystem_repository;
        repo.due_domains(module, profiles, now, limit).await
    }

    #[transactional(ecosystem)]
    async fn count_due_ecosystem(
        &self,
        modules: &[EcosystemModule],
        profiles: &[ProfileId],
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut repo = ecosystem_repository;
        repo.count_due(modules, profiles, now).await
    }

    #[transactional(ecosystem)]
    async fn save_ecosystem_outcome(
        &self,
        domain: &NormalizedDomain,
        module: EcosystemModule,
        outcome: &ModuleOutcome,
        ran_at: DateTime<Utc>,
        next_run_at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut repo = ecosystem_repository;
        repo.save_outcome(domain, module, outcome, ran_at, next_run_at)
            .await
    }

    #[transactional(ecosystem)]
    async fn postpone_ecosystem(
        &self,
        domain: &NormalizedDomain,
        module: EcosystemModule,
        next_run_at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut repo = ecosystem_repository;
        repo.postpone(domain, module, next_run_at).await
    }

    #[transactional(ecosystem)]
    async fn record_module_run(&self, run: &ModuleRun) -> Result<(), CoreError> {
        let mut repo = ecosystem_repository;
        repo.record_run(run).await
    }

    #[transactional(ecosystem)]
    async fn module_stats(&self, since: DateTime<Utc>) -> Result<Vec<ModuleStats>, CoreError> {
        let mut repo = ecosystem_repository;
        repo.stats(since).await
    }

    #[transactional(ecosystem)]
    pub async fn domain_ecosystem(
        &self,
        domain: &NormalizedDomain,
    ) -> Result<DomainEcosystem, CoreError> {
        let mut repo = ecosystem_repository;
        repo.of_domain(domain).await
    }
}
