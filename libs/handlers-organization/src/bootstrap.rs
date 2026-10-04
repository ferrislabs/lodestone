use auth::Identity;
use axum::Extension;
use axum::extract::State;
use handlers::{ApiError, AppState, AuthenticatedUser, DataEnvelope, Response};

use crate::{paths::CurrentUserBootstrapPath, response::OrganizationResponse};

#[utoipa::path(
    post,
    path = "/api/v1/users/@me/bootstrap",
    operation_id = "bootstrapCurrentUser",
    tag = super::TAG,
    responses(
        (status = 200, description = "The caller's organization, created if they had none", body = inline(DataEnvelope<OrganizationResponse>)),
        (status = 401, description = "Unauthorized"),
        (status = 409, description = "No organization name could be derived from the account"),
    ),
    security(("bearer_auth" = []))
)]
#[tracing::instrument(skip_all, err)]
pub async fn handler(
    _: CurrentUserBootstrapPath,
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Extension(AuthenticatedUser(user_id)): Extension<AuthenticatedUser>,
) -> Result<Response<OrganizationResponse>, ApiError> {
    // The name comes from the account, so a first login lands on a usable
    // workspace instead of an empty creation form. Idempotent: calling it again
    // returns the same organization.
    let seed = identity.username().to_owned();

    let organization = state
        .usecase
        .ensure_default_organization(user_id, seed)
        .await?;

    Ok(Response::OK(organization.into()))
}
