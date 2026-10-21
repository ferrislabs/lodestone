use std::{error::Error, net::SocketAddr, num::NonZeroU32, sync::Arc, time::Duration};

use api::{
    internal_router::internal_router,
    jobs::{
        ContactFinders, Intervals, JobRouter, LlmRuntime, WiredSources, schedule_wired_sources,
    },
    logger::init_tracing_and_logging,
    router::router,
    webhook::WebhookSink,
};
use args::Args;
use axum::Router;
use axum_server::Handle;
use clap::Parser;
use common::{JobsConfig, LlmConfig, OutboundConfig};
use dns::{DnsSettings, HickoryResolver};
use dotenvy::dotenv;
use fetch::{FetchSettings, PoliteFetcher};
use handlers::state;
use lodestone_core::{
    RetryPolicy,
    contacts::ContactRunSettings,
    infrastructure::{
        candidate_host::postgres::PgCandidateHosts,
        contact_target::postgres::{PgAtsTargets, PgContactTargets},
        crypto::AesGcmCipher,
        ct_cursor::postgres::PgCtCursors,
        job::postgres::PgJobQueue,
        llm_cache::postgres::PgLlmCache,
    },
    jobs::{SystemClock, WorkerPool, WorkerSettings},
};
use server::{get_addr, run_server, shutdown_signal};
use sources::{
    ats::{AtsSettings, AtsSource},
    boamp::{BoampSettings, BoampSource},
    contact_pages::{ContactPageSettings, PublicPageFinder},
    ct::{CtSettings, CtSource},
    decp::{DecpSettings, DecpSource},
    dns_cname::{DnsCnameSettings, DnsCnameSource},
    france_travail::{FranceTravailSettings, FranceTravailSource},
    http_probe::{HttpProbeSettings, HttpProbeSource, ProbeScheme},
    role_guess::RoleGuessFinder,
    security_txt::SecurityTxtFinder,
    ted::{TedSettings, TedSource},
};
use telemetry::Metrics;
use tokio::sync::watch;
use tracing::{error, info};

const DNS_BATCH_SIZE: u32 = 500;
const DNS_CONCURRENCY: usize = 16;
const PROBE_BATCH_SIZE: u32 = 200;
const PROBE_CONCURRENCY: usize = 8;
const BOAMP_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
const BOAMP_REQUESTS_PER_MINUTE: u32 = 30;
const TED_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
const TED_REQUESTS_PER_MINUTE: u32 = 30;
const FRANCE_TRAVAIL_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
const FRANCE_TRAVAIL_REQUESTS_PER_MINUTE: u32 = 180;
const ATS_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
const ATS_MAX_RATE_LIMIT_WAIT: Duration = Duration::from_secs(75);
const DECP_REQUESTS_PER_MINUTE: u32 = 120;
const DECP_REQUEST_TIMEOUT: Duration = Duration::from_secs(900);
const DECP_MAX_RATE_LIMIT_WAIT: Duration = Duration::from_secs(75);
const CT_LOG_LIST_TTL: Duration = Duration::from_secs(6 * 3600);

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let args = Arc::new(Args::parse());
    let observability = init_tracing_and_logging(&args.log, "lodestone", &args.observability)?;

    let jobs_config = JobsConfig::try_from(args.jobs.clone())?;
    let llm_config = LlmConfig::try_from(args.llm.clone())?;
    let app_state = state(Arc::clone(&args)).await?;
    let outbound_config = OutboundConfig::try_from(args.outbound.clone())?;
    let metrics = Metrics::new();
    let usecase = app_state.usecase.clone().with_metrics(metrics.clone());
    let pool = usecase.pool().clone();
    let llm_runtime = LlmRuntime {
        cipher: AesGcmCipher::new(&llm_config.encryption_key)?,
        cache: PgLlmCache::new(pool.clone()),
        budget_cents: llm_config.run_budget_cents,
    };
    let queue = Arc::new(PgJobQueue::new(pool.clone()));
    let resolver = Arc::new(HickoryResolver::from_system(DnsSettings::default())?);
    let dns_cname = DnsCnameSource::new(
        Arc::clone(&resolver),
        Arc::new(PgCandidateHosts::new(pool.clone())),
        DnsCnameSettings {
            batch_size: DNS_BATCH_SIZE,
            concurrency: DNS_CONCURRENCY,
            revisit_interval: outbound_config.revisit_interval,
        },
    );
    let fetcher = Arc::new(PoliteFetcher::new(FetchSettings::from_outbound(
        &outbound_config,
    ))?);
    let mut boamp_fetch = FetchSettings::from_outbound(&outbound_config);
    boamp_fetch.max_body_bytes = BOAMP_MAX_BODY_BYTES;
    boamp_fetch.requests_per_host_per_minute = boamp_fetch
        .requests_per_host_per_minute
        .max(NonZeroU32::new(BOAMP_REQUESTS_PER_MINUTE).unwrap_or(NonZeroU32::MIN));
    let boamp = BoampSource::new(
        Arc::new(PoliteFetcher::new(boamp_fetch)?),
        BoampSettings {
            base_url: args.boamp.base_url.clone(),
            lookback_days: args.boamp.lookback_days.get(),
            max_pages: args.boamp.max_pages.get(),
        },
    );
    let mut ted_fetch = FetchSettings::from_outbound(&outbound_config);
    ted_fetch.max_body_bytes = TED_MAX_BODY_BYTES;
    ted_fetch.requests_per_host_per_minute = ted_fetch
        .requests_per_host_per_minute
        .max(NonZeroU32::new(TED_REQUESTS_PER_MINUTE).unwrap_or(NonZeroU32::MIN));
    let ted = TedSource::new(
        Arc::new(PoliteFetcher::new(ted_fetch)?),
        TedSettings {
            base_url: args.ted.base_url.clone(),
            lookback_days: args.ted.lookback_days.get(),
            max_pages: args.ted.max_pages.get(),
            buyer_countries: args.ted.buyer_countries.clone(),
        },
    );
    let france_travail = match args.france_travail.credentials() {
        Some((client_id, client_secret)) => {
            let mut france_travail_fetch = FetchSettings::from_outbound(&outbound_config);
            france_travail_fetch.max_body_bytes = FRANCE_TRAVAIL_MAX_BODY_BYTES;
            france_travail_fetch.requests_per_host_per_minute =
                NonZeroU32::new(FRANCE_TRAVAIL_REQUESTS_PER_MINUTE).unwrap_or(NonZeroU32::MIN);
            Some(FranceTravailSource::new(
                Arc::new(PoliteFetcher::new(france_travail_fetch)?),
                FranceTravailSettings {
                    token_url: args.france_travail.token_url.clone(),
                    api_base_url: args.france_travail.api_base_url.clone(),
                    client_id: client_id.clone(),
                    client_secret: client_secret.clone(),
                    published_since_days: args.france_travail.published_since_days.get(),
                    max_pages: args.france_travail.max_pages.get(),
                },
            ))
        }
        None => None,
    };
    let mut ats_fetch = FetchSettings::from_outbound(&outbound_config);
    ats_fetch.max_body_bytes = ATS_MAX_BODY_BYTES;
    let ats = AtsSource::new(
        Arc::new(PoliteFetcher::new(ats_fetch)?),
        PgAtsTargets::new(pool.clone()),
        AtsSettings {
            batch_size: args.ats.batch_size.get(),
            revisit_interval: args.ats.revisit_interval(),
            max_career_paths: args.ats.max_career_paths.get(),
            max_rate_limit_wait: ATS_MAX_RATE_LIMIT_WAIT,
        },
    );
    let mut decp_fetch = FetchSettings::from_outbound(&outbound_config);
    decp_fetch.max_body_bytes = args.decp.max_body_bytes();
    decp_fetch.request_timeout = decp_fetch.request_timeout.max(DECP_REQUEST_TIMEOUT);
    decp_fetch.requests_per_host_per_minute = decp_fetch
        .requests_per_host_per_minute
        .max(NonZeroU32::new(DECP_REQUESTS_PER_MINUTE).unwrap_or(NonZeroU32::MIN));
    let decp = DecpSource::new(
        Arc::new(PoliteFetcher::new(decp_fetch)?),
        DecpSettings {
            dataset_api_url: args.decp.dataset_api_url.clone(),
            annuaire_url: args.decp.annuaire_url.clone(),
            entreprises_url: args.decp.entreprises_url.clone(),
            lookback_days: args.decp.lookback_days.get(),
            notified_within_days: args.decp.notified_within_days.get(),
            max_body_bytes: args.decp.max_body_bytes(),
            max_rate_limit_wait: DECP_MAX_RATE_LIMIT_WAIT,
        },
    );
    let certificate_transparency = CtSource::new(
        Arc::clone(&fetcher),
        Arc::new(PgCandidateHosts::new(pool.clone())),
        Arc::new(PgCtCursors::new(pool.clone())),
        CtSettings {
            log_list_url: args.ct.log_list_url.clone(),
            log_list_ttl: CT_LOG_LIST_TTL,
            max_logs: args.ct.max_logs,
            tiles_per_log: args.ct.tiles_per_run.get(),
            max_lag_tiles: args.ct.max_lag_tiles.get(),
        },
    );
    let contacts = ContactFinders {
        public_page: PublicPageFinder::new(
            Arc::clone(&fetcher),
            ContactPageSettings {
                max_pages: args.contacts.page_paths.len(),
                paths: args.contacts.page_paths.clone(),
                max_rate_limit_wait: args.contacts.max_rate_limit_wait(),
            },
        ),
        security_txt: SecurityTxtFinder::new(
            Arc::clone(&fetcher),
            args.contacts.max_rate_limit_wait(),
        ),
        role_guess: RoleGuessFinder::new(resolver),
        targets: PgContactTargets::new(pool.clone()),
        settings: ContactRunSettings {
            batch_size: args.contacts.batch_size,
            revisit_interval: outbound_config.revisit_interval,
        },
    };
    let http_probe = HttpProbeSource::new(
        fetcher,
        Arc::new(PgCandidateHosts::new(pool)),
        Arc::new(usecase.clone()),
        HttpProbeSettings {
            batch_size: PROBE_BATCH_SIZE,
            concurrency: PROBE_CONCURRENCY,
            revisit_interval: outbound_config.revisit_interval,
            scheme: ProbeScheme::Https,
            port: None,
            host_override: None,
        },
    );
    let webhook = match args.webhook.settings()? {
        Some(settings) => {
            info!(
                host = settings.url.host_str().unwrap_or_default(),
                profiles = settings.profile_ids.len(),
                "webhook delivery enabled"
            );
            Some(WebhookSink::new(
                settings.url,
                settings.secret,
                settings.timeout,
            )?)
        }
        None => {
            info!("webhook delivery disabled, set WEBHOOK_URL and WEBHOOK_SECRET to enable it");
            None
        }
    };
    let job_router = JobRouter::new(
        usecase,
        WiredSources {
            certificate_transparency,
            dns_cname,
            http_probe,
            boamp,
            decp,
            ted,
            france_travail,
            ats,
            contacts,
        },
        Intervals {
            certificate_transparency: args.ct.interval(),
            other_sources: jobs_config.source_interval,
            decp: args.decp.interval(),
        },
        llm_runtime,
    )
    .with_webhook(webhook);
    schedule_wired_sources(&queue, args.france_travail.credentials().is_some()).await?;

    let internal_router = internal_router(metrics.clone(), app_state.usecase.pool().clone())?;
    let router = router(app_state)?;

    let addr = get_addr(&args.server.host, args.server.port).await?;
    let internal_addr = get_addr(&args.server.internal_host, args.server.internal_port).await?;

    let public_handle = Handle::new();
    let internal_handle = Handle::new();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let workers = WorkerPool::new(
        queue,
        Arc::new(job_router),
        Arc::new(SystemClock),
        WorkerSettings {
            workers: jobs_config.workers,
            lease: jobs_config.lease,
            poll_interval: jobs_config.poll_interval,
            retry: RetryPolicy {
                base_delay: jobs_config.retry_base_delay,
                max_delay: RetryPolicy::DEFAULT_MAX_DELAY,
            },
            shutdown_grace: jobs_config.shutdown_grace,
        },
    )
    .with_metrics(metrics);

    let shutdown = Shutdown {
        workers: shutdown_tx,
        public: public_handle.clone(),
        internal: internal_handle.clone(),
        grace: jobs_config.shutdown_grace,
    };
    let on_signal = shutdown.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        on_signal.trigger();
    });

    info!(%addr, %internal_addr, workers = jobs_config.workers.get(), "starting api, internal http servers and job workers");

    tokio::join!(
        serve(addr, router, public_handle, &shutdown),
        serve(internal_addr, internal_router, internal_handle, &shutdown),
        workers.run(shutdown_rx),
    );

    observability.shutdown();

    Ok(())
}

#[derive(Clone)]
struct Shutdown {
    workers: watch::Sender<bool>,
    public: Handle<SocketAddr>,
    internal: Handle<SocketAddr>,
    grace: Duration,
}

impl Shutdown {
    fn trigger(&self) {
        info!("shutting down: stopping new jobs and draining http connections");
        self.workers.send_replace(true);
        self.public.graceful_shutdown(Some(self.grace));
        self.internal.graceful_shutdown(Some(self.grace));
    }
}

async fn serve(addr: SocketAddr, router: Router, handle: Handle<SocketAddr>, shutdown: &Shutdown) {
    if let Err(server_error) = run_server(addr, router, handle).await {
        error!(%server_error, %addr, "http server stopped with an error");
        shutdown.trigger();
    }
}
