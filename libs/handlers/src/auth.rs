use auth::Identity;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use http::{HeaderValue, header::AUTHORIZATION};
use oxid_core::{CreateUserCommand, UserId};
use tracing::error;

use crate::{
    errors::{ApiError, MiddlewareError},
    state::AppState,
};

/// Oxid's own identity for the authenticated caller, resolved from the OIDC
/// subject by [`auth_middleware`] and carried in the request extensions.
///
/// Handlers take this rather than reading the subject off the [`Identity`].
/// The subject identifies the caller at the identity provider; [`UserId`]
/// identifies the `users` row that every organization-scoped table points at.
/// They are different values, and conflating them is precisely what this type
/// exists to prevent — which is why no conversion from one to the other is
/// offered anywhere.
///
/// Absent for client credentials: a service account has no `users` row, so
/// handlers requiring a user will not extract it.
#[derive(Debug, Clone, Copy)]
pub struct AuthenticatedUser(pub UserId);

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, MiddlewareError> {
    let auth_header = req
        .headers()
        .get(AUTHORIZATION)
        .ok_or(MiddlewareError::MissingAuthHeader)?;

    let token = extract_bearer(auth_header).map_err(|_| MiddlewareError::InvalidAuthHeader)?;

    let identity = state.auth.get_identity(token).await.map_err(|e| {
        error!("Auth middleware: failed to identify user {:?}", e);
        MiddlewareError::AuthenticationFailed(e.to_string())
    })?;

    if let Identity::User(user) = &identity {
        let name = user.name.as_deref().unwrap_or_else(|| &user.username);

        // Resolving the subject to a `users` row is what makes the caller
        // addressable in Oxid's own tables. A failure here leaves us unable to
        // say who is calling, so it must stop the request rather than let a
        // handler run against an identity it cannot map.
        let resolved = state
            .usecase
            .create_user(CreateUserCommand {
                name: name.to_string(),
                username: user.username.clone(),
                // Absent stays absent. Substituting a constant used to collapse
                // every email-less identity onto one row.
                email: user.email.clone(),
                sub: user.id.clone(),
            })
            .await
            .map_err(|err| {
                error!("auth middleware: failed to resolve user {:?}", err);
                MiddlewareError::IdentityResolution
            })?;

        req.extensions_mut().insert(AuthenticatedUser(resolved.id));
    }

    req.extensions_mut().insert(identity);

    Ok(next.run(req).await)
}

pub fn extract_bearer(auth_header: &HeaderValue) -> Result<&str, ApiError> {
    auth_header
        .to_str()
        .map_err(|_| ApiError::TokenNotFound)?
        .strip_prefix("Bearer ")
        .ok_or(ApiError::TokenNotFound)
}
