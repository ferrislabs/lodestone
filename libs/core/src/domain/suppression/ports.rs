use common::CoreError;

use crate::domain::{
    NormalizedDomain,
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
}
