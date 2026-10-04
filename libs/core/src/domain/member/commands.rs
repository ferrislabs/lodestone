use crate::{
    UserId,
    domain::{member::MemberId, organization::OrganizationId, role::RoleId},
};

#[derive(Debug, Clone)]
pub struct AddMemberCommand {
    pub organization_id: OrganizationId,
    pub user_id: UserId,
}

#[derive(Debug, Clone)]
pub struct AssignRoleCommand {
    /// The organization both the member and the role must belong to. Checked
    /// by the composite foreign keys, so a mismatched triple is rejected by
    /// the database rather than merely by the service.
    pub organization_id: OrganizationId,
    pub member_id: MemberId,
    pub role_id: RoleId,
}
