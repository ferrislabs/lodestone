use std::collections::HashMap;

use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    lead::{Lead, LeadId},
    llm::provider::ProviderId,
    profile::{ProfileId, RevisionNumber},
    qualification::{
        AgentCallFailure, AgentFailure, Completion, DueGates, LeadScoring, QualificationResult,
        Rescoring, SummaryState,
    },
};

#[cfg_attr(test, mockall::automock)]
pub trait QualificationRepository: Send {
    fn due_leads(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
        limit: u32,
        gates: DueGates,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<Lead>, CoreError>> + Send;

    fn count_due(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
        gates: DueGates,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn due_summaries(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
        limit: u32,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<Lead>, CoreError>> + Send;

    fn count_due_summaries(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn mark_summarised(
        &mut self,
        lead_id: LeadId,
        qualified_at: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn record_summary_failure(
        &mut self,
        lead_id: LeadId,
        qualified_at: DateTime<Utc>,
        reason: &str,
        provider_id: Option<ProviderId>,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<Option<u32>, CoreError>> + Send;

    fn record_agent_failure(
        &mut self,
        failure: &AgentCallFailure,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<u32, CoreError>> + Send;

    fn agent_failures(
        &mut self,
        lead_id: LeadId,
    ) -> impl Future<Output = Result<Vec<AgentFailure>, CoreError>> + Send;

    fn retry_agent_failures(
        &mut self,
        lead_id: LeadId,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn reset_provider_backoff(
        &mut self,
        provider_id: ProviderId,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn reset_summary(
        &mut self,
        lead_id: LeadId,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn summary_state(
        &mut self,
        lead_id: LeadId,
    ) -> impl Future<Output = Result<Option<SummaryState>, CoreError>> + Send;

    fn store_result(
        &mut self,
        result: &QualificationResult,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn complete(
        &mut self,
        completion: &Completion,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn current_results(
        &mut self,
        lead_id: LeadId,
    ) -> impl Future<Output = Result<Vec<QualificationResult>, CoreError>> + Send;

    fn stale_scorings(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
        limit: u32,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<Lead>, CoreError>> + Send;

    fn count_stale_scorings(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn rescore(
        &mut self,
        rescoring: &Rescoring,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn scorings(
        &mut self,
        lead_ids: &[LeadId],
    ) -> impl Future<Output = Result<HashMap<LeadId, LeadScoring>, CoreError>> + Send;
}
