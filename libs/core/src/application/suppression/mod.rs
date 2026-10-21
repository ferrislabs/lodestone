use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::transactional;

use crate::{
    UserId,
    application::LodestoneUseCase,
    domain::{
        NormalizedDomain,
        ecosystem::{SAAS_TENANT_DOMAINS, SAAS_TENANT_REASON},
        profile::ProfileId,
        suppression::{
            SuppressionEntry, SuppressionId, SuppressionTarget, ports::SuppressionRepository,
        },
        values::EmailAddress,
    },
};

impl LodestoneUseCase {
    pub async fn exclude_saas_tenant_leads(&self, now: DateTime<Utc>) -> Result<u64, CoreError> {
        let profiles: Vec<ProfileId> = self
            .list_active_profiles()
            .await?
            .iter()
            .map(|active| active.profile.id)
            .collect();
        self.exclude_tenant_leads(&profiles, now).await
    }

    #[transactional(suppression)]
    async fn exclude_tenant_leads(
        &self,
        profiles: &[ProfileId],
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut repo = suppression_repository;
        repo.exclude_leads_under(profiles, &SAAS_TENANT_DOMAINS, SAAS_TENANT_REASON, now)
            .await
    }

    #[transactional(suppression)]
    pub async fn add_suppression(
        &self,
        target: SuppressionTarget,
        reason: Option<String>,
        created_by: Option<UserId>,
    ) -> Result<SuppressionEntry, CoreError> {
        let mut repo = suppression_repository;
        let entry = SuppressionEntry {
            id: SuppressionId(generate_uuid_v7()),
            target,
            reason,
            created_by,
            created_at: Utc::now(),
        };
        repo.insert(&entry).await?;
        Ok(entry)
    }

    #[transactional(suppression)]
    pub async fn remove_suppression(&self, id: SuppressionId) -> Result<(), CoreError> {
        let mut repo = suppression_repository;
        if repo.remove(id).await? {
            Ok(())
        } else {
            Err(CoreError::NotFound)
        }
    }

    #[transactional(suppression)]
    pub async fn list_suppressions(&self) -> Result<Vec<SuppressionEntry>, CoreError> {
        let mut repo = suppression_repository;
        repo.list().await
    }

    #[transactional(suppression)]
    pub async fn is_suppressed(
        &self,
        domain: &NormalizedDomain,
        email: Option<&EmailAddress>,
    ) -> Result<bool, CoreError> {
        let mut repo = suppression_repository;
        if repo.is_domain_suppressed(domain).await? {
            return Ok(true);
        }
        match email {
            Some(email) => repo.is_email_suppressed(email).await,
            None => Ok(false),
        }
    }

    pub async fn ensure_not_suppressed(
        &self,
        domain: &NormalizedDomain,
        email: Option<&EmailAddress>,
    ) -> Result<(), CoreError> {
        if self.is_suppressed(domain, email).await? {
            Err(CoreError::Forbidden {
                reason: Some("target is on the suppression list".to_owned()),
            })
        } else {
            Ok(())
        }
    }
}
