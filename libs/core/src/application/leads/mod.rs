use chrono::Utc;
use common::CoreError;
use lodestone_macros::transactional;

use crate::{
    UserId,
    application::{LodestoneUseCase, output::schedule_webhook},
    domain::{
        lead::{
            Lead, LeadId, LeadStatus,
            ports::{AssessmentRepository, ContactRepository, LeadRepository, SignalRepository},
            query::{LeadDetail, LeadListing, LeadQuery, ProfileSummary},
        },
        profile::{ProfileId, ports::ProfileRepository},
        qualification::ports::QualificationRepository,
        suppression::ports::SuppressionRepository,
    },
    infrastructure::job::postgres::PgJobOutbox,
};

impl LodestoneUseCase {
    #[transactional(lead, profile)]
    pub async fn list_leads(
        &self,
        profile_id: ProfileId,
        query: LeadQuery,
    ) -> Result<LeadListing, CoreError> {
        let mut leads = lead_repository;
        let mut profiles = profile_repository;
        let profile = profiles
            .find(profile_id)
            .await?
            .ok_or(CoreError::NotFound)?;
        let page = leads.list_page(profile_id, &query).await?;
        Ok(LeadListing { profile, page })
    }

    #[transactional(lead, profile, suppression, signal, contact, assessment, qualification)]
    pub async fn lead_detail(&self, lead_id: LeadId) -> Result<LeadDetail, CoreError> {
        let mut leads = lead_repository;
        let mut profiles = profile_repository;
        let mut suppression = suppression_repository;
        let mut signals = signal_repository;
        let mut contacts = contact_repository;
        let mut assessments = assessment_repository;
        let mut qualification = qualification_repository;

        let lead = leads.find(lead_id).await?.ok_or(CoreError::NotFound)?;
        if suppression.is_domain_suppressed(&lead.domain).await? {
            return Err(CoreError::NotFound);
        }
        let profile = profiles
            .find(lead.profile_id)
            .await?
            .ok_or(CoreError::NotFound)?;
        let qualification_results = qualification.current_results(lead_id).await?;
        let qualification_agents = match qualification_results.first() {
            Some(first) => profiles
                .list_revisions(lead.profile_id)
                .await?
                .into_iter()
                .find(|revision| revision.number == first.profile_revision)
                .map(|revision| {
                    revision
                        .definition
                        .definition()
                        .qualification_agents
                        .clone()
                })
                .unwrap_or_default(),
            None => Vec::new(),
        };
        Ok(LeadDetail {
            qualification_results,
            qualification_agents,
            signals: signals.list_for_lead(lead_id).await?,
            contacts: contacts.list_for_lead(lead_id).await?,
            assessments: assessments.list_for_lead(lead_id).await?,
            lead,
            profile,
        })
    }

    #[transactional(lead, suppression)]
    pub async fn change_lead_status(
        &self,
        lead_id: LeadId,
        to: LeadStatus,
        actor: UserId,
    ) -> Result<Lead, CoreError> {
        let mut leads = lead_repository;
        let mut suppression = suppression_repository;

        let lead = leads.find(lead_id).await?.ok_or(CoreError::NotFound)?;
        if suppression.is_domain_suppressed(&lead.domain).await? {
            return Err(CoreError::NotFound);
        }
        let from = lead.status;
        let target = from.move_to(to).map_err(|_| {
            CoreError::Conflict(format!(
                "lead cannot move from {} to {}",
                from.as_str(),
                to.as_str()
            ))
        })?;
        let updated = leads
            .update_status(lead_id, from, target, Utc::now())
            .await?
            .ok_or_else(|| {
                CoreError::Conflict("lead status changed concurrently, reload and retry".to_owned())
            })?;
        if target == LeadStatus::Validated {
            schedule_webhook(&mut PgJobOutbox::new(&tx), self.webhooks.as_ref(), &updated).await?;
        }
        tracing::info!(lead.id = %lead_id, actor = %actor.0, from = from.as_str(), to = target.as_str(), "lead status changed");
        Ok(updated)
    }

    #[transactional(lead)]
    pub async fn list_profiles_summary(&self) -> Result<Vec<ProfileSummary>, CoreError> {
        let mut leads = lead_repository;
        leads.profile_summaries().await
    }
}
