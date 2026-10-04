use common::CoreError;

use crate::{
    UserId,
    domain::{
        member::{Member, MemberId},
        organization::OrganizationId,
        role::RoleId,
    },
};

#[cfg_attr(test, mockall::automock)]
pub trait MemberRepository: Send {
    fn insert(&mut self, member: &Member)
    -> impl Future<Output = Result<Member, CoreError>> + Send;

    fn list_by_organization(
        &mut self,
        organization_id: OrganizationId,
    ) -> impl Future<Output = Result<Vec<Member>, CoreError>> + Send;

    fn find_by_org_and_user(
        &mut self,
        organization_id: OrganizationId,
        user_id: UserId,
    ) -> impl Future<Output = Result<Option<Member>, CoreError>> + Send;

    fn remove(
        &mut self,
        organization_id: OrganizationId,
        member_id: MemberId,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    /// Carrying the organization is not redundant with `member_id`: it is what
    /// the composite foreign keys check, so a member and a role from different
    /// organizations can never be linked.
    fn assign_role(
        &mut self,
        organization_id: OrganizationId,
        member_id: MemberId,
        role_id: RoleId,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn list_role_ids(
        &mut self,
        organization_id: OrganizationId,
        member_id: MemberId,
    ) -> impl Future<Output = Result<Vec<RoleId>, CoreError>> + Send;
}
