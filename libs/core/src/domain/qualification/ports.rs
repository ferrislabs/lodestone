use common::CoreError;

use crate::domain::{
    lead::{Lead, LeadId},
    profile::{ProfileId, RevisionNumber},
    qualification::{Completion, QualificationResult},
};

#[cfg_attr(test, mockall::automock)]
pub trait QualificationRepository: Send {
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
}
