use axum::{Extension, Json, extract::State};
use handlers::{ApiError, AppState, AuthenticatedUser, DataEnvelope, Response};
use oxid_core::{CreateOrganizationCommand, OrganizationName, Slug};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{paths::OrganizationsPath, response::OrganizationResponse};

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub slug: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/organizations",
    operation_id = "createOrganization",
    tag = super::TAG,
    request_body = CreateOrganizationRequest,
    responses(
        (status = 201, description = "Organization created", body = inline(DataEnvelope<OrganizationResponse>)),
        (status = 400, description = "Validation failed"),
        (status = 401, description = "Unauthorized"),
        (status = 409, description = "Slug already taken"),
    ),
    security(("bearer_auth" = []))
)]
#[tracing::instrument(skip_all, fields(slug = %payload.slug), err)]
pub async fn handler(
    _: OrganizationsPath,
    State(state): State<AppState>,
    Extension(AuthenticatedUser(owner_id)): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateOrganizationRequest>,
) -> Result<Response<OrganizationResponse>, ApiError> {
    let command = CreateOrganizationCommand {
        name: OrganizationName::try_from(payload.name)
            .map_err(|e| ApiError::Validation(e.to_string()))?,
        slug: Slug::try_from(payload.slug).map_err(|e| ApiError::Validation(e.to_string()))?,
        owner_id,
    };

    let org = state.usecase.create_organization(command).await?;

    Ok(Response::Created(org.into()))
}
