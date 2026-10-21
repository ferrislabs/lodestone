use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::transactional;
use tracing::info;

use crate::{
    application::{LodestoneUseCase, sources::source_job, timeline::agent_stopped},
    domain::{
        agent_work::{
            AgentWork, CancelReason, ProfileAgents, WorkTarget, kept_agents, only_removes_agents,
            ports::{AgentRegistry, AgentStop, AgentWorkRepository, KeptAgents},
        },
        job::ports::JobQueue,
        lead::LeadId,
        profile::{
            ActiveProfile, Profile, ProfileId, Revised, commands::ReviseProfileCommand,
            ports::ProfileRepository,
        },
        timeline::ports::TimelineRepository,
        values::SourceName,
    },
    infrastructure::job::postgres::PgJobQueue,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProfileSettlement {
    pub cancelled_jobs: u64,
    pub restored_jobs: u64,
    pub cleared_retries: u64,
    pub carried_leads: u64,
}

fn stop_reason_for(
    previous: &ActiveProfile,
    current: Option<&ActiveProfile>,
    source: SourceName,
) -> CancelReason {
    let Some(current) = current else {
        return CancelReason::ProfileDeleted;
    };
    let agents = ProfileAgents::of(current);
    previous
        .revision
        .definition
        .definition()
        .targets(source)
        .iter()
        .find_map(|target| agents.check(&WorkTarget::of_source(target)))
        .unwrap_or(CancelReason::AgentRemoved)
}

impl LodestoneUseCase {
    #[transactional(profile)]
    pub async fn list_current_profiles(&self) -> Result<Vec<ActiveProfile>, CoreError> {
        let mut repo = profile_repository;
        repo.list_current().await
    }

    #[transactional(profile)]
    async fn current_profile(&self, id: ProfileId) -> Result<Option<ActiveProfile>, CoreError> {
        let mut repo = profile_repository;
        let Some(profile) = repo.find(id).await? else {
            return Ok(None);
        };
        let revision = repo
            .list_revisions(id)
            .await?
            .into_iter()
            .max_by_key(|revision| revision.number);
        Ok(revision.map(|revision| ActiveProfile { profile, revision }))
    }

    #[transactional(profile)]
    async fn remove_profile(&self, id: ProfileId) -> Result<bool, CoreError> {
        let mut repo = profile_repository;
        repo.delete(id).await
    }

    pub async fn refreshed_active_profiles(&self) -> Result<Vec<ActiveProfile>, CoreError> {
        let current = self.list_current_profiles().await?;
        self.agents.refresh(&current);
        Ok(current
            .into_iter()
            .filter(|current| current.profile.active)
            .collect())
    }

    pub async fn has_source_targets(&self, source: SourceName) -> Result<bool, CoreError> {
        Ok(self
            .refreshed_active_profiles()
            .await?
            .iter()
            .any(|active| active.revision.definition.definition().has_targets(source)))
    }

    pub fn agent_work(&self, active: &ActiveProfile, target: WorkTarget) -> AgentWork {
        self.agents
            .token(active.profile.id, active.revision.number, target)
    }

    pub async fn update_profile(
        &self,
        command: ReviseProfileCommand,
    ) -> Result<Revised, CoreError> {
        let previous = self.current_profile(command.profile_id).await?;
        let revised = self.revise_profile(command).await?;
        if revised.is_created()
            && let Some(previous) = previous
        {
            let current = ActiveProfile {
                profile: previous.profile.clone(),
                revision: revised.revision().clone(),
            };
            self.settle_profile(&previous, Some(&current)).await?;
        }
        Ok(revised)
    }

    pub async fn change_profile_activation(
        &self,
        id: ProfileId,
        active: bool,
    ) -> Result<Profile, CoreError> {
        let previous = self.current_profile(id).await?;
        let profile = self.set_profile_active(id, active).await?;
        if let Some(previous) = previous {
            let current = ActiveProfile {
                profile: profile.clone(),
                revision: previous.revision.clone(),
            };
            self.settle_profile(&previous, Some(&current)).await?;
        }
        Ok(profile)
    }

    pub async fn delete_profile(&self, id: ProfileId) -> Result<(), CoreError> {
        let previous = self.current_profile(id).await?.ok_or(CoreError::NotFound)?;
        if !self.remove_profile(id).await? {
            return Err(CoreError::NotFound);
        }
        self.settle_profile(&previous, None).await?;
        Ok(())
    }

    pub async fn settle_profile(
        &self,
        previous: &ActiveProfile,
        current: Option<&ActiveProfile>,
    ) -> Result<ProfileSettlement, CoreError> {
        let id = previous.profile.id;
        match current {
            Some(current) => self.agents.publish(current),
            None => self.agents.delete(id),
        }
        let now = Utc::now();
        let active = self.list_active_profiles().await?;
        let mut settlement = ProfileSettlement::default();
        if previous.profile.active {
            for source in SourceName::ALL {
                let served_before = previous
                    .revision
                    .definition
                    .definition()
                    .has_targets(source);
                let served_now = active
                    .iter()
                    .any(|other| other.revision.definition.definition().has_targets(source));
                if served_before && !served_now {
                    settlement.cancelled_jobs += self
                        .cancel_source_jobs(source, stop_reason_for(previous, current, source), now)
                        .await?;
                }
            }
        }
        settlement.restored_jobs = self.restore_source_jobs(&active, now).await?;
        if let Some(current) = current {
            let kept = if current.profile.active {
                kept_agents(&ProfileAgents::of(current))
            } else {
                KeptAgents::default()
            };
            if previous.revision.number < current.revision.number
                && only_removes_agents(
                    previous.revision.definition.definition(),
                    current.revision.definition.definition(),
                )
            {
                settlement.carried_leads = self
                    .carry_forward(
                        id,
                        previous,
                        current,
                        &kept_agents(&ProfileAgents::of(current)),
                    )
                    .await?;
            }
            settlement.cleared_retries = self.clear_agent_retries(id, &kept).await?;
        }
        info!(
            profile = %id,
            cancelled_jobs = settlement.cancelled_jobs,
            restored_jobs = settlement.restored_jobs,
            cleared_retries = settlement.cleared_retries,
            carried_leads = settlement.carried_leads,
            "profile change settled for running agents"
        );
        Ok(settlement)
    }

    async fn restore_source_jobs(
        &self,
        active: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let queue = PgJobQueue::new(self.pool.clone());
        let mut restored = 0;
        for source in self.cancelled_source_jobs().await? {
            if active
                .iter()
                .any(|profile| profile.revision.definition.definition().has_targets(source))
                && queue
                    .enqueue(&source_job(source, now)?, now)
                    .await?
                    .is_some()
            {
                restored += 1;
            }
        }
        Ok(restored)
    }

    #[transactional(agent_work)]
    async fn cancel_source_jobs(
        &self,
        source: SourceName,
        reason: CancelReason,
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut repo = agent_work_repository;
        repo.cancel_source_jobs(source, reason, now).await
    }

    #[transactional(agent_work)]
    async fn cancelled_source_jobs(&self) -> Result<Vec<SourceName>, CoreError> {
        let mut repo = agent_work_repository;
        repo.cancelled_sources().await
    }

    #[transactional(agent_work)]
    async fn carry_forward(
        &self,
        id: ProfileId,
        previous: &ActiveProfile,
        current: &ActiveProfile,
        kept: &KeptAgents,
    ) -> Result<u64, CoreError> {
        let mut repo = agent_work_repository;
        repo.carry_forward(id, previous.revision.number, current.revision.number, kept)
            .await
    }

    #[transactional(agent_work)]
    async fn clear_agent_retries(
        &self,
        id: ProfileId,
        kept: &KeptAgents,
    ) -> Result<u64, CoreError> {
        let mut repo = agent_work_repository;
        repo.clear_retries(id, kept).await
    }

    #[transactional(agent_work, timeline)]
    pub async fn record_agent_stop(&self, stop: &AgentStop) -> Result<(), CoreError> {
        let mut repo = agent_work_repository;
        let mut timeline = timeline_repository;
        repo.record_stop(stop).await?;
        timeline
            .record(stop.lead_id, &agent_stopped(stop))
            .await
            .map(drop)
    }

    pub async fn note_stop(
        &self,
        lead: LeadId,
        work: &AgentWork,
        reason: CancelReason,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        info!(
            lead_id = %lead,
            kind = work.target().kind.as_str(),
            agent = work.target().agent.as_str(),
            reason = reason.as_str(),
            "agent work stopped"
        );
        self.record_agent_stop(&AgentStop {
            lead_id: lead,
            target: work.target().clone(),
            revision: work.revision(),
            reason,
            stopped_at: at,
        })
        .await
    }

    #[transactional(agent_work)]
    pub async fn lead_agent_stops(&self, lead: LeadId) -> Result<Vec<AgentStop>, CoreError> {
        let mut repo = agent_work_repository;
        repo.stops(lead).await
    }
}
