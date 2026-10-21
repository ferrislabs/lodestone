use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::transactional;
use tracing::info;

use crate::{
    application::LodestoneUseCase,
    domain::{
        lead::Lead,
        profile::ActiveProfile,
        profile::definition::ProfileDefinition,
        qualification::{
            AgentOpinion, EvidenceStrength, LeadScoring, QualificationResult, Rescoring, ResultId,
            ResultStatus, SCORING_VERSION, VerdictSource, agent_score, exclusions, judge,
            ports::QualificationRepository, stored_answers,
        },
        scoring_agent::{Decision, decide, ports::ScoringRepository},
    },
};

pub(crate) fn stored_opinions(
    definition: &ProfileDefinition,
    results: &[QualificationResult],
) -> (Vec<AgentOpinion>, Vec<(ResultId, f64)>) {
    let mut opinions = Vec::new();
    let mut agent_scores = Vec::new();
    for agent in definition
        .qualification_agents
        .iter()
        .filter(|agent| agent.enabled)
    {
        let Some(result) = results
            .iter()
            .find(|result| result.agent_id == agent.id && result.status == ResultStatus::Valid)
        else {
            continue;
        };
        let Some(answers) = stored_answers(agent, &result.answers) else {
            continue;
        };
        let score = agent_score(agent, &answers, definition.verdict.calibration_floor);
        opinions.push(AgentOpinion {
            weight: agent.weight,
            score,
            axes: answers.0.len(),
        });
        agent_scores.push((result.id, score));
    }
    (opinions, agent_scores)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RescoreReport {
    pub leads: u64,
    pub backlog: u64,
}

impl LodestoneUseCase {
    pub async fn rescore_stale_leads(
        &self,
        batch_size: u32,
        now: DateTime<Utc>,
    ) -> Result<RescoreReport, CoreError> {
        let profiles = self.list_active_profiles().await?;
        let mut report = RescoreReport::default();
        let mut remaining = batch_size;
        for active in &profiles {
            if remaining == 0 {
                break;
            }
            for lead in &self.stale_scorings(active, remaining, now).await? {
                self.rescore_lead(active, lead, now).await?;
                report.leads += 1;
                remaining = remaining.saturating_sub(1);
            }
        }
        for active in &profiles {
            report.backlog += self.count_stale_scorings(active, now).await?;
        }
        if report.leads > 0 {
            info!(
                monotonic_counter.qualification_rescored = report.leads,
                backlog = report.backlog,
                "stored leads rescored without calling a model"
            );
        }
        Ok(report)
    }

    async fn rescore_lead(
        &self,
        active: &ActiveProfile,
        lead: &Lead,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let definition = active.revision.definition.definition();
        let results = self.lead_qualification_results(lead.id).await?;
        let scoring_results = self.lead_scoring_results(lead.id).await?;
        let (_, facts, _) = self.signals_with_facts(lead).await?;
        let (opinions, agent_scores) = stored_opinions(definition, &results);
        let evidence =
            EvidenceStrength::compute(&facts, lead.score.get(), &definition.evidence, now);
        let found = exclusions(&results, &definition.qualification_agents);
        let rescoring = Rescoring {
            lead_id: lead.id,
            judgement: judge(
                &opinions,
                evidence,
                definition.verdict,
                &definition.evidence,
            ),
            exclusions: found.clone(),
            scoring_version: SCORING_VERSION,
            scored_at: now,
            agent_scores,
        };
        let scored = !scoring_results.is_empty() && self.has_scoring_verdict(lead).await?;
        let decision = (scored || !found.is_empty())
            .then(|| decide(&definition.scoring_agents, &scoring_results, found));
        self.store_rescoring(&rescoring, decision.as_ref()).await
    }

    async fn has_scoring_verdict(&self, lead: &Lead) -> Result<bool, CoreError> {
        Ok(self
            .scorings_of(lead)
            .await?
            .is_some_and(|scoring| scoring.verdict_source == VerdictSource::Scoring))
    }

    #[transactional(qualification)]
    async fn scorings_of(&self, lead: &Lead) -> Result<Option<LeadScoring>, CoreError> {
        let mut repo = qualification_repository;
        Ok(repo.scorings(&[lead.id]).await?.remove(&lead.id))
    }

    #[transactional(qualification)]
    async fn stale_scorings(
        &self,
        active: &ActiveProfile,
        limit: u32,
        now: DateTime<Utc>,
    ) -> Result<Vec<Lead>, CoreError> {
        let mut repo = qualification_repository;
        repo.stale_scorings(active.profile.id, active.revision.number, limit, now)
            .await
    }

    #[transactional(qualification)]
    async fn count_stale_scorings(
        &self,
        active: &ActiveProfile,
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut repo = qualification_repository;
        repo.count_stale_scorings(active.profile.id, active.revision.number, now)
            .await
    }

    #[transactional(qualification, scoring_agent)]
    async fn store_rescoring(
        &self,
        rescoring: &Rescoring,
        decision: Option<&Decision>,
    ) -> Result<(), CoreError> {
        let mut qualification = qualification_repository;
        let mut scoring = scoring_agent_repository;
        qualification.rescore(rescoring).await?;
        if let Some(decision) = decision {
            scoring.apply_decision(rescoring.lead_id, decision).await?;
        }
        Ok(())
    }
}
