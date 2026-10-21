use chrono::Utc;
use common::CoreError;
use lodestone_macros::transactional;

use crate::{
    UserId,
    application::{LodestoneUseCase, output::schedule_webhook, quality::record_decisions},
    domain::{
        company::{CompanyDossier, ports::CompanyRepository},
        facts::LeadFacts,
        host::ports::KnownHostRepository,
        lead::{
            Lead, LeadId, LeadStatus, Signal,
            ports::{AssessmentRepository, ContactRepository, LeadRepository, SignalRepository},
            query::{LeadDetail, LeadListing, LeadQuery, ProfileSummary},
        },
        profile::{ProfileId, ports::ProfileRepository},
        qualification::ports::QualificationRepository,
        quality::DecisionKind,
        suppression::ports::SuppressionRepository,
    },
    infrastructure::job::postgres::PgJobOutbox,
};

impl LodestoneUseCase {
    #[transactional(lead, profile, assessment, qualification)]
    pub async fn list_leads(
        &self,
        profile_id: ProfileId,
        query: LeadQuery,
    ) -> Result<LeadListing, CoreError> {
        let mut leads = lead_repository;
        let mut profiles = profile_repository;
        let mut assessments = assessment_repository;
        let mut qualification = qualification_repository;
        let profile = profiles
            .find(profile_id)
            .await?
            .ok_or(CoreError::NotFound)?;
        let page = leads.list_page(profile_id, &query).await?;
        let ids: Vec<LeadId> = page.items.iter().map(|lead| lead.id).collect();
        let summaries = assessments.latest_summaries(&ids).await?;
        let scorings = qualification.scorings(&ids).await?;
        Ok(LeadListing {
            profile,
            page,
            summaries,
            scorings,
        })
    }

    #[transactional(
        lead,
        profile,
        suppression,
        signal,
        known_host,
        contact,
        assessment,
        qualification,
        company
    )]
    pub async fn lead_detail(&self, lead_id: LeadId) -> Result<LeadDetail, CoreError> {
        let mut leads = lead_repository;
        let mut profiles = profile_repository;
        let mut suppression = suppression_repository;
        let mut signals = signal_repository;
        let mut known_hosts = known_host_repository;
        let mut contacts = contact_repository;
        let mut assessments = assessment_repository;
        let mut qualification = qualification_repository;
        let mut company = company_repository;

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
        let summary_state = qualification.summary_state(lead_id).await?;
        let scoring = qualification.scorings(&[lead_id]).await?.remove(&lead_id);
        let lead_signals = signals.list_for_lead(lead_id).await?;
        let hosts = known_hosts.list_for_domain(&lead.domain).await?;
        let dossier = company.find(&lead.domain).await?;
        Ok(LeadDetail {
            summary_state,
            scoring,
            qualification_results,
            qualification_agents,
            facts: LeadFacts::compute(&lead, &lead_signals, &hosts).with_dossier(dossier.as_ref()),
            company: dossier,
            signals: lead_signals,
            contacts: contacts.list_for_lead(lead_id).await?,
            assessments: assessments.list_for_lead(lead_id).await?,
            lead,
            profile,
        })
    }

    #[transactional(signal, known_host, company)]
    pub(crate) async fn signals_with_facts(
        &self,
        lead: &Lead,
    ) -> Result<(Vec<Signal>, LeadFacts, Option<CompanyDossier>), CoreError> {
        let mut signals = signal_repository;
        let mut known_hosts = known_host_repository;
        let mut company = company_repository;
        let found = signals.list_for_lead(lead.id).await?;
        let hosts = known_hosts.list_for_domain(&lead.domain).await?;
        let dossier = company.find(&lead.domain).await?;
        let facts = LeadFacts::compute(lead, &found, &hosts).with_dossier(dossier.as_ref());
        Ok((found, facts, dossier))
    }

    #[transactional(lead, suppression, qualification, profile, decision)]
    pub async fn change_lead_status(
        &self,
        lead_id: LeadId,
        to: LeadStatus,
        actor: UserId,
    ) -> Result<Lead, CoreError> {
        let mut leads = lead_repository;
        let mut suppression = suppression_repository;
        let mut qualification = qualification_repository;
        let mut profiles = profile_repository;
        let mut decisions = decision_repository;

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
        if let Some(kind) = DecisionKind::for_lead(target) {
            record_decisions(
                &mut decisions,
                &mut qualification,
                &mut profiles,
                &lead,
                &[kind],
                actor,
                updated.updated_at,
            )
            .await?;
        }
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
