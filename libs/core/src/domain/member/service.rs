use authz::{Authorizer, Resource, Subject};
use chrono::Utc;
use common::{CoreError, generate_uuid_v7};

use crate::application::policy;
use crate::{
    UserId,
    domain::{
        member::{
            Member, MemberId,
            commands::{AddMemberCommand, AssignRoleCommand},
            ports::MemberRepository,
        },
        organization::OrganizationId,
        role::{RoleId, ports::RoleRepository},
    },
};

pub struct MemberService<M, R, A>
where
    M: MemberRepository,
    R: RoleRepository,
    A: Authorizer,
{
    repo: M,
    roles: R,
    authz: A,
}

impl<M, R, A> MemberService<M, R, A>
where
    M: MemberRepository,
    R: RoleRepository,
    A: Authorizer,
{
    pub fn new(repo: M, roles: R, authz: A) -> Self {
        Self { repo, roles, authz }
    }

    /// Loads the actor's standing in `organization_id`. Refuses a subject that
    /// is not a member, so every method below starts from a caller that has
    /// some claim on the organization.
    async fn actor_in(
        &mut self,
        actor: Subject,
        organization_id: OrganizationId,
    ) -> Result<Subject, CoreError> {
        policy::enrich_for_organization(actor, organization_id, &mut self.repo, &mut self.roles)
            .await
    }

    #[tracing::instrument(skip(self), fields(organization_id = %command.organization_id.0, user_id = %command.user_id.0), err)]
    pub async fn add_member(
        &mut self,
        actor: Subject,
        command: AddMemberCommand,
    ) -> Result<Member, CoreError> {
        let actor = self.actor_in(actor, command.organization_id).await?;
        policy::require(
            &self.authz,
            &actor,
            "member.invite",
            Resource::new("organization", command.organization_id.0.to_string()),
        )
        .await?;

        let member = Member {
            id: MemberId(generate_uuid_v7()),
            organization_id: command.organization_id,
            user_id: command.user_id,
            joined_at: Utc::now(),
        };

        self.repo.insert(&member).await
    }

    #[tracing::instrument(skip(self), fields(organization_id = %organization_id.0), err)]
    pub async fn list_members(
        &mut self,
        actor: Subject,
        organization_id: OrganizationId,
    ) -> Result<Vec<Member>, CoreError> {
        // Membership is the authorization: seeing who else belongs is part of
        // belonging, and enrichment already refuses an outsider.
        self.actor_in(actor, organization_id).await?;
        self.repo.list_by_organization(organization_id).await
    }

    #[tracing::instrument(skip(self), fields(organization_id = %organization_id.0, user_id = %user_id.0), err)]
    pub async fn find_membership(
        &mut self,
        actor: Subject,
        organization_id: OrganizationId,
        user_id: UserId,
    ) -> Result<Option<Member>, CoreError> {
        self.actor_in(actor, organization_id).await?;
        self.repo
            .find_by_org_and_user(organization_id, user_id)
            .await
    }

    #[tracing::instrument(skip(self, actor), fields(organization_id = %organization_id.0, member_id = %member_id.0), err)]
    pub async fn remove_member(
        &mut self,
        actor: Subject,
        organization_id: OrganizationId,
        member_id: MemberId,
    ) -> Result<(), CoreError> {
        let actor = self.actor_in(actor, organization_id).await?;
        policy::require(
            &self.authz,
            &actor,
            "member.remove",
            Resource::new("member", member_id.0.to_string()),
        )
        .await?;

        self.repo.remove(organization_id, member_id).await
    }

    #[tracing::instrument(skip(self), fields(member_id = %command.member_id.0, role_id = %command.role_id.0), err)]
    pub async fn assign_role(
        &mut self,
        actor: Subject,
        command: AssignRoleCommand,
    ) -> Result<(), CoreError> {
        let actor = self.actor_in(actor, command.organization_id).await?;
        // Without this, any member could grant themselves the owner role of
        // their own organization: scoping by organization stops the
        // cross-tenant grant, not the escalation inside one.
        policy::require(
            &self.authz,
            &actor,
            "role.assign",
            Resource::new("role", command.role_id.0.to_string()),
        )
        .await?;

        self.repo
            .assign_role(command.organization_id, command.member_id, command.role_id)
            .await
    }

    #[tracing::instrument(skip(self, actor), fields(organization_id = %organization_id.0, member_id = %member_id.0), err)]
    pub async fn list_role_ids(
        &mut self,
        actor: Subject,
        organization_id: OrganizationId,
        member_id: MemberId,
    ) -> Result<Vec<RoleId>, CoreError> {
        self.actor_in(actor, organization_id).await?;
        self.repo.list_role_ids(organization_id, member_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::policy,
        domain::{
            member::ports::MockMemberRepository,
            role::{Permissions, Role, ports::MockRoleRepository},
        },
    };
    use authz::{Decision, MockAuthorizer};
    use mockall::predicate::eq;
    use uuid::Uuid;

    fn org_id() -> OrganizationId {
        OrganizationId(Uuid::new_v4())
    }

    fn user_id() -> UserId {
        UserId(Uuid::new_v4())
    }

    fn actor_for(user_id: UserId) -> Subject {
        policy::user_subject(user_id, Vec::new())
    }

    /// Stages the reads `enrich_for_organization` performs, granting the actor
    /// `permissions` in `organization_id`.
    fn stage_membership(
        members: &mut MockMemberRepository,
        roles: &mut MockRoleRepository,
        organization_id: OrganizationId,
        user_id: UserId,
        member_id: MemberId,
        permissions: Permissions,
    ) {
        let role_id = RoleId(Uuid::new_v4());
        members
            .expect_find_by_org_and_user()
            .with(eq(organization_id), eq(user_id))
            .times(1)
            .returning(move |organization_id, user_id| {
                let member = Member {
                    id: member_id,
                    organization_id,
                    user_id,
                    joined_at: Utc::now(),
                };
                Box::pin(async move { Ok(Some(member)) })
            });
        members
            .expect_list_role_ids()
            .with(eq(organization_id), eq(member_id))
            .times(1)
            .returning(move |_, _| Box::pin(async move { Ok(vec![role_id]) }));
        roles
            .expect_list_by_organization()
            .with(eq(organization_id))
            .times(1)
            .returning(move |organization_id| {
                let now = Utc::now();
                let role = Role {
                    id: role_id,
                    organization_id,
                    name: "staged".into(),
                    permissions,
                    created_at: now,
                    updated_at: now,
                };
                Box::pin(async move { Ok(vec![role]) })
            });
    }

    fn authorizer(allow: bool) -> MockAuthorizer {
        let mut authz = MockAuthorizer::new();
        authz.expect_evaluate().returning(move |_| {
            let decision = if allow {
                Decision::allow()
            } else {
                Decision::deny()
            };
            Box::pin(async move { Ok(decision) })
        });
        authz
    }

    #[tokio::test]
    async fn assign_role_carries_the_organization_to_the_repository() {
        let (oid, uid, mid) = (org_id(), user_id(), MemberId(Uuid::new_v4()));
        let role_id = RoleId(Uuid::new_v4());

        let mut members = MockMemberRepository::new();
        let mut roles = MockRoleRepository::new();
        stage_membership(&mut members, &mut roles, oid, uid, mid, Permissions::ALL);

        members
            .expect_assign_role()
            .with(eq(oid), eq(mid), eq(role_id))
            .times(1)
            .returning(|_, _, _| Box::pin(async { Ok(()) }));

        let mut service = MemberService::new(members, roles, authorizer(true));
        service
            .assign_role(
                actor_for(uid),
                AssignRoleCommand {
                    organization_id: oid,
                    member_id: mid,
                    role_id,
                },
            )
            .await
            .unwrap();
    }

    /// The escalation the audit found: scoping by organization stops a
    /// cross-tenant grant, but not a member granting themselves the owner role
    /// of their own organization. Only the permission check does.
    #[tokio::test]
    async fn assign_role_refuses_a_member_without_the_role_bit() {
        let (oid, uid, mid) = (org_id(), user_id(), MemberId(Uuid::new_v4()));

        let mut members = MockMemberRepository::new();
        let mut roles = MockRoleRepository::new();
        stage_membership(&mut members, &mut roles, oid, uid, mid, Permissions::NONE);
        members.expect_assign_role().never();

        let mut service = MemberService::new(members, roles, authorizer(false));
        let err = service
            .assign_role(
                actor_for(uid),
                AssignRoleCommand {
                    organization_id: oid,
                    member_id: mid,
                    role_id: RoleId(Uuid::new_v4()),
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(err, CoreError::Forbidden { .. }));
    }

    #[tokio::test]
    async fn assign_role_refuses_a_non_member_outright() {
        let (oid, uid) = (org_id(), user_id());

        let mut members = MockMemberRepository::new();
        members
            .expect_find_by_org_and_user()
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(None) }));
        members.expect_assign_role().never();

        let mut service =
            MemberService::new(members, MockRoleRepository::new(), MockAuthorizer::new());
        let err = service
            .assign_role(
                actor_for(uid),
                AssignRoleCommand {
                    organization_id: oid,
                    member_id: MemberId(Uuid::new_v4()),
                    role_id: RoleId(Uuid::new_v4()),
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(err, CoreError::Forbidden { .. }));
    }

    #[tokio::test]
    async fn remove_member_carries_the_organization_and_requires_the_bit() {
        let (oid, uid, mid) = (org_id(), user_id(), MemberId(Uuid::new_v4()));
        let target = MemberId(Uuid::new_v4());

        let mut members = MockMemberRepository::new();
        let mut roles = MockRoleRepository::new();
        stage_membership(&mut members, &mut roles, oid, uid, mid, Permissions::ALL);

        members
            .expect_remove()
            .with(eq(oid), eq(target))
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(()) }));

        let mut service = MemberService::new(members, roles, authorizer(true));
        service
            .remove_member(actor_for(uid), oid, target)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn list_members_is_open_to_any_member() {
        let (oid, uid, mid) = (org_id(), user_id(), MemberId(Uuid::new_v4()));

        let mut members = MockMemberRepository::new();
        let mut roles = MockRoleRepository::new();
        stage_membership(&mut members, &mut roles, oid, uid, mid, Permissions::NONE);

        members
            .expect_list_by_organization()
            .with(eq(oid))
            .times(1)
            .returning(|_| Box::pin(async { Ok(Vec::new()) }));

        let mut service = MemberService::new(members, roles, MockAuthorizer::new());
        let listed = service.list_members(actor_for(uid), oid).await.unwrap();
        assert!(listed.is_empty());
    }

    #[tokio::test]
    async fn list_members_refuses_a_non_member() {
        let (oid, uid) = (org_id(), user_id());

        let mut members = MockMemberRepository::new();
        members
            .expect_find_by_org_and_user()
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(None) }));
        members.expect_list_by_organization().never();

        let mut service =
            MemberService::new(members, MockRoleRepository::new(), MockAuthorizer::new());
        let err = service.list_members(actor_for(uid), oid).await.unwrap_err();

        assert!(matches!(err, CoreError::Forbidden { .. }));
    }

    #[tokio::test]
    async fn add_member_requires_the_invite_bit() {
        let (oid, uid, mid) = (org_id(), user_id(), MemberId(Uuid::new_v4()));

        let mut members = MockMemberRepository::new();
        let mut roles = MockRoleRepository::new();
        stage_membership(&mut members, &mut roles, oid, uid, mid, Permissions::NONE);
        members.expect_insert().never();

        let mut service = MemberService::new(members, roles, authorizer(false));
        let err = service
            .add_member(
                actor_for(uid),
                AddMemberCommand {
                    organization_id: oid,
                    user_id: user_id(),
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(err, CoreError::Forbidden { .. }));
    }
}
