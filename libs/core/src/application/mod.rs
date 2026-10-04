use auth::{AuthService, FerrisKeyRepository};
use authz::LocalPolicyEngine;
use common::{Config, CoreError};
use rate_limit::{Quota, RateLimitService, RedisRateLimiter};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::domain::role::Permissions;
use crate::infrastructure::postgres::error::map_sqlx_error;

pub mod member;
pub mod organization;
pub mod policy;
pub mod role;
pub mod user;

pub type OxidAuthService = AuthService<FerrisKeyRepository>;
pub type OxidRateLimitService = RateLimitService<RedisRateLimiter>;

/// In-process Policy Decision Point used by Oxid's services. Aliased so
/// callers can swap the concrete engine later (e.g. for a remote PDP)
/// with a single type change.
pub type OxidAuthorizer = LocalPolicyEngine;

/// Builds the default action → required permission bits map. The bit
/// values come from [`Permissions`] so the service-side bitfield stays
/// the single source of truth.
pub fn default_authorizer() -> OxidAuthorizer {
    LocalPolicyEngine::builder()
        .action("organization.update", Permissions::MANAGE_ORG.0)
        .action("organization.delete", Permissions::MANAGE_ORG.0)
        .action("member.invite", Permissions::MANAGE_MEMBERS.0)
        .action("member.remove", Permissions::MANAGE_MEMBERS.0)
        .action("role.assign", Permissions::MANAGE_ROLES.0)
        .action("role.manage", Permissions::MANAGE_ROLES.0)
        .build()
}

#[derive(Clone)]
pub struct OxidUseCase {
    pub(crate) pool: PgPool,
    pub(crate) authz: OxidAuthorizer,
}

impl OxidUseCase {
    pub fn new(pool: PgPool, authz: OxidAuthorizer) -> Self {
        Self { pool, authz }
    }
}

#[derive(Clone)]
pub struct OxidService {
    pub auth: OxidAuthService,
    pub usecase: OxidUseCase,
    pub rate_limit: OxidRateLimitService,
    pub rate_limit_quota: Quota,
}

impl OxidService {
    pub fn new(
        auth: OxidAuthService,
        usecase: OxidUseCase,
        rate_limit: OxidRateLimitService,
        rate_limit_quota: Quota,
    ) -> Self {
        Self {
            auth,
            usecase,
            rate_limit,
            rate_limit_quota,
        }
    }
}

pub async fn create_service(config: Config) -> Result<OxidService, CoreError> {
    let auth_repo = FerrisKeyRepository::new(config.auth.issuer, None);
    let auth = AuthService::new(auth_repo);

    let db_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        config.database.username,
        config.database.password.expose(),
        config.database.host,
        config.database.port,
        config.database.name,
    );
    let pool = PgPoolOptions::new()
        .connect(&db_url)
        .await
        .map_err(map_sqlx_error)?;

    let limiter = RedisRateLimiter::connect(config.rate_limit.redis_url.expose())
        .await
        // The URL may carry a password, and this message reaches a panic
        // at boot. Report the failure, not the credential.
        .map_err(|_| CoreError::Internal("redis connection failed".to_owned()))?;
    let rate_limit = RateLimitService::new(limiter);
    let rate_limit_quota = Quota::per_minute(config.rate_limit.per_minute);

    Ok(OxidService::new(
        auth,
        OxidUseCase::new(pool, default_authorizer()),
        rate_limit,
        rate_limit_quota,
    ))
}
