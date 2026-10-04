use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    NormalizedDomain,
    lead::{Assessment, ContactEmail, Lead, LeadId, LeadStatus, Score, Signal},
    profile::ProfileId,
};

#[cfg_attr(test, mockall::automock)]
pub trait LeadRepository: Send {
    fn record_seen(
        &mut self,
        profile_id: ProfileId,
        domain: &NormalizedDomain,
        seen_at: DateTime<Utc>,
    ) -> impl Future<Output = Result<Lead, CoreError>> + Send;

    fn find(&mut self, id: LeadId) -> impl Future<Output = Result<Option<Lead>, CoreError>> + Send;

    fn find_by_domain(
        &mut self,
        profile_id: ProfileId,
        domain: &NormalizedDomain,
    ) -> impl Future<Output = Result<Option<Lead>, CoreError>> + Send;

    fn update_status(
        &mut self,
        id: LeadId,
        from: LeadStatus,
        to: LeadStatus,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<Option<Lead>, CoreError>> + Send;

    fn update_score(
        &mut self,
        id: LeadId,
        score: Score,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<Option<Lead>, CoreError>> + Send;

    fn list_by_profile(
        &mut self,
        profile_id: ProfileId,
    ) -> impl Future<Output = Result<Vec<Lead>, CoreError>> + Send;
}

#[cfg_attr(test, mockall::automock)]
pub trait SignalRepository: Send {
    fn insert(&mut self, signal: &Signal) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn list_for_lead(
        &mut self,
        lead_id: LeadId,
    ) -> impl Future<Output = Result<Vec<Signal>, CoreError>> + Send;
}

#[cfg_attr(test, mockall::automock)]
pub trait ContactRepository: Send {
    fn insert(
        &mut self,
        contact: &ContactEmail,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn list_for_lead(
        &mut self,
        lead_id: LeadId,
    ) -> impl Future<Output = Result<Vec<ContactEmail>, CoreError>> + Send;
}

#[cfg_attr(test, mockall::automock)]
pub trait AssessmentRepository: Send {
    fn insert(
        &mut self,
        assessment: &Assessment,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn list_for_lead(
        &mut self,
        lead_id: LeadId,
    ) -> impl Future<Output = Result<Vec<Assessment>, CoreError>> + Send;
}
