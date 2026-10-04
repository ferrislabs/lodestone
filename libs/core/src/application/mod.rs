use auth::{AuthService, FerrisKeyRepository};
use common::{Config, CoreError};
use rate_limit::{InMemoryRateLimiter, Quota, RateLimitService};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::infrastructure::postgres::error::map_sqlx_error;

pub mod contacts;
pub mod jobs;
pub mod llm;
pub mod partners;
pub mod policy;
pub mod probe;
pub mod profile;
pub mod qualification;
pub mod sources;
pub mod suppression;
pub mod triage;
pub mod user;

pub type LodestoneAuthService = AuthService<FerrisKeyRepository>;
pub type LodestoneRateLimitService = RateLimitService<InMemoryRateLimiter>;

#[derive(Clone)]
pub struct LodestoneUseCase {
    pub(crate) pool: PgPool,
}

impl LodestoneUseCase {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[derive(Clone)]
pub struct LodestoneService {
    pub auth: LodestoneAuthService,
    pub usecase: LodestoneUseCase,
    pub rate_limit: LodestoneRateLimitService,
    pub rate_limit_quota: Quota,
}

impl LodestoneService {
    pub fn new(
        auth: LodestoneAuthService,
        usecase: LodestoneUseCase,
        rate_limit: LodestoneRateLimitService,
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

pub async fn create_service(config: Config) -> Result<LodestoneService, CoreError> {
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

    let rate_limit = RateLimitService::new(InMemoryRateLimiter::new());
    let rate_limit_quota = Quota::per_minute(config.rate_limit.per_minute);

    Ok(LodestoneService::new(
        auth,
        LodestoneUseCase::new(pool),
        rate_limit,
        rate_limit_quota,
    ))
}
