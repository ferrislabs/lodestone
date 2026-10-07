use chrono::Utc;
use common::CoreError;
use lodestone_macros::transactional;

use crate::{
    UserId,
    application::{
        LodestoneUseCase,
        output::schedule_webhook,
        quality::{Decided, record_decisions},
    },
    domain::{
        company::{CompanyDossier, CompanyEntity, SiblingLead, ports::CompanyRepository},
        facts::{LeadFacts, SiblingSignals},
        host::{CandidateHost, ports::KnownHostRepository},
        lead::{
            Lead, LeadId, LeadStatus, Signal,
            ports::{AssessmentRepository, ContactRepository, LeadRepository, SignalRepository},
            query::{LeadDetail, LeadListing, LeadQuery, ProfileSummary},
        },
        profile::{ProfileId, ports::ProfileRepository},
        qualification::ports::QualificationRepository,
        quality::{DecisionKind, Rejection},
        scoring_agent::ports::ScoringRepository,
        suppression::ports::SuppressionRepository,
    },
    infrastructure::job::postgres::PgJobOutbox,
};

pub(crate) struct CompanyContext {
    pub facts: LeadFacts,
    pub dossier: Option<CompanyDossier>,
    pub entity: Option<CompanyEntity>,
    pub siblings: Vec<SiblingLead>,
}

async fn company_context(
    signals: &mut impl SignalRepository,
    known_hosts: &mut impl KnownHostRepository,
    company: &mut impl CompanyRepository,
    lead: &Lead,
    own_signals: &[Signal],
) -> Result<CompanyContext, CoreError> {
    let dossier = company.find(&lead.domain).await?;
    let entity = company.entity_of(&lead.domain).await?;
    let siblings = company.siblings(lead.id).await?;
    let mut hosts: Vec<CandidateHost> = known_hosts.list_for_domain(&lead.domain).await?;
    let mut sibling_signals = Vec::new();
    for sibling in siblings
        .iter()
        .filter(|sibling| sibling.shares_profile(lead.profile_id))
    {
        hosts.extend(known_hosts.list_for_domain(&sibling.domain).await?);
        sibling_signals.push((&sibling.domain, signals.list_for_lead(sibling.id).await?));
    }
    let aggregated: Vec<SiblingSignals<'_>> = sibling_signals
        .iter()
        .map(|(domain, found)| SiblingSignals {
            domain,
            signals: found,
        })
        .collect();
    let facts = LeadFacts::compute_aggregated(lead, own_signals, &hosts, &aggregated)
        .with_dossier(dossier.as_ref());
    Ok(CompanyContext {
        facts,
        dossier,
        entity,
        siblings,
    })
}

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
        scoring_agent,
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
        let mut scoring_agents = scoring_agent_repository;
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
        let scoring_results = scoring_agents.current_results(lead_id).await?;
        let revision_number = qualification_results
            .first()
            .map(|result| result.profile_revision)
            .or_else(|| {
                scoring_results
                    .first()
                    .map(|result| result.profile_revision)
            });
        let (qualification_agents, scoring_agents) = match revision_number {
            Some(number) => profiles
                .list_revisions(lead.profile_id)
                .await?
                .into_iter()
                .find(|revision| revision.number == number)
                .map(|revision| {
                    let definition = revision.definition.definition();
                    (
                        definition.qualification_agents.clone(),
                        definition.scoring_agents.clone(),
                    )
                })
                .unwrap_or_default(),
            None => (Vec::new(), Vec::new()),
        };
        let summary_state = qualification.summary_state(lead_id).await?;
        let scoring = qualification.scorings(&[lead_id]).await?.remove(&lead_id);
        let lead_signals = signals.list_for_lead(lead_id).await?;
        let context = company_context(
            &mut signals,
            &mut known_hosts,
            &mut company,
            &lead,
            &lead_signals,
        )
        .await?;
        Ok(LeadDetail {
            summary_state,
            scoring,
            qualification_results,
            qualification_agents,
            scoring_results,
            scoring_agents,
            facts: context.facts,
            company: context.dossier,
            entity: context.entity,
            siblings: context.siblings,
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
        let context =
            company_context(&mut signals, &mut known_hosts, &mut company, lead, &found).await?;
        Ok((found, context.facts, context.dossier))
    }

    #[transactional(
        lead,
        suppression,
        qualification,
        scoring_agent,
        profile,
        decision,
        assessment
    )]
    pub async fn change_lead_status(
        &self,
        lead_id: LeadId,
        to: LeadStatus,
        actor: UserId,
        rejection: Option<Rejection>,
    ) -> Result<Lead, CoreError> {
        let mut leads = lead_repository;
        let mut assessments = assessment_repository;
        let mut suppression = suppression_repository;
        let mut qualification = qualification_repository;
        let mut scoring_agents = scoring_agent_repository;
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
        if target == LeadStatus::Rejected && rejection.is_none() {
            return Err(CoreError::Invalid {
                field: "reason".to_owned(),
                message: "is required to reject".to_owned(),
            });
        }
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
                &mut scoring_agents,
                &mut profiles,
                &mut assessments,
                &lead,
                Decided {
                    kinds: &[kind],
                    rejection: rejection.as_ref(),
                    actor,
                    at: updated.updated_at,
                },
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
