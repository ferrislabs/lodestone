use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{get, patch, post},
};
use axum_extra::routing::TypedPath;
use handlers::{AppState, auth::auth_middleware};

use crate::paths::{
    CurrentUserBootstrapPath, CurrentUserOrganizationsPath, OrganizationPath, OrganizationsPath,
};

pub mod bootstrap;
pub mod create;
pub mod list_mine;
pub mod paths;
pub mod response;
pub mod update;

pub const TAG: &str = "organizations";

pub fn router(state: &AppState) -> Router<AppState> {
    Router::new()
        .route(OrganizationsPath::PATH, post(create::handler))
        // Reading and deleting an organization are implemented in the service
        // but have no handler yet. They stay unmounted rather than routed to a
        // panicking stub: a 405 is an answer, a dropped connection is not.
        .route(OrganizationPath::PATH, patch(update::handler))
        .route(CurrentUserOrganizationsPath::PATH, get(list_mine::handler))
        // Explicit and called once, rather than a side effect of every
        // authenticated request: provisioning already runs there and adding a
        // write per call is what this deliberately avoids.
        .route(CurrentUserBootstrapPath::PATH, post(bootstrap::handler))
        .layer(from_fn_with_state(state.clone(), auth_middleware))
}
