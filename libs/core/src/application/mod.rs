use std::{sync::Arc, time::Duration};

use auth::{AuthService, FerrisKeyRepository};
use common::{Config, CoreError};
use rate_limit::{InMemoryRateLimiter, Quota, RateLimitService};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::{
    application::{llm::limits::ProviderGates, timeline::in_flight::InFlightRegistry},
    domain::{enrichment::RequalifyPolicy, output::WebhookPolicy},
    infrastructure::{agent_registry::InMemoryAgentRegistry, postgres::error::map_sqlx_error},
};

pub mod agents;
pub mod calibration;
pub mod company;
pub mod contacts;
pub mod drafts;
pub mod dropped;
pub mod ecosystem;
pub mod evaluation;
pub mod jobs;
pub mod leads;
pub mod listeners;
pub mod llm;
pub mod output;
pub mod partners;
pub mod policy;
pub mod probe;
pub mod profile;
pub mod qualification;
pub mod quality;
pub mod rescoring;
pub mod schedule;
pub mod scoring_agent;
pub mod settings;
pub mod source_providers;
pub mod sources;
pub mod suppression;
pub mod timeline;
pub mod triage;
pub mod user;

pub type LodestoneAuthService = AuthService<FerrisKeyRepository>;
pub type LodestoneRateLimitService = RateLimitService<InMemoryRateLimiter>;

#[derive(Clone)]
pub struct LodestoneUseCase {
    pub(crate) pool: PgPool,
    pub(crate) webhooks: Option<WebhookPolicy>,
    pub(crate) listener_stale_after: Duration,
    pub(crate) requalify: RequalifyPolicy,
    pub(crate) agents: Arc<InMemoryAgentRegistry>,
    pub(crate) gates: Arc<ProviderGates>,
    pub(crate) in_flight: Arc<InFlightRegistry>,
}

impl LodestoneUseCase {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            webhooks: None,
            listener_stale_after: crate::domain::listener::STALE_FLOOR,
            requalify: RequalifyPolicy::default(),
            agents: Arc::new(InMemoryAgentRegistry::new()),
            gates: Arc::new(ProviderGates::default()),
            in_flight: Arc::new(InFlightRegistry::default()),
        }
    }

    pub fn agent_registry(&self) -> &InMemoryAgentRegistry {
        &self.agents
    }

    pub fn with_requalify_policy(mut self, policy: RequalifyPolicy) -> Self {
        self.requalify = policy;
        self
    }

    pub fn requalify_policy(&self) -> RequalifyPolicy {
        self.requalify
    }

    pub fn with_listener_stale_after(mut self, stale_after: Duration) -> Self {
        self.listener_stale_after = stale_after;
        self
    }

    pub fn with_webhooks(mut self, policy: Option<WebhookPolicy>) -> Self {
        self.webhooks = policy;
        self
    }

    pub fn webhooks(&self) -> Option<&WebhookPolicy> {
        self.webhooks.as_ref()
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
    let auth_repo = FerrisKeyRepository::new(config.auth.issuer, config.auth.audience);
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
