use axum::extract::State;
use lodestone_core::{Profile, ProfileSummary};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    errors::ApiError,
    response::{DataEnvelope, Response},
    state::AppState,
};

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct ProfileRefDto {
    pub id: String,
    pub name: String,
}

impl From<&Profile> for ProfileRefDto {
    fn from(profile: &Profile) -> Self {
        Self {
            id: profile.id.to_string(),
            name: profile.name.as_str().to_owned(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct ProfileSummaryDto {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub revision: Option<u32>,
    pub lead_count: u64,
}

impl From<&ProfileSummary> for ProfileSummaryDto {
    fn from(summary: &ProfileSummary) -> Self {
        Self {
            id: summary.id.to_string(),
            name: summary.name.as_str().to_owned(),
            active: summary.active,
            revision: summary.revision.map(|revision| revision.get()),
            lead_count: summary.lead_count,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/profiles",
    tag = "profiles",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Detection profiles with their current revision and visible lead count", body = DataEnvelope<Vec<ProfileSummaryDto>>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
    )
)]
pub async fn list_profiles(
    State(state): State<AppState>,
) -> Result<Response<Vec<ProfileSummaryDto>>, ApiError> {
    let summaries = state.usecase.list_profiles_summary().await?;
    Ok(Response::OK(
        summaries.iter().map(ProfileSummaryDto::from).collect(),
    ))
}
