use std::time::Duration;

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::transactional;
use tracing::{debug, info};

use crate::{
    application::{LodestoneUseCase, sources::SourceRunError},
    domain::{
        NormalizedDomain,
        company::{
            CompanyBuilder, CompanyDossier, DossierBuild, bounded_error, ports::CompanyRepository,
            settle,
        },
        job::ports::Clock,
        lead::{LeadId, ports::LeadRepository},
        source::SourceError,
        suppression::ports::SuppressionRepository,
    },
};

pub const COMPANY_RESEARCH_LISTENER: &str = "company_research";
const MIN_RATE_LIMIT_PAUSE: chrono::Duration = chrono::Duration::minutes(1);

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
    pub backlog: u64,
    pub rate_limited: bool,
}

fn failed_build(error: &SourceError) -> DossierBuild {
    DossierBuild {
        siren: None,
        official: None,
        pages: Vec::new(),
        reachable: false,
        failure: Some(bounded_error(&error.to_string())),
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
        let refresh = chrono::Duration::from_std(settings.refresh_interval)
            .map_err(|_| CoreError::Internal("refresh interval out of range".to_owned()))?;
        let mut report = CompanyRunReport::default();
        let domains = self
            .due_company_domains(clock.now(), settings.batch_size)
            .await?;
        for domain in &domains {
            let previous = self.company_dossier(domain).await?;
            let built = match builder.build(domain).await {
                Ok(built) => built,
                Err(SourceError::RateLimited { retry_after }) => {
                    report.rate_limited = true;
                    let pause = chrono::Duration::from_std(retry_after)
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
            let dossier = settle(domain.clone(), built, previous.as_ref(), now, refresh);
            if dossier.error.is_some() {
                report.failed += 1;
            }
            if dossier.content_at == Some(now) {
                report.changed += 1;
            }
            self.save_company_dossier(&dossier).await?;
            report.domains += 1;
            info!(
                %domain,
                state = dossier.state.as_str(),
                pages = dossier.pages.len(),
                official = dossier.official.is_some(),
                "company dossier stored"
            );
        }
        report.backlog = self.count_due_company_domains(clock.now()).await?;
        Ok(report)
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
