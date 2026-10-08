use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::transactional;
use tracing::info;

use crate::{
    UserId,
    application::{
        LodestoneUseCase,
        output::schedule_webhook,
        quality::{Decided, record_decisions},
    },
    domain::{
        agent_work::ports::AgentWorkRepository,
        calibration::{Calibrations, ports::CalibrationRepository},
        company::{
            CompanyDossier, CompanyEntity, DossierDocument, PAGE_PROMPT_CHARS, SiblingLead,
            ports::CompanyRepository,
        },
        ecosystem::{
            EcosystemReadiness, ecosystem_documents, hosting_platform, ports::EcosystemRepository,
        },
        enrichment::{
            InspectionRecord, ListedEnrichment, enrichment_state, evaluation_status,
            only_certificates,
        },
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
    #[transactional(lead, profile, assessment, qualification, ecosystem)]
    pub async fn list_leads(
        &self,
        profile_id: ProfileId,
        query: LeadQuery,
    ) -> Result<LeadListing, CoreError> {
        let mut leads = lead_repository;
        let mut profiles = profile_repository;
        let mut assessments = assessment_repository;
        let mut qualification = qualification_repository;
        let mut ecosystem = ecosystem_repository;
        let profile = profiles
            .find(profile_id)
            .await?
            .ok_or(CoreError::NotFound)?;
        let page = leads.list_page(profile_id, &query).await?;
        let ids: Vec<LeadId> = page.items.iter().map(|lead| lead.id).collect();
        let summaries = assessments.latest_summaries(&ids).await?;
        let scorings = qualification.scorings(&ids).await?;
        let max_per_day = self.requalify.max_per_day;
        let evaluations = ecosystem
            .listed(&ids)
            .await?
            .into_iter()
            .map(|(id, listed)| (id, evaluation_status(&listed, max_per_day)))
            .collect();
        Ok(LeadListing {
            profile,
            page,
            summaries,
            scorings,
            evaluations,
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
        company,
        calibration,
        ecosystem,
        agent_work
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
        let mut calibrations = calibration_repository;
        let mut ecosystems = ecosystem_repository;
        let mut stops = agent_work_repository;

        let lead = leads.find(lead_id).await?.ok_or(CoreError::NotFound)?;
        if suppression.is_domain_suppressed(&lead.domain).await?
            || suppression.lead_exclusion(lead_id).await?.is_some()
        {
            return Err(CoreError::NotFound);
        }
        let profile = profiles
            .find(lead.profile_id)
            .await?
            .ok_or(CoreError::NotFound)?;
        let qualification_results = qualification.current_results(lead_id).await?;
        let scoring_results = scoring_agents.current_results(lead_id).await?;
        let agent_failures = qualification.agent_failures(lead_id).await?;
        let revision_number = qualification_results
            .first()
            .map(|result| result.profile_revision)
            .or_else(|| {
                scoring_results
                    .first()
                    .map(|result| result.profile_revision)
            })
            .or_else(|| {
                agent_failures
                    .first()
                    .map(|failure| failure.profile_revision)
            });
        let stored = calibrations.list(lead.profile_id).await?;
        let (qualification_agents, scoring_agents, calibrations) = match revision_number {
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
                        Calibrations::for_definition(stored, definition),
                    )
                })
                .unwrap_or_default(),
            None => (Vec::new(), Vec::new(), Calibrations::none()),
        };
        let summary_state = qualification.summary_state(lead_id).await?;
        let scoring = qualification.scorings(&[lead_id]).await?.remove(&lead_id);
        let lead_signals = signals.list_for_lead(lead_id).await?;
        let ecosystem = ecosystems.of_domain(&lead.domain).await?;
        let listed = ecosystems
            .listed(&[lead_id])
            .await?
            .remove(&lead_id)
            .unwrap_or_else(|| ListedEnrichment {
                record: InspectionRecord {
                    enabled: true,
                    hosting_platform: hosting_platform(&lead.domain),
                    modules: ecosystem.modules.clone(),
                    readiness: EcosystemReadiness::default(),
                },
                only_certificates: only_certificates(&lead_signals),
                evaluated_at: lead.qualified_at,
                requalification: None,
            });
        let enrichment = enrichment_state(&listed.record);
        let evaluation = evaluation_status(&listed, self.requalify.max_per_day);
        let context = company_context(
            &mut signals,
            &mut known_hosts,
            &mut company,
            &lead,
            &lead_signals,
        )
        .await?;
        Ok(LeadDetail {
            enrichment,
            evaluation,
            ecosystem,
            summary_state,
            agent_failures,
            agent_stops: stops.stops(lead_id).await?,
            scoring,
            qualification_results,
            qualification_agents,
            scoring_results,
            scoring_agents,
            calibrations,
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

    pub async fn request_lead_reevaluation(&self, lead_id: LeadId) -> Result<Lead, CoreError> {
        self.request_lead_reevaluation_at(lead_id, Utc::now()).await
    }

    #[transactional(lead, suppression, qualification)]
    pub async fn request_lead_reevaluation_at(
        &self,
        lead_id: LeadId,
        now: DateTime<Utc>,
    ) -> Result<Lead, CoreError> {
        let mut leads = lead_repository;
        let mut suppression = suppression_repository;
        let mut qualification = qualification_repository;
        let lead = leads.find(lead_id).await?.ok_or(CoreError::NotFound)?;
        if suppression.is_domain_suppressed(&lead.domain).await?
            || suppression.lead_exclusion(lead_id).await?.is_some()
        {
            return Err(CoreError::NotFound);
        }
        qualification.request_reevaluation(lead_id, now).await?;
        info!(lead.id = %lead_id, "lead reevaluation requested");
        Ok(lead)
    }

    #[transactional(ecosystem)]
    pub(crate) async fn grounding_documents(
        &self,
        lead: &Lead,
        dossier: Option<&CompanyDossier>,
    ) -> Result<Vec<DossierDocument>, CoreError> {
        let mut ecosystem = ecosystem_repository;
        let mut documents = dossier
            .map(|dossier| dossier.documents(PAGE_PROMPT_CHARS))
            .unwrap_or_default();
        documents.extend(ecosystem_documents(
            &ecosystem.of_domain(&lead.domain).await?.findings,
        ));
        Ok(documents)
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
