use std::collections::HashMap;

use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    NormalizedDomain,
    lead::{
        Assessment, AssessmentId, ContactEmail, Lead, LeadId, LeadStatus, Score, Signal,
        draft::{DraftEdit, DraftPage, DraftRecord, DraftReview},
        query::{LeadPage, LeadQuery, ProfileSummary, VerdictFilter},
    },
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

    fn list_by_domain(
        &mut self,
        domain: &NormalizedDomain,
    ) -> impl Future<Output = Result<Vec<Lead>, CoreError>> + Send;

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

    fn list_page(
        &mut self,
        profile_id: ProfileId,
        query: &LeadQuery,
    ) -> impl Future<Output = Result<LeadPage, CoreError>> + Send;

    fn profile_summaries(
        &mut self,
    ) -> impl Future<Output = Result<Vec<ProfileSummary>, CoreError>> + Send;
}

#[cfg_attr(test, mockall::automock)]
pub trait SignalRepository: Send {
    fn record(&mut self, signal: &Signal) -> impl Future<Output = Result<bool, CoreError>> + Send;

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

    fn latest_summaries(
        &mut self,
        lead_ids: &[LeadId],
    ) -> impl Future<Output = Result<HashMap<LeadId, String>, CoreError>> + Send;

    fn find_draft(
        &mut self,
        id: AssessmentId,
    ) -> impl Future<Output = Result<Option<DraftRecord>, CoreError>> + Send;

    fn list_pending_drafts(
        &mut self,
        profile_id: Option<ProfileId>,
        verdicts: &[VerdictFilter],
        limit: u32,
        offset: u64,
    ) -> impl Future<Output = Result<DraftPage, CoreError>> + Send;

    fn edit_draft(
        &mut self,
        id: AssessmentId,
        edit: &DraftEdit,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn decide_draft(
        &mut self,
        id: AssessmentId,
        review: &DraftReview,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;
}
