use common::Config;
use dns::{DnsSettings, HickoryResolver};
use fetch::{FetchSettings, PoliteFetcher, RobotsGate};
use lodestone_core::{
    BODACC, COMPANY_RESEARCH, DECP, LodestoneAuthService, LodestoneRateLimitService,
    LodestoneUseCase, ProfileId, RequalifyPolicy, WebhookPolicy, create_service,
    infrastructure::crypto::AesGcmCipher,
};
use rate_limit::Quota;
use server::errors::ServerError;
use source_providers::ProviderEngine;
use sources::{
    bodacc::BodaccSettings,
    company::CompanySettings,
    decp::holders::HolderSettings,
    ecosystem::{EcosystemConfig, EcosystemModules, WebsiteSettings},
    endpoint::Endpoint,
};
use std::sync::Arc;
use tracing::warn;

const ECOSYSTEM_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

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
    pub ecosystem: Arc<Ecosystem>,
    pub robots: RobotsGate,
}

pub type Ecosystem = EcosystemModules<PoliteFetcher, HickoryResolver>;

pub fn ecosystem_modules(
    args: &Args,
    usecase: &LodestoneUseCase,
    fetcher: Arc<PoliteFetcher>,
    resolver: Arc<HickoryResolver>,
) -> Ecosystem {
    let wait = args.contacts.max_rate_limit_wait();
    EcosystemModules::new(
        fetcher,
        resolver,
        EcosystemConfig {
            company: CompanySettings {
                entreprises_url: Endpoint::catalog(
                    usecase.clone(),
                    COMPANY_RESEARCH,
                    "entreprises_url",
                    args.decp.entreprises_url.clone(),
                ),
                max_pages: args.company.max_pages.get(),
                max_rate_limit_wait: wait,
                origin: None,
            },
            bodacc: BodaccSettings {
                dataset_url: Endpoint::catalog(
                    usecase.clone(),
                    BODACC,
                    "base_url",
                    args.bodacc.url.clone(),
                ),
                max_rate_limit_wait: wait,
            },
            holders: HolderSettings {
                holders_url: Endpoint::catalog(
                    usecase.clone(),
                    DECP,
                    "holders_url",
                    args.decp.holders_url.clone(),
                ),
                max_rate_limit_wait: wait,
            },
            website: WebsiteSettings {
                max_rate_limit_wait: wait,
                origin: None,
            },
            max_career_paths: args.ats.max_career_paths.get(),
            max_rate_limit_wait: wait,
        },
    )
}

pub async fn state(args: Arc<Args>) -> Result<AppState, ServerError> {
    let config = Config::try_from(args.as_ref().clone())
        .map_err(|e| ServerError::Configuration(e.to_string()))?;

    let cipher = AesGcmCipher::new(&config.llm.encryption_key)
        .map_err(|e| ServerError::Configuration(e.to_string()))?;

    let robots = RobotsGate::new(args.crawl.robots_policy);
    let fetch_settings = FetchSettings::from_outbound(&config.outbound);
    let mut ecosystem_fetch = fetch_settings.clone();
    ecosystem_fetch.max_body_bytes = ecosystem_fetch.max_body_bytes.max(ECOSYSTEM_MAX_BODY_BYTES);
    let ecosystem_fetcher = Arc::new(
        PoliteFetcher::new(ecosystem_fetch)
            .map_err(|e| ServerError::Configuration(e.to_string()))?
            .with_robots_gate(robots.clone()),
    );
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
    let requalify = RequalifyPolicy::from_std(
        args.enrichment.requalify_debounce(),
        args.enrichment.requalify_max_per_day,
    );
    let service = create_service(config).await.unwrap();
    match service
        .usecase
        .crawl_settings(args.crawl.robots_policy)
        .await
    {
        Ok(settings) => {
            robots.set(settings.robots_policy);
        }
        Err(error) => {
            warn!(%error, "crawl settings unreadable, CRAWL_ROBOTS_POLICY applies until the next refresh");
        }
    }
    let resolver = HickoryResolver::from_system(DnsSettings::default())
        .map_err(|e| ServerError::Configuration(e.to_string()))?;
    let ecosystem = ecosystem_modules(
        &args,
        &service.usecase,
        ecosystem_fetcher,
        Arc::new(resolver),
    );

    Ok(AppState {
        args,
        auth: service.auth,
        usecase: service
            .usecase
            .with_webhooks(webhooks)
            .with_listener_stale_after(stale_after)
            .with_requalify_policy(requalify),
        rate_limit: service.rate_limit,
        rate_limit_quota: service.rate_limit_quota,
        cipher: Arc::new(cipher),
        providers: Arc::new(providers),
        ecosystem: Arc::new(ecosystem),
        robots,
    })
}
