use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    lead::{Lead, LeadId},
    profile::{ProfileId, RevisionNumber},
    scoring_agent::{Decision, ScoringResult},
};

#[derive(Debug, Clone, PartialEq)]
pub struct ScoringCompletion {
    pub lead_id: LeadId,
    pub profile_revision: RevisionNumber,
    pub qualified_at: DateTime<Utc>,
    pub decision: Decision,
}

#[cfg_attr(test, mockall::automock)]
pub trait ScoringRepository: Send {
    fn due_leads(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<Lead>, CoreError>> + Send;

    fn count_due(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn store_result(
        &mut self,
        result: &ScoringResult,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn complete(
        &mut self,
        completion: &ScoringCompletion,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn current_results(
        &mut self,
        lead_id: LeadId,
    ) -> impl Future<Output = Result<Vec<ScoringResult>, CoreError>> + Send;

    fn apply_decision(
        &mut self,
        lead_id: LeadId,
        decision: &Decision,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;
}
