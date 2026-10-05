use auth::Identity;
use axum::{Extension, Router, middleware::from_fn_with_state, routing::get};
use lodestone_core::UserId;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    auth::{AuthenticatedUser, auth_middleware},
    errors::ApiError,
    response::{DataEnvelope, Response},
    state::AppState,
};

pub const API_PREFIX: &str = "/api/v1";

pub fn router(state: AppState) -> Router<AppState> {
    let routes = Router::new().route("/me", get(me));

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
