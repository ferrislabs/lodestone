use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    agent_work::{AgentWork, CancelReason, ProfileAgents, WorkKind, WorkTarget},
    lead::LeadId,
    profile::{ActiveProfile, ProfileId, RevisionNumber},
    values::SourceName,
};

pub trait AgentRegistry: Send + Sync {
    fn publish(&self, current: &ActiveProfile);

    fn delete(&self, profile: ProfileId);

    fn refresh(&self, current: &[ActiveProfile]);

    fn agents(&self, profile: ProfileId) -> Option<ProfileAgents>;

    fn token(&self, profile: ProfileId, revision: RevisionNumber, target: WorkTarget) -> AgentWork;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStop {
    pub lead_id: LeadId,
    pub target: WorkTarget,
    pub revision: RevisionNumber,
    pub reason: CancelReason,
    pub stopped_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeptAgents {
    pub qualification: Vec<String>,
    pub scoring: Vec<String>,
}

pub trait AgentWorkRepository: Send {
    fn clear_retries(
        &mut self,
        profile: ProfileId,
        kept: &KeptAgents,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn carry_forward(
        &mut self,
        profile: ProfileId,
        from: RevisionNumber,
        to: RevisionNumber,
        kept: &KeptAgents,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn record_stop(
        &mut self,
        stop: &AgentStop,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn clear_stops(
        &mut self,
        lead: LeadId,
        kinds: &[WorkKind],
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn stops(
        &mut self,
        lead: LeadId,
    ) -> impl Future<Output = Result<Vec<AgentStop>, CoreError>> + Send;

    fn cancel_source_jobs(
        &mut self,
        source: SourceName,
        reason: CancelReason,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn cancelled_sources(
        &mut self,
    ) -> impl Future<Output = Result<Vec<SourceName>, CoreError>> + Send;
}
