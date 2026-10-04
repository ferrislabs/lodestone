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
use common::JobsConfig;
use dotenvy::dotenv;
use handlers::state;
use lodestone_core::{
    RetryPolicy,
    infrastructure::job::postgres::PgJobQueue,
    jobs::{SystemClock, WorkerPool, WorkerSettings},
};
use server::{get_addr, run_server, shutdown_signal};
use tokio::sync::watch;
use tracing::{error, info};

use crate::{
    internal_router::internal_router, jobs::JobRouter, logger::init_tracing_and_logging,
    router::router,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let args = Arc::new(Args::parse());
    let observability = init_tracing_and_logging(&args.log, "lodestone", &args.observability)?;

    let jobs_config = JobsConfig::try_from(args.jobs.clone())?;
    let app_state = state(Arc::clone(&args)).await?;
    let queue = PgJobQueue::new(app_state.usecase.pool().clone());

    let router = router(app_state)?;
    let internal_router = internal_router()?;

    let addr = get_addr(&args.server.host, args.server.port).await?;
    let internal_addr = get_addr(&args.server.internal_host, args.server.internal_port).await?;

    let public_handle = Handle::new();
    let internal_handle = Handle::new();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let workers = WorkerPool::new(
        Arc::new(queue),
        Arc::new(JobRouter),
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
