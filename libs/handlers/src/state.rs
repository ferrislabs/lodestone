use common::Config;
use fetch::{FetchSettings, PoliteFetcher};
use lodestone_core::{
    LodestoneAuthService, LodestoneRateLimitService, LodestoneUseCase, ProfileId, WebhookPolicy,
    create_service, infrastructure::crypto::AesGcmCipher,
};
use rate_limit::Quota;
use server::errors::ServerError;
use source_providers::ProviderEngine;
use std::sync::Arc;

use args::Args;

#[derive(Clone)]
pub struct AppState {
    pub args: Arc<Args>,

    pub auth: LodestoneAuthService,
    pub usecase: LodestoneUseCase,
    pub rate_limit: LodestoneRateLimitService,
    pub rate_limit_quota: Quota,
    pub cipher: Arc<AesGcmCipher>,
    pub providers: Arc<ProviderEngine<PoliteFetcher>>,
}

pub async fn state(args: Arc<Args>) -> Result<AppState, ServerError> {
    let config = Config::try_from(args.as_ref().clone())
        .map_err(|e| ServerError::Configuration(e.to_string()))?;

    let cipher = AesGcmCipher::new(&config.llm.encryption_key)
        .map_err(|e| ServerError::Configuration(e.to_string()))?;

    let fetch_settings = FetchSettings::from_outbound(&config.outbound);
    let allow_http = fetch_settings.allow_private_targets;
    let fetcher = PoliteFetcher::new(fetch_settings)
        .map_err(|e| ServerError::Configuration(e.to_string()))?;
    let providers = ProviderEngine::new(Arc::new(fetcher)).with_allow_http(allow_http);

    let webhooks = args
        .webhook
        .settings()
        .map_err(|e| ServerError::Configuration(e.to_string()))?
        .map(|settings| WebhookPolicy {
            profile_ids: settings.profile_ids.into_iter().map(ProfileId).collect(),
            max_attempts: config.jobs.max_attempts,
        });

    let stale_after = lodestone_core::stale_after(args.listeners.idle());
    let company_wait = args.company.wait();
    let enrichment_wait = args.enrichment.wait();
    let service = create_service(config).await.unwrap();

    Ok(AppState {
        args,
        auth: service.auth,
        usecase: service
            .usecase
            .with_webhooks(webhooks)
            .with_listener_stale_after(stale_after)
            .with_company_wait(company_wait)
            .with_enrichment_wait(enrichment_wait),
        rate_limit: service.rate_limit,
        rate_limit_quota: service.rate_limit_quota,
        cipher: Arc::new(cipher),
        providers: Arc::new(providers),
    })
}
