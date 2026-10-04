pub mod internal_router;
pub mod jobs;
pub mod logger;
pub mod openapi;
pub mod router;

use std::{error::Error, net::SocketAddr, sync::Arc, time::Duration};

use args::Args;
use axum::Router;
use axum_server::Handle;
use clap::Parser;
use common::{JobsConfig, OutboundConfig};
use dns::{DnsSettings, HickoryResolver};
use dotenvy::dotenv;
use fetch::{FetchSettings, PoliteFetcher};
use handlers::state;
use lodestone_core::{
    RetryPolicy,
    infrastructure::{candidate_host::postgres::PgCandidateHosts, job::postgres::PgJobQueue},
    jobs::{SystemClock, WorkerPool, WorkerSettings},
};
use server::{get_addr, run_server, shutdown_signal};
use sources::{
    dns_cname::{DnsCnameSettings, DnsCnameSource},
    http_probe::{HttpProbeSettings, HttpProbeSource, ProbeScheme},
};
use tokio::sync::watch;
use tracing::{error, info};

use crate::{
    internal_router::internal_router,
    jobs::{JobRouter, schedule_wired_sources},
    logger::init_tracing_and_logging,
    router::router,
};

const DNS_BATCH_SIZE: u32 = 500;
const DNS_CONCURRENCY: usize = 16;
const PROBE_BATCH_SIZE: u32 = 200;
const PROBE_CONCURRENCY: usize = 8;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let args = Arc::new(Args::parse());
    let observability = init_tracing_and_logging(&args.log, "lodestone", &args.observability)?;

    let jobs_config = JobsConfig::try_from(args.jobs.clone())?;
    let app_state = state(Arc::clone(&args)).await?;
    let outbound_config = OutboundConfig::try_from(args.outbound.clone())?;
    let pool = app_state.usecase.pool().clone();
    let queue = Arc::new(PgJobQueue::new(pool.clone()));
    let dns_cname = DnsCnameSource::new(
        Arc::new(HickoryResolver::from_system(DnsSettings::default())?),
        Arc::new(PgCandidateHosts::new(pool.clone())),
        DnsCnameSettings {
            batch_size: DNS_BATCH_SIZE,
            concurrency: DNS_CONCURRENCY,
            revisit_interval: outbound_config.revisit_interval,
        },
    );
    let http_probe = HttpProbeSource::new(
        Arc::new(PoliteFetcher::new(FetchSettings::from_outbound(
            &outbound_config,
        ))?),
        Arc::new(PgCandidateHosts::new(pool)),
        Arc::new(app_state.usecase.clone()),
        HttpProbeSettings {
            batch_size: PROBE_BATCH_SIZE,
            concurrency: PROBE_CONCURRENCY,
            revisit_interval: outbound_config.revisit_interval,
            scheme: ProbeScheme::Https,
            port: None,
            host_override: None,
        },
    );
    let job_router = JobRouter::new(
        app_state.usecase.clone(),
        dns_cname,
        http_probe,
        jobs_config.source_interval,
    );
    schedule_wired_sources(&queue).await?;

    let router = router(app_state)?;
    let internal_router = internal_router()?;

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
    );

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
