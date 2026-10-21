use auth::Identity;
use axum::{
    Extension, Router,
    middleware::from_fn_with_state,
    routing::{get, patch, post},
};
use lodestone_core::UserId;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    auth::{AuthenticatedUser, auth_middleware},
    calibration, company, drafts,
    errors::ApiError,
    exports, leads, llm, profiles, quality,
    response::{DataEnvelope, Response},
    sources,
    state::AppState,
};

pub const API_PREFIX: &str = "/api/v1";

pub fn router(state: AppState) -> Router<AppState> {
    let routes = Router::new()
        .route("/me", get(me))
        .route(
            "/profiles",
            get(profiles::list_profiles).post(profiles::create_profile),
        )
        .route("/profiles/import", post(profiles::import_profile))
        .route(
            "/profiles/{profile_id}",
            get(profiles::get_profile).put(profiles::update_profile),
        )
        .route(
            "/profiles/{profile_id}/revisions",
            get(profiles::list_revisions),
        )
        .route(
            "/profiles/{profile_id}/revisions/{number}",
            get(profiles::get_revision),
        )
        .route(
            "/profiles/{profile_id}/activate",
            post(profiles::activate_profile),
        )
        .route(
            "/profiles/{profile_id}/deactivate",
            post(profiles::deactivate_profile),
        )
        .route(
            "/profiles/{profile_id}/export",
            get(profiles::export_profile),
        )
        .route("/profiles/{profile_id}/leads", get(leads::list_leads))
        .route(
            "/profiles/{profile_id}/leads/export",
            get(exports::export_leads),
        )
        .route(
            "/llm/providers",
            get(llm::list_providers).post(llm::create_provider),
        )
        .route(
            "/llm/providers/{provider_id}",
            get(llm::get_provider)
                .patch(llm::update_provider)
                .delete(llm::delete_provider),
        )
        .route(
            "/llm/providers/{provider_id}/enable",
            post(llm::enable_provider),
        )
        .route(
            "/llm/providers/{provider_id}/disable",
            post(llm::disable_provider),
        )
        .route(
            "/llm/providers/{provider_id}/test",
            post(llm::test_provider),
        )
        .route("/llm/tiers", get(llm::list_tiers))
        .route(
            "/llm/tiers/{tier}",
            axum::routing::put(llm::assign_tier).delete(llm::unassign_tier),
        )
        .route("/sources", get(sources::list_sources))
        .route("/sources/{source}/run", post(sources::run_source))
        .route("/drafts", get(drafts::list_drafts))
        .route(
            "/drafts/{assessment_id}",
            get(drafts::get_draft).patch(drafts::edit_draft),
        )
        .route(
            "/drafts/{assessment_id}/approve",
            post(drafts::approve_draft),
        )
        .route("/drafts/{assessment_id}/reject", post(drafts::reject_draft))
        .route("/quality", get(quality::quality))
        .route("/quality/calibration", get(calibration::calibration))
        .route(
            "/quality/calibration/run",
            post(calibration::run_calibration),
        )
        .route("/leads/{lead_id}", get(leads::lead_detail))
        .route("/leads/{lead_id}/status", patch(leads::change_lead_status))
        .route(
            "/leads/{lead_id}/summary/retry",
            post(leads::retry_lead_summary),
        )
        .route(
            "/leads/{lead_id}/agents/retry",
            post(leads::retry_lead_agents),
        )
        .route(
            "/leads/{lead_id}/company/refresh",
            post(company::refresh_company),
        );

    Router::new()
        .nest(API_PREFIX, routes)
        .route_layer(from_fn_with_state(state, auth_middleware))
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MeResponse {
    User { user_id: UserId, subject: String },
    Client { client_id: String },
}

#[utoipa::path(
    get,
    path = "/api/v1/me",
    tag = "me",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "The authenticated caller", body = DataEnvelope<MeResponse>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
    )
)]
pub async fn me(
    Extension(identity): Extension<Identity>,
    user: Option<Extension<AuthenticatedUser>>,
) -> Result<Response<MeResponse>, ApiError> {
    match (identity, user) {
        (Identity::User(subject), Some(Extension(AuthenticatedUser(user_id)))) => {
            Ok(Response::OK(MeResponse::User {
                user_id,
                subject: subject.id,
            }))
        }
        (Identity::Client(client), _) => Ok(Response::OK(MeResponse::Client {
            client_id: client.client_id,
        })),
        (Identity::User(_), None) => Err(ApiError::Internal),
    }
}
