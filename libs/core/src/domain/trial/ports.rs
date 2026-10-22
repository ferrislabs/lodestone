use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    profile::ProfileId,
    trial::{DecidedLead, ProfileTrial, TrialId, TrialLead, TrialStatus},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub status: TrialStatus,
    pub error: Option<String>,
    pub at: DateTime<Utc>,
}

#[cfg_attr(test, mockall::automock)]
pub trait ProfileTrialRepository: Send {
    fn create(
        &mut self,
        trial: &ProfileTrial,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn find(
        &mut self,
        id: TrialId,
    ) -> impl Future<Output = Result<Option<ProfileTrial>, CoreError>> + Send;

    fn list_for_profile(
        &mut self,
        profile_id: ProfileId,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<ProfileTrial>, CoreError>> + Send;

    fn transition(
        &mut self,
        id: TrialId,
        change: &Transition,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn record_spend(
        &mut self,
        id: TrialId,
        spent_cents: f64,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn record_lead(
        &mut self,
        trial_id: TrialId,
        lead: &TrialLead,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn list_leads(
        &mut self,
        trial_id: TrialId,
    ) -> impl Future<Output = Result<Vec<TrialLead>, CoreError>> + Send;

    fn decided_leads(
        &mut self,
        profile_id: ProfileId,
    ) -> impl Future<Output = Result<Vec<DecidedLead>, CoreError>> + Send;
}
