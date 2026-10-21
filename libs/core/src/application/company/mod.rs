use std::{num::NonZeroU16, time::Duration};

use chrono::{DateTime, Duration as Delta, Utc};
use common::CoreError;
use lodestone_macros::transactional;
use serde_json::json;
use tracing::{debug, info};

use crate::{
    application::{LodestoneUseCase, sources::SourceRunError, timeline::requalification_planned},
    domain::{
        NormalizedDomain,
        company::{
            CompanyBuilder, CompanyDossier, DossierBuild, DossierState, PageReview, PageVerifier,
            bounded_error, ports::CompanyRepository, settle,
        },
        ecosystem::hosting_platform,
        facts::country_of,
        fetch::link::UnconfirmedLinks,
        job::{JobKind, NewJob, ports::Clock},
        lead::{
            LeadId,
            ports::{LeadRepository, SignalRepository},
        },
        profile::ActiveProfile,
        qualification::ports::QualificationRepository,
        source::SourceError,
        suppression::ports::SuppressionRepository,
        territory::{OUT_OF_COUNTRY_REASON, out_of_country},
        timeline::ports::TimelineRepository,
    },
};

pub const COMPANY_RESEARCH_LISTENER: &str = "company_research";
pub const PAGE_RECHECK_JOB_KIND: &str = "company.recheck_pages";
const PAGE_RECHECK_DEDUP_KEY: &str = "company:recheck_pages";
const PAGE_RECHECK_MAX_ATTEMPTS: NonZeroU16 = NonZeroU16::new(5).unwrap();
const MIN_RATE_LIMIT_PAUSE: Delta = Delta::minutes(1);

pub fn page_recheck_job(run_at: DateTime<Utc>) -> Result<NewJob, CoreError> {
    let kind = JobKind::parse(PAGE_RECHECK_JOB_KIND)
        .map_err(|_| CoreError::Internal("invalid page recheck job kind".to_owned()))?;
    Ok(NewJob {
        kind,
        payload: json!({}),
        run_at,
        max_attempts: PAGE_RECHECK_MAX_ATTEMPTS,
        dedup_key: Some(PAGE_RECHECK_DEDUP_KEY.to_owned()),
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageRecheckReport {
    pub dossiers: u64,
    pub changed: u64,
    pub unconfirmed_links: UnconfirmedLinks,
    pub remaining: u64,
    pub retry_after: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompanyRunSettings {
    pub batch_size: u32,
    pub refresh_interval: Duration,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompanyRunReport {
    pub domains: u64,
    pub changed: u64,
    pub failed: u64,
    pub excluded_out_of_country: u64,
    pub backlog: u64,
    pub rate_limited: bool,
    pub unconfirmed_links: UnconfirmedLinks,
}

fn failed_build(error: &SourceError) -> DossierBuild {
    DossierBuild {
        siren: None,
        official: None,
        pages: Vec::new(),
        reachable: false,
        failure: Some(bounded_error(&error.to_string())),
        unconfirmed: UnconfirmedLinks::default(),
    }
}

impl LodestoneUseCase {
    pub async fn listen_company_research<B, C>(
        &self,
        builder: &B,
        settings: CompanyRunSettings,
        clock: &C,
    ) -> Result<CompanyRunReport, SourceRunError>
    where
        B: CompanyBuilder,
        C: Clock,
    {
        let refresh = Delta::from_std(settings.refresh_interval)
            .map_err(|_| CoreError::Internal("refresh interval out of range".to_owned()))?;
        let mut report = CompanyRunReport::default();
        let domains = self
            .due_company_domains(clock.now(), settings.batch_size)
            .await?;
        for domain in &domains {
            if hosting_platform(domain).is_some() {
                self.save_company_dossier(&CompanyDossier::on_hosting_platform(
                    domain.clone(),
                    clock.now(),
                    refresh,
                ))
                .await?;
                report.domains += 1;
                continue;
            }
            let previous = self.company_dossier(domain).await?;
            let built = match builder.build(domain).await {
                Ok(built) => built,
                Err(SourceError::RateLimited { retry_after }) => {
                    report.rate_limited = true;
                    let pause = Delta::from_std(retry_after)
                        .unwrap_or(MIN_RATE_LIMIT_PAUSE)
                        .max(MIN_RATE_LIMIT_PAUSE);
                    let held = previous
                        .unwrap_or_else(|| CompanyDossier::pending(domain.clone(), clock.now()))
                        .deferred(clock.now() + pause);
                    self.save_company_dossier(&held).await?;
                    break;
                }
                Err(error) => {
                    debug!(%domain, %error, "company research failed");
                    failed_build(&error)
                }
            };
            let now = clock.now();
            report.unconfirmed_links.merge(&built.unconfirmed);
            let dossier = settle(domain.clone(), built, previous.as_ref(), now, refresh);
            if dossier.error.is_some() {
                report.failed += 1;
            }
            let changed = dossier.content_at == Some(now);
            if changed {
                report.changed += 1;
            }
            self.save_company_dossier(&dossier).await?;
            if changed {
                self.mark_domain_requalification(domain, now).await?;
            }
            report.excluded_out_of_country += self.exclude_out_of_country(&dossier, now).await?;
            report.domains += 1;
            info!(
                %domain,
                state = dossier.state.as_str(),
                pages = dossier.pages.len(),
                ignored_links = dossier.ignored_links,
                official = dossier.official.is_some(),
                "company dossier stored"
            );
        }
        report.backlog = self.count_due_company_domains(clock.now()).await?;
        Ok(report)
    }

    pub async fn recheck_company_pages<V, C>(
        &self,
        verifier: &V,
        batch_size: u32,
        clock: &C,
    ) -> Result<PageRecheckReport, SourceRunError>
    where
        V: PageVerifier,
        C: Clock,
    {
        let mut report = PageRecheckReport::default();
        for domain in self.unchecked_company_domains(batch_size).await? {
            let Some(dossier) = self.company_dossier(&domain).await? else {
                continue;
            };
            let review = match verifier.review(&dossier.pages).await {
                Ok(review) => review,
                Err(SourceError::RateLimited { retry_after }) => {
                    report.retry_after = Some(retry_after);
                    break;
                }
                Err(error) => {
                    debug!(%domain, %error, "dossier pages could not be rechecked, they are kept");
                    PageReview::default()
                }
            };
            let reviewed = dossier.reviewed(&review, clock.now());
            if !self
                .save_company_review(&reviewed, dossier.built_at)
                .await?
            {
                continue;
            }
            report.dossiers += 1;
            report.unconfirmed_links.merge(&review.unconfirmed());
            if reviewed.content_at != dossier.content_at {
                report.changed += 1;
                self.mark_domain_requalification(&domain, clock.now())
                    .await?;
            }
            info!(
                %domain,
                kept = reviewed.pages.len(),
                removed = review.rejected.len(),
                "dossier pages rechecked"
            );
        }
        report.remaining = self.count_unchecked_company_pages().await?;
        Ok(report)
    }

    pub async fn settle_platform_dossiers<C: Clock>(
        &self,
        refresh_interval: Duration,
        clock: &C,
    ) -> Result<u64, CoreError> {
        let refresh = Delta::from_std(refresh_interval)
            .map_err(|_| CoreError::Internal("refresh interval out of range".to_owned()))?;
        let mut settled = 0;
        for domain in self
            .company_domains_except(DossierState::HostingPlatform)
            .await?
            .into_iter()
            .filter(|domain| hosting_platform(domain).is_some())
        {
            self.save_company_dossier(&CompanyDossier::on_hosting_platform(
                domain,
                clock.now(),
                refresh,
            ))
            .await?;
            settled += 1;
        }
        Ok(settled)
    }

    #[transactional(qualification, timeline)]
    pub(crate) async fn mark_domain_requalification(
        &self,
        domain: &NormalizedDomain,
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut qualification = qualification_repository;
        let mut timeline = timeline_repository;
        let after = self.requalify.requalify_after(now);
        let marked = qualification
            .mark_requalification(domain, after, now)
            .await?;
        if marked > 0 {
            timeline
                .record_for_domain(
                    domain,
                    &requalification_planned(after, now, "dossier", None),
                )
                .await?;
        }
        Ok(marked)
    }

    pub async fn exclude_out_of_country(
        &self,
        dossier: &CompanyDossier,
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let Some(head_office) = dossier.head_office_country_code() else {
            return Ok(0);
        };
        if country_of(&dossier.domain).is_some() {
            return Ok(0);
        }
        let profiles = self.list_active_profiles().await?;
        let excluded = self
            .exclude_leads_out_of(&dossier.domain, head_office, &profiles, now)
            .await?;
        if excluded > 0 {
            info!(domain = %dossier.domain, head_office, excluded, "leads outside the target countries suppressed");
        }
        Ok(excluded)
    }

    #[transactional(lead, signal, suppression, timeline)]
    async fn exclude_leads_out_of(
        &self,
        domain: &NormalizedDomain,
        head_office: &str,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut leads = lead_repository;
        let mut signals = signal_repository;
        let mut suppression = suppression_repository;
        let mut excluded: Vec<LeadId> = Vec::new();
        for lead in leads.list_by_domain(domain).await? {
            let Some(active) = profiles
                .iter()
                .find(|active| active.profile.id == lead.profile_id)
            else {
                continue;
            };
            let found = signals.list_for_lead(lead.id).await?;
            if out_of_country(
                active.revision.definition.definition(),
                domain,
                &found,
                head_office,
            ) {
                excluded.push(lead.id);
            }
        }
        let mut timeline = timeline_repository;
        let count = suppression
            .exclude_leads(&excluded, OUT_OF_COUNTRY_REASON, now)
            .await?;
        if count > 0 {
            timeline
                .record_exclusions(OUT_OF_COUNTRY_REASON, now)
                .await?;
        }
        Ok(count)
    }

    #[transactional(company)]
    async fn company_domains_except(
        &self,
        state: DossierState,
    ) -> Result<Vec<NormalizedDomain>, CoreError> {
        let mut repo = company_repository;
        repo.domains_except(state).await
    }

    #[transactional(company)]
    pub async fn count_unchecked_company_pages(&self) -> Result<u64, CoreError> {
        let mut repo = company_repository;
        repo.count_unchecked().await
    }

    #[transactional(company)]
    async fn unchecked_company_domains(
        &self,
        limit: u32,
    ) -> Result<Vec<NormalizedDomain>, CoreError> {
        let mut repo = company_repository;
        repo.unchecked_domains(limit).await
    }

    #[transactional(company)]
    async fn save_company_review(
        &self,
        dossier: &CompanyDossier,
        built_at: Option<DateTime<Utc>>,
    ) -> Result<bool, CoreError> {
        let mut repo = company_repository;
        repo.save_review(dossier, built_at).await
    }

    pub async fn request_company_refresh(
        &self,
        lead_id: LeadId,
    ) -> Result<CompanyDossier, CoreError> {
        self.request_company_refresh_at(lead_id, Utc::now()).await
    }

    #[transactional(lead, suppression, company)]
    pub async fn request_company_refresh_at(
        &self,
        lead_id: LeadId,
        now: DateTime<Utc>,
    ) -> Result<CompanyDossier, CoreError> {
        let mut leads = lead_repository;
        let mut suppression = suppression_repository;
        let mut company = company_repository;
        let lead = leads.find(lead_id).await?.ok_or(CoreError::NotFound)?;
        if suppression.is_domain_suppressed(&lead.domain).await? {
            return Err(CoreError::NotFound);
        }
        company.request_refresh(&lead.domain, now).await?;
        company
            .find(&lead.domain)
            .await?
            .ok_or_else(|| CoreError::Internal("company dossier vanished".to_owned()))
    }

    #[transactional(company)]
    pub async fn company_dossier(
        &self,
        domain: &NormalizedDomain,
    ) -> Result<Option<CompanyDossier>, CoreError> {
        let mut repo = company_repository;
        repo.find(domain).await
    }

    #[transactional(company)]
    async fn due_company_domains(
        &self,
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<NormalizedDomain>, CoreError> {
        let mut repo = company_repository;
        repo.due_domains(now, limit).await
    }

    #[transactional(company)]
    async fn count_due_company_domains(&self, now: DateTime<Utc>) -> Result<u64, CoreError> {
        let mut repo = company_repository;
        repo.count_due(now).await
    }

    #[transactional(company)]
    async fn save_company_dossier(&self, dossier: &CompanyDossier) -> Result<(), CoreError> {
        let mut repo = company_repository;
        repo.save(dossier).await
    }
}
