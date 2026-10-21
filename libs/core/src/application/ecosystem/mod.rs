use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration as StdDuration, Instant},
};

use chrono::{DateTime, Duration, Utc};
use common::CoreError;
use futures_util::{StreamExt, future::join_all, stream};
use lodestone_macros::transactional;
use serde_json::json;
use thiserror::Error;
use tracing::{debug, info};

use crate::{
    application::{
        LodestoneUseCase,
        sources::SourceRunError,
        timeline::{in_flight::Subject, module_ended, module_span, requalification_planned},
    },
    domain::{
        NormalizedDomain,
        company::{CompanyDossier, Siren, bounded_error},
        ecosystem::{
            DomainEcosystem, DueBatch, DueDomain, EcosystemFinding, EcosystemModule,
            EcosystemReadiness, InspectionSettings, InspectionTarget, ModuleOutcome, ModuleRun,
            ModuleRunner, ModuleStats, ModuleStatus, STATS_WINDOW_DAYS, SirenLookup,
            hosting_platform, ports::EcosystemRepository,
        },
        job::ports::Clock,
        lead::{Lead, LeadId, ports::LeadRepository},
        profile::{
            ActiveProfile, ProfileId, definition::ProfileDefinition, ports::ProfileRepository,
        },
        qualification::ports::QualificationRepository,
        source::SourceError,
        suppression::ports::SuppressionRepository,
        timeline::{
            EventActor, EventStatus, TimelineEntry, TimelineStep, kind, ports::TimelineRepository,
        },
    },
};

pub const ECOSYSTEM_LISTENER: &str = "ecosystem_inspection";
const MIN_RATE_LIMIT_PAUSE: Duration = Duration::minutes(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectionRunSettings {
    pub batch_size: u32,
    pub priority_batch_size: u32,
    pub concurrency: NonZeroUsize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InspectionReport {
    pub domains: u64,
    pub findings: u64,
    pub failed: u64,
    pub not_applicable: u64,
    pub changed: u64,
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
    #[error("Fournisseur non prêt : {0}")]
    ProviderNotReady(&'static str),
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

struct Planned<'a, R> {
    runner: &'a R,
    module: EcosystemModule,
    due: Vec<DueDomain>,
    paused: AtomicBool,
}

#[derive(Clone, Copy)]
struct Visit<'a> {
    plan: usize,
    position: usize,
    item: &'a DueDomain,
}

enum Inspected {
    Skipped,
    RateLimited,
    Ran {
        outcome: ModuleOutcome,
        changed: bool,
    },
}

fn by_domain<'a, R>(planned: &'a [Planned<'_, R>]) -> Vec<Vec<Visit<'a>>> {
    let mut index: HashMap<&NormalizedDomain, usize> = HashMap::new();
    let mut domains: Vec<Vec<Visit<'a>>> = Vec::new();
    for (plan, planned) in planned.iter().enumerate() {
        for (position, item) in planned.due.iter().enumerate() {
            let visit = Visit {
                plan,
                position,
                item,
            };
            match index.get(&item.domain) {
                Some(&slot) => domains[slot].push(visit),
                None => {
                    index.insert(&item.domain, domains.len());
                    domains.push(vec![visit]);
                }
            }
        }
    }
    domains
}

struct Tally {
    run: ModuleRun,
    latest: Option<usize>,
}

#[derive(Default)]
struct RegistryContext {
    dossier: Option<CompanyDossier>,
    recorded: Option<Siren>,
    identity_settled: bool,
    previous: Vec<EcosystemFinding>,
}

impl RegistryContext {
    fn lookup(&self) -> SirenLookup<'_> {
        SirenLookup::of(
            self.dossier.as_ref(),
            self.recorded.as_ref(),
            self.identity_settled,
        )
    }
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
        let readiness = self.ecosystem_readiness().await?;
        let mut ready: Vec<EcosystemModule> = Vec::new();
        let mut planned: Vec<Planned<'_, R>> = Vec::new();
        for runner in runners {
            let module = runner.module();
            if !readiness.is_ready(module) {
                info!(
                    module = module.as_str(),
                    provider = module.provider().unwrap_or_default(),
                    "ecosystem module skipped, provider not ready"
                );
                continue;
            }
            ready.push(module);
            let batch = DueBatch {
                priority: settings.priority_batch_size,
                refresh: settings.batch_size,
            };
            let due = self
                .due_ecosystem_domains(module, &ids, clock.now(), batch)
                .await?;
            planned.push(Planned {
                runner,
                module,
                due,
                paused: AtomicBool::new(false),
            });
        }
        let mut tallies: Vec<Tally> = planned
            .iter()
            .map(|plan| Tally {
                run: ModuleRun {
                    module: plan.module,
                    ran_at: clock.now(),
                    leads_inspected: 0,
                    findings_count: 0,
                    latest_example: None,
                },
                latest: None,
            })
            .collect();
        let plans = planned.as_slice();
        let inspected = inspected.as_slice();
        let grouped = &by_domain(plans);
        let mut domains = stream::iter(0..grouped.len())
            .map(move |slot| async move {
                let visits = &grouped[slot];
                let mut done = Vec::with_capacity(visits.len());
                for waits in [false, true] {
                    let stage: Vec<usize> = (0..visits.len())
                        .filter(|&index| {
                            plans[visits[index].plan].module.waits_for_identity() == waits
                        })
                        .collect();
                    let results = join_all(stage.into_iter().map(move |index| async move {
                        let visit = visits[index];
                        self.inspect_visit(&plans[visit.plan], visit.item, inspected, clock)
                            .await
                            .map(|result| (visit, result))
                    }))
                    .await;
                    for result in results {
                        done.push(result?);
                    }
                }
                Ok::<_, CoreError>(done)
            })
            .buffered(settings.concurrency.get());
        while let Some(done) = domains.next().await {
            for (visit, result) in done? {
                let (outcome, changed) = match result {
                    Inspected::Skipped => continue,
                    Inspected::RateLimited => {
                        report.rate_limited = true;
                        continue;
                    }
                    Inspected::Ran { outcome, changed } => (outcome, changed),
                };
                let tally = &mut tallies[visit.plan];
                if changed {
                    report.changed += 1;
                }
                report.domains += 1;
                match &outcome {
                    ModuleOutcome::Done(findings) => {
                        report.findings += count(findings.len());
                        tally.run.findings_count += count(findings.len());
                        if let Some(first) = findings.first()
                            && tally.latest.is_none_or(|latest| latest < visit.position)
                        {
                            tally.run.latest_example = Some(first.example(&visit.item.domain));
                            tally.latest = Some(visit.position);
                        }
                    }
                    ModuleOutcome::NotApplicable(_) => report.not_applicable += 1,
                    ModuleOutcome::Failed(_) => report.failed += 1,
                }
                tally.run.leads_inspected += visit.item.leads;
            }
        }
        for Tally { run, .. } in &tallies {
            if run.leads_inspected > 0 {
                self.record_module_run(run).await?;
                info!(
                    module = run.module.as_str(),
                    leads = run.leads_inspected,
                    findings = run.findings_count,
                    "ecosystem module pass finished"
                );
            }
        }
        report.backlog = self.count_due_ecosystem(&ready, &ids, clock.now()).await?;
        Ok(report)
    }

    async fn inspect_visit<R, C>(
        &self,
        plan: &Planned<'_, R>,
        item: &DueDomain,
        inspected: &[&ActiveProfile],
        clock: &C,
    ) -> Result<Inspected, CoreError>
    where
        R: ModuleRunner,
        C: Clock,
    {
        if plan.paused.load(Ordering::Acquire) {
            return Ok(Inspected::Skipped);
        }
        let module = plan.module;
        let now = clock.now();
        let span = module_span(module, now);
        let outcome = if not_applicable(module, &item.domain) {
            ModuleOutcome::NotApplicable(None)
        } else {
            let actor = EventActor::module(module.as_str());
            let _running = self.enter_step(
                Subject::Domain(item.domain.clone()),
                TimelineStep::Inspection,
                actor.clone(),
                now,
            );
            self.record_domain_timeline(
                &item.domain,
                &TimelineEntry::started(TimelineStep::Inspection, kind::MODULE, now, &span)
                    .actor(actor),
            )
            .await?;
            let merged = settings_for(inspected, item);
            let registry = self.registry_context(module, &item.domain).await?;
            let target = InspectionTarget {
                domain: &item.domain,
                settings: &merged,
                dossier: registry.dossier.as_ref(),
                siren: registry.lookup(),
                previous: &registry.previous,
                now,
            };
            match plan.runner.inspect(&target).await {
                Ok(inspection) => inspection.outcome(),
                Err(SourceError::RateLimited { retry_after }) => {
                    plan.paused.store(true, Ordering::Release);
                    let next_run_at = now + pause_of(retry_after);
                    let waiting = TimelineEntry::ended(
                        TimelineStep::Inspection,
                        kind::MODULE,
                        EventStatus::Waiting,
                        (&span, now),
                        clock.now(),
                    )
                    .actor(EventActor::module(module.as_str()))
                    .detail(json!({ "rate_limited": true, "next_run_at": next_run_at }));
                    self.postpone_ecosystem(&item.domain, module, next_run_at, &waiting)
                        .await?;
                    info!(module = module.as_str(), domain = %item.domain, ?retry_after, "ecosystem inspection paused, host rate limited");
                    return Ok(Inspected::RateLimited);
                }
                Err(error) => {
                    debug!(module = module.as_str(), domain = %item.domain, %error, "ecosystem module failed");
                    ModuleOutcome::Failed(bounded_error(&error.to_string()))
                }
            }
        };
        let next_run = match outcome {
            ModuleOutcome::Failed(_) => now + module.failure_retry(),
            ModuleOutcome::Done(_) | ModuleOutcome::NotApplicable(_) => {
                now + module.refresh_interval()
            }
        };
        let changed = self
            .save_ecosystem_outcome(&item.domain, module, &outcome, (now, clock.now()), next_run)
            .await?;
        Ok(Inspected::Ran { outcome, changed })
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
        if let Some(provider) = module.provider()
            && !self.is_provider_usable(provider).await?
        {
            return Err(DryRunError::ProviderNotReady(provider));
        }
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
        let registry = self.registry_context(module, &lead.domain).await?;
        let target = InspectionTarget {
            domain: &lead.domain,
            settings: &settings,
            dossier: registry.dossier.as_ref(),
            siren: registry.lookup(),
            previous: &registry.previous,
            now,
        };
        match runner.inspect(&target).await {
            Ok(inspection) => match inspection.outcome() {
                ModuleOutcome::Done(findings) => {
                    dry_run.status = ModuleStatus::Done;
                    dry_run.findings = findings;
                }
                outcome => {
                    dry_run.status = outcome.status();
                    dry_run.error = outcome.note().map(str::to_owned);
                }
            },
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

    async fn registry_context(
        &self,
        module: EcosystemModule,
        domain: &NormalizedDomain,
    ) -> Result<RegistryContext, CoreError> {
        let dossier = if module.needs_siren() {
            self.company_dossier(domain).await?
        } else {
            None
        };
        let reads_records = module.keeps_history() || module.waits_for_identity();
        if !reads_records {
            return Ok(RegistryContext {
                dossier,
                ..RegistryContext::default()
            });
        }
        let recorded = self.domain_ecosystem(domain).await?;
        let siren = recorded.recorded_siren();
        let identity_settled = recorded.identity_settled();
        let previous = recorded
            .findings
            .into_iter()
            .filter(|finding| finding.module == module)
            .collect();
        Ok(RegistryContext {
            dossier,
            recorded: siren,
            identity_settled,
            previous,
        })
    }

    pub async fn ecosystem_readiness(&self) -> Result<EcosystemReadiness, CoreError> {
        let mut not_ready = Vec::new();
        for module in EcosystemModule::AVAILABLE {
            if let Some(provider) = module.provider()
                && !self.is_provider_usable(provider).await?
            {
                not_ready.push(module);
            }
        }
        Ok(EcosystemReadiness::with_not_ready(not_ready))
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
        batch: DueBatch,
    ) -> Result<Vec<DueDomain>, CoreError> {
        let mut repo = ecosystem_repository;
        repo.due_domains(module, profiles, now, batch).await
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

    #[transactional(ecosystem, qualification, timeline)]
    async fn save_ecosystem_outcome(
        &self,
        domain: &NormalizedDomain,
        module: EcosystemModule,
        outcome: &ModuleOutcome,
        (ran_at, finished_at): (DateTime<Utc>, DateTime<Utc>),
        next_run_at: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let mut repo = ecosystem_repository;
        let mut qualification = qualification_repository;
        let mut timeline = timeline_repository;
        let changed = repo
            .save_outcome(domain, module, outcome, ran_at, next_run_at)
            .await?;
        let span = module_span(module, ran_at);
        timeline
            .record_for_domain(
                domain,
                &module_ended(module, outcome, (&span, ran_at), finished_at, changed),
            )
            .await?;
        if changed {
            let after = self.requalify.requalify_after(ran_at);
            let marked = qualification
                .mark_requalification(domain, after, ran_at)
                .await?;
            timeline
                .record_for_domain(
                    domain,
                    &requalification_planned(
                        after,
                        finished_at,
                        "inspection",
                        Some(module.as_str()),
                    ),
                )
                .await?;
            debug!(module = module.as_str(), %domain, leads = marked, "new findings, requalification planned");
        }
        Ok(changed)
    }

    #[transactional(ecosystem, timeline)]
    async fn postpone_ecosystem(
        &self,
        domain: &NormalizedDomain,
        module: EcosystemModule,
        next_run_at: DateTime<Utc>,
        waiting: &TimelineEntry,
    ) -> Result<(), CoreError> {
        let mut repo = ecosystem_repository;
        let mut timeline = timeline_repository;
        repo.postpone(domain, module, next_run_at).await?;
        timeline.record_for_domain(domain, waiting).await.map(drop)
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
