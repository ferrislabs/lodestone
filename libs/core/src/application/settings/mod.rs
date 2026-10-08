use chrono::{DateTime, Utc};
use common::{CoreError, RobotsPolicy};
use lodestone_macros::transactional;

use crate::{
    application::LodestoneUseCase,
    domain::{
        UserId,
        company::ports::CompanyRepository,
        settings::{CrawlSettings, ROBOTS_POLICY_KEY, ports::OrganisationSettingRepository},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrawlSettingsUpdate {
    pub settings: CrawlSettings,
    pub dossiers_retried: u64,
}

impl LodestoneUseCase {
    #[transactional(organisation_setting)]
    pub async fn crawl_settings(&self, seed: RobotsPolicy) -> Result<CrawlSettings, CoreError> {
        let mut settings = organisation_setting_repository;
        let stored = settings.find(ROBOTS_POLICY_KEY).await?;
        Ok(CrawlSettings::effective(stored, seed))
    }

    #[transactional(organisation_setting, company)]
    pub async fn update_crawl_settings(
        &self,
        policy: RobotsPolicy,
        user: UserId,
        now: DateTime<Utc>,
        seed: RobotsPolicy,
    ) -> Result<CrawlSettingsUpdate, CoreError> {
        let mut settings = organisation_setting_repository;
        let mut company = company_repository;
        settings
            .save(ROBOTS_POLICY_KEY, policy.as_str(), user, now)
            .await?;
        let dossiers_retried = if policy.ignores_robots() {
            company.retry_robots_unreachable(now).await?
        } else {
            0
        };
        let stored = settings.find(ROBOTS_POLICY_KEY).await?;
        Ok(CrawlSettingsUpdate {
            settings: CrawlSettings::effective(stored, seed),
            dossiers_retried,
        })
    }
}
