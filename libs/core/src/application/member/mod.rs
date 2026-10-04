use authz::Subject;
use common::CoreError;
use oxid_macros::transactional;

use crate::{
    UserId,
    application::OxidUseCase,
    domain::{
        member::{
            Member, MemberId,
            commands::{AddMemberCommand, AssignRoleCommand},
            service::MemberService,
        },
        organization::OrganizationId,
        role::RoleId,
    },
};

impl OxidUseCase {
    #[transactional(member, role, authz)]
    pub async fn add_member(
        &self,
        actor: Subject,
        command: AddMemberCommand,
    ) -> Result<Member, CoreError> {
        let mut service = MemberService::new(member_repository, role_repository, authz);
        service.add_member(actor, command).await
    }

    #[transactional(member, role, authz)]
    pub async fn list_members(
        &self,
        actor: Subject,
        organization_id: OrganizationId,
    ) -> Result<Vec<Member>, CoreError> {
        let mut service = MemberService::new(member_repository, role_repository, authz);
        service.list_members(actor, organization_id).await
    }

    #[transactional(member, role, authz)]
    pub async fn find_membership(
        &self,
        actor: Subject,
        organization_id: OrganizationId,
        user_id: UserId,
    ) -> Result<Option<Member>, CoreError> {
        let mut service = MemberService::new(member_repository, role_repository, authz);
        service
            .find_membership(actor, organization_id, user_id)
            .await
    }

    #[transactional(member, role, authz)]
    pub async fn remove_member(
        &self,
        actor: Subject,
        organization_id: OrganizationId,
        member_id: MemberId,
    ) -> Result<(), CoreError> {
        let mut service = MemberService::new(member_repository, role_repository, authz);
        service
            .remove_member(actor, organization_id, member_id)
            .await
    }

    #[transactional(member, role, authz)]
    pub async fn assign_role(
        &self,
        actor: Subject,
        command: AssignRoleCommand,
    ) -> Result<(), CoreError> {
        let mut service = MemberService::new(member_repository, role_repository, authz);
        service.assign_role(actor, command).await
    }

    #[transactional(member, role, authz)]
    pub async fn list_role_ids(
        &self,
        actor: Subject,
        organization_id: OrganizationId,
        member_id: MemberId,
    ) -> Result<Vec<RoleId>, CoreError> {
        let mut service = MemberService::new(member_repository, role_repository, authz);
        service
            .list_role_ids(actor, organization_id, member_id)
            .await
    }
}
