use auth::Identity;
use axum::{Extension, Json, extract::State};
use handlers::{ApiError, AppState, AuthenticatedUser, DataEnvelope, Response};
use oxid_core::{
    OrganizationId, OrganizationName, Slug, UpdateOrganizationCommand, application::policy,
};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{paths::OrganizationPath, response::OrganizationResponse};

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateOrganizationRequest {
    pub name: String,
    pub slug: String,
}

#[utoipa::path(
    patch,
    path = "/api/v1/organizations/{organization_id}",
    operation_id = "updateOrganization",
    tag = super::TAG,
    params(
        ("organization_id" = OrganizationId, Path, description = "Organization identifier"),
    ),
    request_body = UpdateOrganizationRequest,
    responses(
        (status = 200, description = "Organization updated", body = inline(DataEnvelope<OrganizationResponse>)),
        (status = 400, description = "Validation failed"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Organization not found"),
        (status = 409, description = "Slug already taken"),
    ),
    security(("bearer_auth" = []))
)]
#[tracing::instrument(skip_all, err)]
pub async fn handler(
    OrganizationPath { organization_id }: OrganizationPath,
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Extension(AuthenticatedUser(user_id)): Extension<AuthenticatedUser>,
    Json(payload): Json<UpdateOrganizationRequest>,
) -> Result<Response<OrganizationResponse>, ApiError> {
    let iam_roles = match &identity {
        Identity::User(u) => u.roles.clone(),
        Identity::Client(c) => c.roles.clone(),
    };
    let actor = policy::user_subject(user_id, iam_roles);

    let organization = state
        .usecase
        .update_organization(UpdateOrganizationCommand {
            actor,
            id: organization_id,
            name: OrganizationName::try_from(payload.name)
                .map_err(|e| ApiError::Validation(e.to_string()))?,
            slug: Slug::try_from(payload.slug).map_err(|e| ApiError::Validation(e.to_string()))?,
        })
        .await?;

    Ok(Response::OK(organization.into()))
}
