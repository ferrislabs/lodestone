use chrono::Utc;
use common::{CoreError, generate_uuid_v7};

/// Translates a Postgres unique-violation into a business-friendly message.
/// The infra layer surfaces `CoreError::Conflict(constraint_name)`; we map the
/// stable constraint identifier to a message safe to expose at the API.
fn map_organization_conflict(err: CoreError) -> CoreError {
    match err {
        CoreError::Conflict(constraint) => {
            let message = match constraint.as_str() {
                "organizations_slug_key" => "slug already taken",
                _ => "organization conflict",
            };
            CoreError::Conflict(message.to_owned())
        }
        other => other,
    }
}

use authz::{Authorizer, Resource, Subject};

/// How many slug variants to try before giving up. Beyond this the seed is
/// hopeless and a caller should be told, not looped on.
const MAX_SLUG_ATTEMPTS: u32 = 20;

use crate::{
    UserId,
    application::policy,
    domain::{
        member::{Member, MemberId, ports::MemberRepository},
        organization::{
            Organization, OrganizationId,
            commands::{CreateOrganizationCommand, UpdateOrganizationCommand},
            naming::{OrganizationName, Slug},
            ports::OrganizationRepository,
        },
        role::{
            ADMIN_ROLE_NAME, MEMBER_ROLE_NAME, OWNER_ROLE_NAME, Permissions, Role, RoleId,
            ports::RoleRepository,
        },
    },
};

pub struct OrganizationService<O, R, M, A>
where
    O: OrganizationRepository,
    R: RoleRepository,
    M: MemberRepository,
    A: Authorizer,
{
    organization_repository: O,
    role_repository: R,
    member_repository: M,
    authz: A,
}

impl<O, R, M, A> OrganizationService<O, R, M, A>
where
    O: OrganizationRepository,
    R: RoleRepository,
    M: MemberRepository,
    A: Authorizer,
{
    pub fn new(
        organization_repository: O,
        role_repository: R,
        member_repository: M,
        authz: A,
    ) -> Self {
        Self {
            organization_repository,
            role_repository,
            member_repository,
            authz,
        }
    }

    #[tracing::instrument(skip(self, actor), fields(organization_id = %id.0), err)]
    pub async fn get_organization(
        &mut self,
        actor: Subject,
        id: OrganizationId,
    ) -> Result<Organization, CoreError> {
        let organization = self
            .organization_repository
            .find_by_id(id)
            .await?
            .ok_or(CoreError::NotFound)?;

        // Membership *is* the read authorization: enrichment refuses a subject
        // with no standing in this organization, so no action bits are needed.
        policy::enrich_for_organization(
            actor,
            organization.id,
            &mut self.member_repository,
            &mut self.role_repository,
        )
        .await?;

        Ok(organization)
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id.0), err)]
    pub async fn list_organizations_for_user(
        &mut self,
        user_id: UserId,
    ) -> Result<Vec<Organization>, CoreError> {
        self.organization_repository.list_for_user(user_id).await
    }

    #[tracing::instrument(skip(self), fields(organization_id = %command.id.0, organization.slug = %command.slug), err)]
    pub async fn update_organization(
        &mut self,
        command: UpdateOrganizationCommand,
    ) -> Result<Organization, CoreError> {
        // 1. Load — required to authorize against actual org context.
        let mut organization = self
            .organization_repository
            .find_by_id(command.id)
            .await?
            .ok_or(CoreError::NotFound)?;

        // 2. Authorize — enrich the actor with the org membership /
        //    aggregated permission bitfield, then ask the policy engine.
        let actor = policy::enrich_for_organization(
            command.actor,
            organization.id,
            &mut self.member_repository,
            &mut self.role_repository,
        )
        .await?;
        policy::require(
            &self.authz,
            &actor,
            "organization.update",
            Resource::new("organization", organization.id.0.to_string())
                .with_property("oxid.slug", organization.slug.clone()),
        )
        .await?;

        // 3. Mutate.
        organization.name = command.name.into_inner();
        organization.slug = command.slug.into_inner();
        organization.updated_at = Utc::now();

        self.organization_repository
            .update(&organization)
            .await
            .map_err(map_organization_conflict)
    }

    #[tracing::instrument(skip(self, actor), fields(organization_id = %id.0), err)]
    pub async fn soft_delete_organization(
        &mut self,
        actor: Subject,
        id: OrganizationId,
    ) -> Result<(), CoreError> {
        let organization = self
            .organization_repository
            .find_by_id(id)
            .await?
            .ok_or(CoreError::NotFound)?;

        let actor = policy::enrich_for_organization(
            actor,
            organization.id,
            &mut self.member_repository,
            &mut self.role_repository,
        )
        .await?;
        policy::require(
            &self.authz,
            &actor,
            "organization.delete",
            Resource::new("organization", organization.id.0.to_string()),
        )
        .await?;

        self.organization_repository
            .soft_delete(id, Utc::now())
            .await
    }

    #[tracing::instrument(skip(self), fields(organization.slug = %command.slug, owner_id = %command.owner_id.0), err)]
    pub async fn create_organization(
        &mut self,
        command: CreateOrganizationCommand,
    ) -> Result<Organization, CoreError> {
        let now = Utc::now();
        // `owner_id` is Oxid's own user id, resolved from the OIDC subject by
        // the authentication middleware. No lookup is needed here: the caller
        // could not have reached this point without a `users` row.
        let owner_id = command.owner_id;

        let organization = self
            .organization_repository
            .insert(&Organization {
                id: OrganizationId(generate_uuid_v7()),
                name: command.name.into_inner(),
                slug: command.slug.into_inner(),
                owner_id,
                deleted_at: None,
                created_at: now,
                updated_at: now,
            })
            .await
            .map_err(map_organization_conflict)?;

        let owner_role = self
            .role_repository
            .insert(&Role {
                id: RoleId(generate_uuid_v7()),
                organization_id: organization.id,
                name: OWNER_ROLE_NAME.into(),
                permissions: Permissions::ALL,
                created_at: now,
                updated_at: now,
            })
            .await?;

        self.role_repository
            .insert(&Role {
                id: RoleId(generate_uuid_v7()),
                organization_id: organization.id,
                name: ADMIN_ROLE_NAME.into(),
                permissions: Permissions::MANAGE_MEMBERS,
                created_at: now,
                updated_at: now,
            })
            .await?;

        self.role_repository
            .insert(&Role {
                id: RoleId(generate_uuid_v7()),
                organization_id: organization.id,
                name: MEMBER_ROLE_NAME.into(),
                permissions: Permissions::NONE,
                created_at: now,
                updated_at: now,
            })
            .await?;

        let member = self
            .member_repository
            .insert(&Member {
                id: MemberId(generate_uuid_v7()),
                organization_id: organization.id,
                user_id: owner_id,
                joined_at: now,
            })
            .await?;

        self.member_repository
            .assign_role(organization.id, member.id, owner_role.id)
            .await?;

        Ok(organization)
    }

    /// Gives a user their first organization, once.
    ///
    /// Deliberately not part of authentication: user provisioning already runs
    /// on every authenticated request, and adding organization creation there
    /// would put a write on the hot path of every call. This is a first-login
    /// action, invoked once, and it is idempotent - a replay, a refreshed page
    /// or a concurrent second call finds the existing organization and returns
    /// it rather than creating another.
    #[tracing::instrument(skip(self), fields(user_id = %owner_id.0), err)]
    pub async fn ensure_default_organization(
        &mut self,
        owner_id: UserId,
        seed: &str,
    ) -> Result<Organization, CoreError> {
        if let Some(existing) = self
            .organization_repository
            .list_for_user(owner_id)
            .await?
            .into_iter()
            .next()
        {
            return Ok(existing);
        }

        let base = Slug::from_seed(seed).ok_or_else(|| {
            CoreError::Conflict("cannot derive an organization name from this account".to_owned())
        })?;
        let name = OrganizationName::try_from(seed.trim().to_owned()).unwrap_or_else(|_| {
            OrganizationName::try_from(base.as_str().to_owned())
                .expect("a valid slug is a valid name")
        });

        // Slugs are unique among live organizations and two accounts with the
        // same display name are ordinary, so the derived slug has to be checked
        // before it is used. Retrying a failed insert is not an option: a unique
        // violation aborts the enclosing transaction, and everything after it
        // would fail too.
        //
        // Reading first leaves a race with a concurrent bootstrap. The unique
        // index still refuses the duplicate, so the loser gets a conflict and a
        // retry finds the slug taken - correctness rests on the constraint, not
        // on this check.
        let slug = self.first_available_slug(&base).await?;

        self.create_organization(CreateOrganizationCommand {
            name,
            slug,
            owner_id,
        })
        .await
    }

    async fn first_available_slug(&mut self, base: &Slug) -> Result<Slug, CoreError> {
        for attempt in 0..MAX_SLUG_ATTEMPTS {
            let candidate = if attempt == 0 {
                base.clone()
            } else {
                match base.with_suffix(attempt + 1) {
                    Some(slug) => slug,
                    None => break,
                }
            };

            if !self
                .organization_repository
                .slug_is_taken(candidate.as_str())
                .await?
            {
                return Ok(candidate);
            }
        }

        Err(CoreError::Conflict(
            "could not find an available organization slug".to_owned(),
        ))
    }

    #[tracing::instrument(skip(self), fields(organization_id = %organization_id.0, user_id = %user_id.0), err)]
    pub async fn leave_organization(
        &mut self,
        actor: Subject,
        organization_id: OrganizationId,
        user_id: UserId,
    ) -> Result<(), CoreError> {
        let organization = self
            .get_organization(actor.clone(), organization_id)
            .await?;

        if organization.owner_id == user_id {
            return self.soft_delete_organization(actor, organization_id).await;
        }

        let member = self
            .member_repository
            .find_by_org_and_user(organization_id, user_id)
            .await?
            .ok_or(CoreError::NotFound)?;

        self.member_repository
            .remove(organization_id, member.id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        UserId,
        application::policy,
        domain::organization::naming::{OrganizationName, Slug},
        domain::{
            member::ports::MockMemberRepository, organization::ports::MockOrganizationRepository,
            role::ports::MockRoleRepository,
        },
    };
    use authz::{Decision, MockAuthorizer};
    use mockall::predicate::eq;
    use uuid::Uuid;

    fn fixture(id: OrganizationId) -> Organization {
        let now = Utc::now();
        Organization {
            id,
            name: "Acme".into(),
            slug: "acme".into(),
            owner_id: UserId(Uuid::new_v4()),
            deleted_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// A PDP that allows everything. For tests whose subject is a system
    /// subject: enrichment short-circuits for them, but the action is still
    /// evaluated, so the authorizer must answer.
    fn allowing_authorizer() -> MockAuthorizer {
        let mut authz = MockAuthorizer::new();
        authz
            .expect_evaluate()
            .returning(|_| Box::pin(async { Ok(Decision::allow()) }));
        authz
    }

    /// Subject built the way the API handler will build it: `oxid.user_id`
    /// is set, no IAM roles, no org context yet (the service enriches it).
    fn actor_for(user_id: UserId) -> authz::Subject {
        policy::user_subject(user_id, Vec::new())
    }

    /// Stages the calls `policy::enrich_for_organization` makes: lookup
    /// the member, list its role ids, list the org's roles. The roles
    /// list is left empty so aggregated permissions = 0; tests that rely
    /// on the engine answering `allow`/`deny` go through MockAuthorizer.
    fn stage_org_membership(
        members: &mut MockMemberRepository,
        roles: &mut MockRoleRepository,
        org_id: OrganizationId,
        user_id: UserId,
        member_id: MemberId,
        enrichments: usize,
    ) {
        members
            .expect_find_by_org_and_user()
            .with(eq(org_id), eq(user_id))
            .times(enrichments)
            .returning(move |organization_id, user_id| {
                let m = Member {
                    id: member_id,
                    organization_id,
                    user_id,
                    joined_at: Utc::now(),
                };
                Box::pin(async move { Ok(Some(m)) })
            });
        members
            .expect_list_role_ids()
            .with(eq(org_id), eq(member_id))
            .times(enrichments)
            .returning(|_, _| Box::pin(async { Ok(Vec::new()) }));
        roles
            .expect_list_by_organization()
            .with(eq(org_id))
            .times(enrichments)
            .returning(|_| Box::pin(async { Ok(Vec::new()) }));
    }

    #[tokio::test]
    async fn get_organization_returns_not_found_when_missing() {
        let id = OrganizationId(Uuid::new_v4());

        let mut organization_repository = MockOrganizationRepository::new();
        let role_repository = MockRoleRepository::new();
        let member_repository = MockMemberRepository::new();

        organization_repository
            .expect_find_by_id()
            .with(eq(id))
            .times(1)
            .returning(|_| Box::pin(async { Ok(None) }));

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            MockAuthorizer::new(),
        );
        let err = service
            .get_organization(actor_for(UserId(Uuid::new_v4())), id)
            .await
            .unwrap_err();

        assert!(matches!(err, CoreError::NotFound));
    }

    #[tokio::test]
    async fn get_organization_returns_entity_when_found() {
        let id = OrganizationId(Uuid::new_v4());
        let user_id = UserId(Uuid::new_v4());

        let mut organization_repository = MockOrganizationRepository::new();
        let mut role_repository = MockRoleRepository::new();
        let mut member_repository = MockMemberRepository::new();

        // Reading is guarded by membership, so the caller has to be one.
        stage_org_membership(
            &mut member_repository,
            &mut role_repository,
            id,
            user_id,
            MemberId(Uuid::new_v4()),
            1,
        );

        organization_repository
            .expect_find_by_id()
            .with(eq(id))
            .times(1)
            .returning(move |id| {
                let org = fixture(id);
                Box::pin(async move { Ok(Some(org)) })
            });

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            MockAuthorizer::new(),
        );

        let org = service
            .get_organization(actor_for(user_id), id)
            .await
            .unwrap();

        assert_eq!(org.id, id);
    }

    #[tokio::test]
    async fn update_organization_mutates_and_saves() {
        let id = OrganizationId(Uuid::new_v4());
        let user_id = UserId(Uuid::new_v4());
        let member_id = MemberId(Uuid::new_v4());

        let mut organization_repository = MockOrganizationRepository::new();
        let mut role_repository = MockRoleRepository::new();
        let mut member_repository = MockMemberRepository::new();

        organization_repository
            .expect_find_by_id()
            .with(eq(id))
            .times(1)
            .returning(move |id| {
                let org = fixture(id);
                Box::pin(async move { Ok(Some(org)) })
            });
        stage_org_membership(
            &mut member_repository,
            &mut role_repository,
            id,
            user_id,
            member_id,
            1,
        );
        organization_repository
            .expect_update()
            .times(1)
            .returning(|o| {
                let cloned = Organization {
                    id: o.id,
                    name: o.name.clone(),
                    slug: o.slug.clone(),
                    owner_id: o.owner_id,
                    deleted_at: o.deleted_at,
                    created_at: o.created_at,
                    updated_at: o.updated_at,
                };
                Box::pin(async move { Ok(cloned) })
            });

        let mut authz = MockAuthorizer::new();
        authz
            .expect_evaluate()
            .times(1)
            .returning(|_| Box::pin(async { Ok(Decision::allow()) }));

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            authz,
        );

        let updated = service
            .update_organization(UpdateOrganizationCommand {
                actor: actor_for(user_id),
                id,
                name: OrganizationName::try_from("Acme Inc.".to_owned()).unwrap(),
                slug: Slug::try_from("acme-inc".to_owned()).unwrap(),
            })
            .await
            .unwrap();

        assert_eq!(updated.name, "Acme Inc.");
        assert_eq!(updated.slug, "acme-inc");
    }

    #[tokio::test]
    async fn update_organization_returns_not_found_when_missing() {
        let id = OrganizationId(Uuid::new_v4());

        let mut organization_repository = MockOrganizationRepository::new();
        let role_repository = MockRoleRepository::new();
        let member_repository = MockMemberRepository::new();

        organization_repository
            .expect_find_by_id()
            .with(eq(id))
            .times(1)
            .returning(|_| Box::pin(async { Ok(None) }));

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            MockAuthorizer::new(),
        );

        let err = service
            .update_organization(UpdateOrganizationCommand {
                actor: actor_for(UserId(Uuid::new_v4())),
                id,
                name: OrganizationName::try_from("Whatever".to_owned()).unwrap(),
                slug: Slug::try_from("whatever".to_owned()).unwrap(),
            })
            .await
            .unwrap_err();

        assert!(matches!(err, CoreError::NotFound));
    }

    #[tokio::test]
    async fn update_organization_returns_forbidden_when_not_a_member() {
        let id = OrganizationId(Uuid::new_v4());
        let user_id = UserId(Uuid::new_v4());

        let mut organization_repository = MockOrganizationRepository::new();
        let role_repository = MockRoleRepository::new();
        let mut member_repository = MockMemberRepository::new();

        organization_repository
            .expect_find_by_id()
            .with(eq(id))
            .times(1)
            .returning(move |id| {
                let org = fixture(id);
                Box::pin(async move { Ok(Some(org)) })
            });
        member_repository
            .expect_find_by_org_and_user()
            .with(eq(id), eq(user_id))
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(None) }));

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            MockAuthorizer::new(),
        );

        let err = service
            .update_organization(UpdateOrganizationCommand {
                actor: actor_for(user_id),
                id,
                name: OrganizationName::try_from("Acme Inc.".to_owned()).unwrap(),
                slug: Slug::try_from("acme-inc".to_owned()).unwrap(),
            })
            .await
            .unwrap_err();

        assert!(matches!(err, CoreError::Forbidden { .. }));
    }

    #[tokio::test]
    async fn update_organization_returns_forbidden_when_authz_denies() {
        let id = OrganizationId(Uuid::new_v4());
        let user_id = UserId(Uuid::new_v4());
        let member_id = MemberId(Uuid::new_v4());

        let mut organization_repository = MockOrganizationRepository::new();
        let mut role_repository = MockRoleRepository::new();
        let mut member_repository = MockMemberRepository::new();

        organization_repository
            .expect_find_by_id()
            .with(eq(id))
            .times(1)
            .returning(move |id| {
                let org = fixture(id);
                Box::pin(async move { Ok(Some(org)) })
            });
        stage_org_membership(
            &mut member_repository,
            &mut role_repository,
            id,
            user_id,
            member_id,
            1,
        );
        // No `expect_update` — the call must short-circuit before mutation.

        let mut authz = MockAuthorizer::new();
        authz
            .expect_evaluate()
            .withf(move |req| {
                req.action.name == "organization.update"
                    && req.resource.r#type == "organization"
                    && req.resource.id == id.0.to_string()
            })
            .times(1)
            .returning(|_| Box::pin(async { Ok(Decision::deny()) }));

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            authz,
        );

        let err = service
            .update_organization(UpdateOrganizationCommand {
                actor: actor_for(user_id),
                id,
                name: OrganizationName::try_from("Acme Inc.".to_owned()).unwrap(),
                slug: Slug::try_from("acme-inc".to_owned()).unwrap(),
            })
            .await
            .unwrap_err();

        assert!(matches!(err, CoreError::Forbidden { .. }));
    }

    #[tokio::test]
    async fn soft_delete_organization_calls_repo() {
        let id = OrganizationId(Uuid::new_v4());
        let user_id = UserId(Uuid::new_v4());

        let mut organization_repository = MockOrganizationRepository::new();
        let mut role_repository = MockRoleRepository::new();
        let mut member_repository = MockMemberRepository::new();

        stage_org_membership(
            &mut member_repository,
            &mut role_repository,
            id,
            user_id,
            MemberId(Uuid::new_v4()),
            1,
        );

        organization_repository
            .expect_find_by_id()
            .with(eq(id))
            .times(1)
            .returning(move |id| {
                let org = fixture(id);
                Box::pin(async move { Ok(Some(org)) })
            });

        organization_repository
            .expect_soft_delete()
            .withf(move |i, _| *i == id)
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(()) }));

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            allowing_authorizer(),
        );

        service
            .soft_delete_organization(actor_for(user_id), id)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn soft_delete_organization_returns_not_found_when_missing() {
        let id = OrganizationId(Uuid::new_v4());

        let mut organization_repository = MockOrganizationRepository::new();
        let role_repository = MockRoleRepository::new();
        let member_repository = MockMemberRepository::new();

        organization_repository
            .expect_find_by_id()
            .with(eq(id))
            .times(1)
            .returning(|_| Box::pin(async { Ok(None) }));

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            MockAuthorizer::new(),
        );

        let err = service
            .soft_delete_organization(actor_for(UserId(Uuid::new_v4())), id)
            .await
            .unwrap_err();

        assert!(matches!(err, CoreError::NotFound));
    }

    #[tokio::test]
    async fn list_organizations_for_user_delegates_to_repo() {
        let user_id = UserId(Uuid::new_v4());

        let mut organization_repository = MockOrganizationRepository::new();
        let role_repository = MockRoleRepository::new();
        let member_repository = MockMemberRepository::new();

        organization_repository
            .expect_list_for_user()
            .with(eq(user_id))
            .times(1)
            .returning(|_| Box::pin(async { Ok(vec![]) }));

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            MockAuthorizer::new(),
        );

        let orgs = service.list_organizations_for_user(user_id).await.unwrap();

        assert!(orgs.is_empty());
    }

    fn create_cmd() -> CreateOrganizationCommand {
        CreateOrganizationCommand {
            name: OrganizationName::try_from("Acme".to_owned()).unwrap(),
            slug: Slug::try_from("acme".to_owned()).unwrap(),
            owner_id: UserId(Uuid::new_v4()),
        }
    }

    #[tokio::test]
    async fn create_organization_seeds_roles_and_owner_membership() {
        let mut organization_repository = MockOrganizationRepository::new();
        let mut role_repository = MockRoleRepository::new();
        let mut member_repository = MockMemberRepository::new();

        organization_repository
            .expect_insert()
            .times(1)
            .returning(|o| {
                let cloned = Organization {
                    id: o.id,
                    name: o.name.clone(),
                    slug: o.slug.clone(),
                    owner_id: o.owner_id,
                    deleted_at: o.deleted_at,
                    created_at: o.created_at,
                    updated_at: o.updated_at,
                };
                Box::pin(async move { Ok(cloned) })
            });

        role_repository.expect_insert().times(3).returning(|r| {
            let cloned = Role {
                id: r.id,
                organization_id: r.organization_id,
                name: r.name.clone(),
                permissions: r.permissions,
                created_at: r.created_at,
                updated_at: r.updated_at,
            };
            Box::pin(async move { Ok(cloned) })
        });

        member_repository.expect_insert().times(1).returning(|m| {
            let cloned = Member {
                id: m.id,
                organization_id: m.organization_id,
                user_id: m.user_id,
                joined_at: m.joined_at,
            };
            Box::pin(async move { Ok(cloned) })
        });

        member_repository
            .expect_assign_role()
            .times(1)
            .returning(|_, _, _| Box::pin(async { Ok(()) }));

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            MockAuthorizer::new(),
        );

        let org = service.create_organization(create_cmd()).await.unwrap();

        assert_eq!(org.name, "Acme");
        assert_eq!(org.slug, "acme");
        assert!(org.deleted_at.is_none());
    }

    #[tokio::test]
    async fn create_organization_translates_slug_unique_violation_to_business_error() {
        let mut organization_repository = MockOrganizationRepository::new();
        let role_repository = MockRoleRepository::new();
        let member_repository = MockMemberRepository::new();

        // Infra-style payload: constraint name only.
        organization_repository
            .expect_insert()
            .times(1)
            .returning(|_| {
                Box::pin(async { Err(CoreError::Conflict("organizations_slug_key".into())) })
            });

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            MockAuthorizer::new(),
        );

        let err = service.create_organization(create_cmd()).await.unwrap_err();

        match err {
            CoreError::Conflict(msg) => assert_eq!(msg, "slug already taken"),
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn leave_organization_soft_deletes_when_owner_leaves() {
        let owner_id = UserId(Uuid::new_v4());
        let org_id = OrganizationId(Uuid::new_v4());

        let mut organization_repository = MockOrganizationRepository::new();
        let mut role_repository = MockRoleRepository::new();
        let mut member_repository = MockMemberRepository::new();

        // The owner is authorized as a real member, not waved through: leaving
        // enriches twice - once to read the organization, once to delete it.
        stage_org_membership(
            &mut member_repository,
            &mut role_repository,
            org_id,
            owner_id,
            MemberId(Uuid::new_v4()),
            2,
        );

        organization_repository
            .expect_find_by_id()
            .with(eq(org_id))
            .times(2)
            .returning(move |id| {
                let now = Utc::now();
                let org = Organization {
                    id,
                    name: "Acme".into(),
                    slug: "acme".into(),
                    owner_id,
                    deleted_at: None,
                    created_at: now,
                    updated_at: now,
                };
                Box::pin(async move { Ok(Some(org)) })
            });

        organization_repository
            .expect_soft_delete()
            .withf(move |i, _| *i == org_id)
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(()) }));

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            allowing_authorizer(),
        );

        service
            .leave_organization(actor_for(owner_id), org_id, owner_id)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn leave_organization_removes_membership_when_non_owner_leaves() {
        let owner_id = UserId(Uuid::new_v4());
        let leaver_id = UserId(Uuid::new_v4());
        let org_id = OrganizationId(Uuid::new_v4());
        let member_id = MemberId(Uuid::new_v4());

        let mut organization_repository = MockOrganizationRepository::new();
        let mut role_repository = MockRoleRepository::new();
        let mut member_repository = MockMemberRepository::new();

        organization_repository
            .expect_find_by_id()
            .with(eq(org_id))
            .times(1)
            .returning(move |id| {
                let now = Utc::now();
                let org = Organization {
                    id,
                    name: "Acme".into(),
                    slug: "acme".into(),
                    owner_id,
                    deleted_at: None,
                    created_at: now,
                    updated_at: now,
                };
                Box::pin(async move { Ok(Some(org)) })
            });

        // Once by the read guard inside `get_organization`, once by the
        // membership lookup that follows.
        role_repository
            .expect_list_by_organization()
            .with(eq(org_id))
            .times(1)
            .returning(|_| Box::pin(async { Ok(Vec::new()) }));
        member_repository
            .expect_list_role_ids()
            .with(eq(org_id), eq(member_id))
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(Vec::new()) }));
        member_repository
            .expect_find_by_org_and_user()
            .with(eq(org_id), eq(leaver_id))
            .times(2)
            .returning(move |organization_id, user_id| {
                let m = Member {
                    id: member_id,
                    organization_id,
                    user_id,
                    joined_at: Utc::now(),
                };
                Box::pin(async move { Ok(Some(m)) })
            });

        member_repository
            .expect_remove()
            .with(eq(org_id), eq(member_id))
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(()) }));

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            MockAuthorizer::new(),
        );

        service
            .leave_organization(actor_for(leaver_id), org_id, leaver_id)
            .await
            .unwrap();
    }

    /// A caller with no standing in the organization must not be able to read
    /// it. Enrichment is the guard: it refuses before any action is evaluated.
    #[tokio::test]
    async fn get_organization_refuses_a_non_member() {
        let id = OrganizationId(Uuid::new_v4());

        let mut organization_repository = MockOrganizationRepository::new();
        organization_repository
            .expect_find_by_id()
            .returning(move |_| {
                let organization = fixture(id);
                Box::pin(async move { Ok(Some(organization)) })
            });

        let mut member_repository = MockMemberRepository::new();
        member_repository
            .expect_find_by_org_and_user()
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(None) }));

        let mut service = OrganizationService::new(
            organization_repository,
            MockRoleRepository::new(),
            member_repository,
            MockAuthorizer::new(),
        );

        let err = service
            .get_organization(actor_for(UserId(Uuid::new_v4())), id)
            .await
            .unwrap_err();

        assert!(matches!(err, CoreError::Forbidden { .. }));
    }

    /// A member whose roles do not carry the organization-management bit must
    /// not be able to delete it, even though enrichment lets them through.
    #[tokio::test]
    async fn soft_delete_refuses_a_member_without_manage_org() {
        let id = OrganizationId(Uuid::new_v4());
        let user_id = UserId(Uuid::new_v4());
        let member_id = MemberId(Uuid::new_v4());

        let mut organization_repository = MockOrganizationRepository::new();
        organization_repository
            .expect_find_by_id()
            .returning(move |_| {
                let organization = fixture(id);
                Box::pin(async move { Ok(Some(organization)) })
            });
        organization_repository.expect_soft_delete().never();

        let mut member_repository = MockMemberRepository::new();
        let mut role_repository = MockRoleRepository::new();
        stage_org_membership(
            &mut member_repository,
            &mut role_repository,
            id,
            user_id,
            member_id,
            1,
        );

        let mut authz = MockAuthorizer::new();
        authz
            .expect_evaluate()
            .times(1)
            .returning(|_| Box::pin(async { Ok(Decision::deny()) }));

        let mut service = OrganizationService::new(
            organization_repository,
            role_repository,
            member_repository,
            authz,
        );

        let err = service
            .soft_delete_organization(actor_for(user_id), id)
            .await
            .unwrap_err();

        assert!(matches!(err, CoreError::Forbidden { .. }));
    }
}
