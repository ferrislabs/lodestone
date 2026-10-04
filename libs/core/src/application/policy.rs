//! Authorization helpers shared by every domain service.
//!
//! Two responsibilities:
//!
//! 1. **Subject conventions** — keys under which Oxid stores its own
//!    properties on the AuthZen [`Subject`] (`oxid.user_id`,
//!    `oxid.member_id`, `oxid.permissions`, `oxid.organization_id`).
//!    Centralizing them here keeps the format consistent and lets the
//!    [`LocalPolicyEngine`](authz::LocalPolicyEngine) read them with
//!    matching constants.
//!
//! 2. **Enrichment & decision helpers** — load the actor's organization
//!    membership + aggregated permission bitfield, then call
//!    [`Authorizer::evaluate`] with a properly shaped request and map
//!    the [`Decision`] to a [`CoreError`].

use authz::{
    AccessEvaluationRequest, Action, Authorizer, DEFAULT_SUPER_ADMIN_ROLE, Resource,
    SUBJECT_IAM_ROLES_KEY, SUBJECT_PERMISSIONS_KEY, Subject,
};
use common::CoreError;
use serde_json::json;

use crate::{
    UserId,
    domain::{
        member::ports::MemberRepository,
        organization::OrganizationId,
        role::{Permissions, ports::RoleRepository},
    },
};

/// Subject property carrying the actor's Oxid `UserId` (UUID string).
pub const SUBJECT_USER_ID_KEY: &str = "oxid.user_id";

/// Subject property carrying the actor's `MemberId` for the org context.
pub const SUBJECT_MEMBER_ID_KEY: &str = "oxid.member_id";

/// Subject property carrying the `OrganizationId` of the org context.
pub const SUBJECT_ORGANIZATION_ID_KEY: &str = "oxid.organization_id";

/// Re-exported for callers that build subjects without going through
/// [`enrich_for_organization`] (e.g. unit tests).
pub use authz::SUBJECT_PERMISSIONS_KEY as SUBJECT_PERMISSIONS_KEY_REEXPORT;

/// Read the Oxid `UserId` carried by a [`Subject`]. Returns
/// [`CoreError::Internal`] when missing or malformed — handlers must
/// always set this property before calling into a service.
pub fn subject_user_id(subject: &Subject) -> Result<UserId, CoreError> {
    use std::str::FromStr;

    let raw = subject
        .properties
        .get(SUBJECT_USER_ID_KEY)
        .and_then(|v| v.as_str())
        .ok_or_else(|| CoreError::Internal(format!("subject missing `{SUBJECT_USER_ID_KEY}`")))?;
    UserId::from_str(raw).map_err(|e| {
        CoreError::Internal(format!(
            "subject `{SUBJECT_USER_ID_KEY}` is not a UUID: {e}"
        ))
    })
}

/// Loads `(member, aggregated permissions)` for the user behind the
/// subject in the given organization, and writes the AuthZen-shaped
/// properties (`oxid.member_id`, `oxid.organization_id`,
/// `oxid.permissions`) onto a clone of the subject.
///
/// Returns [`CoreError::Forbidden`] when the user is not a member of the
/// organization — the actor has authenticated but has no standing in
/// this org, which is an authorization failure (not a 404 — that would
/// leak existence). System subjects are short-circuited (no DB load).
/// Whether the subject carries the super-admin realm role.
///
/// Enrichment has to know: it refuses a non-member before the policy engine is
/// ever consulted, so a super-admin who belongs to no organization would be
/// turned away by the very step meant to prepare their request. The bypass
/// lives in the engine, but it is unreachable unless this lets them through.
///
/// Caveat: the role name is read from the shared default. An engine built with
/// a custom `super_admin_role` would diverge from this check - the two should
/// be given a single source before that configuration is used.
fn carries_super_admin(subject: &Subject) -> bool {
    subject
        .properties
        .get(SUBJECT_IAM_ROLES_KEY)
        .and_then(|roles| roles.as_array())
        .is_some_and(|roles| {
            roles
                .iter()
                .filter_map(|role| role.as_str())
                .any(|role| role == DEFAULT_SUPER_ADMIN_ROLE)
        })
}

pub async fn enrich_for_organization<M, R>(
    subject: Subject,
    organization_id: OrganizationId,
    members: &mut M,
    roles: &mut R,
) -> Result<Subject, CoreError>
where
    M: MemberRepository,
    R: RoleRepository,
{
    if subject.is_system() || carries_super_admin(&subject) {
        return Ok(subject);
    }

    let user_id = subject_user_id(&subject)?;

    let member = members
        .find_by_org_and_user(organization_id, user_id)
        .await?
        .ok_or(CoreError::Forbidden {
            reason: Some("not a member of this organization".to_owned()),
        })?;

    let role_ids = members.list_role_ids(organization_id, member.id).await?;
    let org_roles = roles.list_by_organization(organization_id).await?;

    let aggregated = org_roles
        .iter()
        .filter(|r| role_ids.contains(&r.id))
        .map(|r| r.permissions)
        .fold(Permissions::NONE, |acc, p| acc | p);

    let mut enriched = subject;
    enriched.properties.insert(
        SUBJECT_MEMBER_ID_KEY.to_owned(),
        json!(member.id.0.to_string()),
    );
    enriched.properties.insert(
        SUBJECT_ORGANIZATION_ID_KEY.to_owned(),
        json!(organization_id.0.to_string()),
    );
    enriched
        .properties
        .insert(SUBJECT_PERMISSIONS_KEY.to_owned(), json!(aggregated.bits()));

    Ok(enriched)
}

/// Calls the authorizer with `(subject, action, resource)` and converts
/// a deny decision into [`CoreError::Forbidden`]. PDP-side failures
/// surface as [`CoreError::Internal`] (the PDP itself is misbehaving;
/// that should not look like a denial to the caller).
pub async fn require<A: Authorizer>(
    authorizer: &A,
    subject: &Subject,
    action: &str,
    resource: Resource,
) -> Result<(), CoreError> {
    let request = AccessEvaluationRequest::new(subject.clone(), Action::new(action), resource);

    let decision = authorizer
        .evaluate(&request)
        .await
        .map_err(|e| CoreError::Internal(format!("authorization engine error: {e}")))?;

    if decision.is_allowed() {
        return Ok(());
    }

    let reason = decision
        .context
        .as_ref()
        .and_then(|c| c.0.get("reason"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned());

    Err(CoreError::Forbidden { reason })
}

/// Builds a [`Subject`] for a regular Oxid user. `iam_roles` is the list
/// of realm/IAM roles carried by the JWT, surfaced under `iam.roles`
/// where the [`LocalPolicyEngine`](authz::LocalPolicyEngine) reads them
/// for the super-admin bypass.
pub fn user_subject(user_id: UserId, iam_roles: Vec<String>) -> Subject {
    use authz::SubjectKind;

    Subject::new(SubjectKind::User, user_id.to_string())
        .with_property(SUBJECT_USER_ID_KEY, user_id.to_string())
        .with_property(SUBJECT_IAM_ROLES_KEY, json!(iam_roles))
}
