use chrono::{DateTime, Utc};
use common::CoreError;

use crate::{
    UserId,
    domain::organization::{Organization, OrganizationId},
};

#[cfg_attr(test, mockall::automock)]
pub trait OrganizationRepository: Send {
    fn insert(
        &mut self,
        organization: &Organization,
    ) -> impl Future<Output = Result<Organization, CoreError>> + Send;

    /// Whether a live organization already holds this slug. Reading first is
    /// what lets a caller pick a free one: a failed insert aborts the enclosing
    /// transaction, so retrying inside it is not an option.
    fn slug_is_taken(&mut self, slug: &str)
    -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn find_by_id(
        &mut self,
        id: OrganizationId,
    ) -> impl Future<Output = Result<Option<Organization>, CoreError>> + Send;

    fn list_for_user(
        &mut self,
        user_id: UserId,
    ) -> impl Future<Output = Result<Vec<Organization>, CoreError>> + Send;

    fn update(
        &mut self,
        organization: &Organization,
    ) -> impl Future<Output = Result<Organization, CoreError>> + Send;

    fn soft_delete(
        &mut self,
        id: OrganizationId,
        deleted_at: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;
}
