//! Authorization helpers shared by every domain service.
//!
//! Two responsibilities:
//!
//! 1. **Subject conventions** — keys under which Lodestone stores its own
//!    properties on the AuthZen [`Subject`] (`lodestone.user_id`).
//!
//! 2. **Decision helpers** — call [`Authorizer::evaluate`] with a properly
//!    shaped request and map the [`Decision`] to a [`CoreError`].

use authz::{
    AccessEvaluationRequest, Action, Authorizer, Resource, SUBJECT_IAM_ROLES_KEY, Subject,
};
use common::CoreError;
use serde_json::json;

use crate::UserId;

/// Subject property carrying the actor's Lodestone `UserId` (UUID string).
pub const SUBJECT_USER_ID_KEY: &str = "lodestone.user_id";

/// Read the Lodestone `UserId` carried by a [`Subject`]. Returns
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

/// Builds a [`Subject`] for a regular Lodestone user. `iam_roles` is the list
/// of realm/IAM roles carried by the JWT, surfaced under `iam.roles`
/// where the [`LocalPolicyEngine`](authz::LocalPolicyEngine) reads them
/// for the super-admin bypass.
pub fn user_subject(user_id: UserId, iam_roles: Vec<String>) -> Subject {
    use authz::SubjectKind;

    Subject::new(SubjectKind::User, user_id.to_string())
        .with_property(SUBJECT_USER_ID_KEY, user_id.to_string())
        .with_property(SUBJECT_IAM_ROLES_KEY, json!(iam_roles))
}
