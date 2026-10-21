use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    NormalizedDomain,
    lead::LeadId,
    profile::ProfileId,
    suppression::{SuppressionEntry, SuppressionId},
    values::EmailAddress,
};

#[cfg_attr(test, mockall::automock)]
pub trait SuppressionRepository: Send {
    fn insert(
        &mut self,
        entry: &SuppressionEntry,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn remove(&mut self, id: SuppressionId)
    -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn list(&mut self) -> impl Future<Output = Result<Vec<SuppressionEntry>, CoreError>> + Send;

    fn is_domain_suppressed(
        &mut self,
        domain: &NormalizedDomain,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn is_email_suppressed(
        &mut self,
        email: &EmailAddress,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn lead_exclusion(
        &mut self,
        lead_id: LeadId,
    ) -> impl Future<Output = Result<Option<String>, CoreError>> + Send;

    fn exclude_leads(
        &mut self,
        lead_ids: &[LeadId],
        reason: &str,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn lift_exclusion(
        &mut self,
        lead_id: LeadId,
        reason: &str,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn exclude_leads_under(
        &mut self,
        profiles: &[ProfileId],
        suffixes: &[&'static str],
        reason: &str,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;
}
