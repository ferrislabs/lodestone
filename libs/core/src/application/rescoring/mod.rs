use common::CoreError;
use lodestone_macros::transactional;
use tracing::info;

use crate::{
    application::LodestoneUseCase,
    domain::{
        lead::Lead,
        profile::ActiveProfile,
        qualification::{
            AgentOpinion, EvidenceStrength, Rescoring, ResultStatus, SCORING_VERSION, agent_score,
            exclusions, judge, ports::QualificationRepository, stored_answers,
        },
    },
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RescoreReport {
    pub leads: u64,
    pub backlog: u64,
}

impl LodestoneUseCase {
    pub async fn rescore_stale_leads(&self, batch_size: u32) -> Result<RescoreReport, CoreError> {
        let profiles = self.list_active_profiles().await?;
        let mut report = RescoreReport::default();
        let mut remaining = batch_size;
        for active in &profiles {
            if remaining == 0 {
                break;
            }
            for lead in &self.stale_scorings(active, remaining).await? {
                self.rescore_lead(active, lead).await?;
                report.leads += 1;
                remaining = remaining.saturating_sub(1);
            }
        }
        for active in &profiles {
            report.backlog += self.count_stale_scorings(active).await?;
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

    async fn rescore_lead(&self, active: &ActiveProfile, lead: &Lead) -> Result<(), CoreError> {
        let definition = active.revision.definition.definition();
        let results = self.lead_qualification_results(lead.id).await?;
        let (_, facts, _) = self.signals_with_facts(lead).await?;
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
        let evidence = EvidenceStrength::compute(&facts, lead.score.get(), &definition.evidence);
        let rescoring = Rescoring {
            lead_id: lead.id,
            judgement: judge(
                &opinions,
                evidence,
                definition.verdict,
                &definition.evidence,
            )
            .excluding(exclusions(&results, &definition.qualification_agents)),
            scoring_version: SCORING_VERSION,
            agent_scores,
        };
        self.store_rescoring(&rescoring).await
    }

    #[transactional(qualification)]
    async fn stale_scorings(
        &self,
        active: &ActiveProfile,
        limit: u32,
    ) -> Result<Vec<Lead>, CoreError> {
        let mut repo = qualification_repository;
        repo.stale_scorings(active.profile.id, active.revision.number, limit)
            .await
    }

    #[transactional(qualification)]
    async fn count_stale_scorings(&self, active: &ActiveProfile) -> Result<u64, CoreError> {
        let mut repo = qualification_repository;
        repo.count_stale_scorings(active.profile.id, active.revision.number)
            .await
    }

    #[transactional(qualification)]
    async fn store_rescoring(&self, rescoring: &Rescoring) -> Result<(), CoreError> {
        let mut repo = qualification_repository;
        repo.rescore(rescoring).await
    }
}
