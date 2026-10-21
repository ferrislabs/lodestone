use axum::{Extension, Json, extract::State};
use chrono::Utc;
use common::RobotsPolicy;
use lodestone_core::{CrawlSettings, UserId, settings::CrawlSettingsUpdate};
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::ToSchema;

use crate::{
    auth::AuthenticatedUser,
    errors::{ApiError, ErrorDetail},
    response::{DataEnvelope, Response},
    state::AppState,
};

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct CrawlSettingsDto {
    pub robots_policy: String,
    pub origin: String,
    pub environment_default: String,
    pub updated_by: Option<String>,
    pub updated_by_name: Option<String>,
    #[schema(format = DateTime)]
    pub updated_at: Option<String>,
    pub robots_ignored: u64,
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct CrawlSettingsUpdateDto {
    pub settings: CrawlSettingsDto,
    pub dossiers_retried: u64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateCrawlSettingsRequest {
    pub robots_policy: String,
}

fn actor(user: Option<Extension<AuthenticatedUser>>) -> Result<UserId, ApiError> {
    match user {
        Some(Extension(AuthenticatedUser(id))) => Ok(id),
        None => Err(ApiError::Forbidden),
    }
}

fn dto(state: &AppState, settings: CrawlSettings) -> CrawlSettingsDto {
    CrawlSettingsDto {
        robots_policy: settings.robots_policy.as_str().to_owned(),
        origin: settings.origin.as_str().to_owned(),
        environment_default: state.args.crawl.robots_policy.as_str().to_owned(),
        updated_by: settings.updated_by.map(|user| user.to_string()),
        updated_by_name: settings.updated_by_name,
        updated_at: settings.updated_at.map(|at| at.to_rfc3339()),
        robots_ignored: state.robots.robots_ignored(),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/settings/crawl",
    tag = "settings",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "The organisation-wide collection rules, with who saved them and when", body = DataEnvelope<CrawlSettingsDto>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
    )
)]
pub async fn get_crawl_settings(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
) -> Result<Response<CrawlSettingsDto>, ApiError> {
    actor(user)?;
    let settings = state
        .usecase
        .crawl_settings(state.args.crawl.robots_policy)
        .await?;
    Ok(Response::OK(dto(&state, settings)))
}

#[utoipa::path(
    put,
    path = "/api/v1/settings/crawl",
    tag = "settings",
    security(("bearer_auth" = [])),
    request_body = UpdateCrawlSettingsRequest,
    responses(
        (status = 200, description = "The policy was saved with the caller and the time and applies to the next request; switching to `ignore` schedules the dossiers refused by robots.txt for a new attempt", body = DataEnvelope<CrawlSettingsUpdateDto>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 422, description = "The policy is neither `respect` nor `ignore`"),
    )
)]
pub async fn put_crawl_settings(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Json(body): Json<UpdateCrawlSettingsRequest>,
) -> Result<Response<CrawlSettingsUpdateDto>, ApiError> {
    let user = actor(user)?;
    let policy = RobotsPolicy::parse(&body.robots_policy).ok_or_else(|| {
        ApiError::Rejected(vec![ErrorDetail {
            path: "robots_policy".to_owned(),
            message: "must be `respect` or `ignore`".to_owned(),
        }])
    })?;
    let CrawlSettingsUpdate {
        settings,
        dossiers_retried,
    } = state
        .usecase
        .update_crawl_settings(policy, user, Utc::now(), state.args.crawl.robots_policy)
        .await?;
    state.robots.set(settings.robots_policy);
    info!(
        robots_policy = settings.robots_policy.as_str(),
        %user,
        dossiers_retried,
        "crawl robots policy saved"
    );
    Ok(Response::OK(CrawlSettingsUpdateDto {
        settings: dto(&state, settings),
        dossiers_retried,
    }))
}
