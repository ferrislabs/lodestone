use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use chrono::Utc;
use common::CoreError;
use sqlx::PgPool;
use tokio::{
    sync::watch,
    time::{Instant, interval_at, sleep},
};
use tracing::{info, warn};

use crate::{
    application::{LodestoneUseCase, sources::SourceRunError},
    domain::{
        listener::{ListenerState, ListenerStatus, StatusUpdate, StepOutcome, lock_key},
        values::SourceName,
    },
    infrastructure::listener::postgres::{PgListenerLock, PgListenerStatuses},
};

pub trait ListenerStep: Send + Sync {
    fn name(&self) -> &str;

    fn is_active(&self) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn blocking_provider(
        &self,
    ) -> impl Future<Output = Result<Option<&'static str>, CoreError>> + Send {
        async { Ok(None) }
    }

    fn run_once(&self) -> impl Future<Output = Result<StepOutcome, SourceRunError>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListenerSettings {
    pub idle: Duration,
    pub lock_retry: Duration,
    pub yield_pause: Duration,
    pub backoff_base: Duration,
    pub backoff_max: Duration,
    pub shutdown_grace: Duration,
}

impl ListenerSettings {
    pub const DEFAULT_IDLE: Duration = Duration::from_secs(15);
    pub const DEFAULT_LOCK_RETRY: Duration = Duration::from_secs(15);
    pub const DEFAULT_YIELD: Duration = Duration::from_millis(250);
    pub const DEFAULT_BACKOFF_BASE: Duration = Duration::from_secs(5);
    pub const DEFAULT_BACKOFF_MAX: Duration = Duration::from_secs(300);

    pub fn backoff(&self, failures: u32) -> Duration {
        let shift = failures.saturating_sub(1).min(16);
        self.backoff_base
            .saturating_mul(1u32 << shift)
            .min(self.backoff_max)
    }
}

pub struct Listener<S: ListenerStep> {
    step: S,
    pool: PgPool,
    instance: String,
    settings: ListenerSettings,
    blocked: AtomicBool,
}

async fn pause(shutdown: &mut watch::Receiver<bool>, duration: Duration) -> bool {
    tokio::select! {
        () = sleep(duration) => true,
        _ = shutdown.wait_for(|stopped| *stopped) => false,
    }
}

impl<S: ListenerStep> Listener<S> {
    pub fn new(step: S, pool: PgPool, instance: String, settings: ListenerSettings) -> Self {
        Self {
            step,
            pool,
            instance,
            settings,
            blocked: AtomicBool::new(false),
        }
    }

    pub async fn run(&self, mut shutdown: watch::Receiver<bool>) {
        let name = self.step.name();
        let statuses = PgListenerStatuses::new(self.pool.clone());
        let key = lock_key(name);
        let mut lock: Option<PgListenerLock> = None;
        let mut failures = 0u32;
        let mut held_once = false;
        while !*shutdown.borrow() {
            match self.hold(&mut lock, key).await {
                Ok(true) => {
                    if !held_once {
                        info!(listener = name, holder = %self.instance, "listener holds its lock");
                        held_once = true;
                    }
                }
                Ok(false) => {
                    held_once = false;
                    if let Err(error) = statuses.record_standby(name, Utc::now()).await {
                        warn!(listener = name, %error, "listener standby status not recorded");
                    }
                    if !pause(&mut shutdown, self.settings.lock_retry).await {
                        break;
                    }
                    continue;
                }
                Err(error) => {
                    held_once = false;
                    lock = None;
                    warn!(listener = name, %error, "listener lock unavailable");
                    if !pause(&mut shutdown, self.settings.lock_retry).await {
                        break;
                    }
                    continue;
                }
            }
            let Some(wait) = self.iterate(&statuses, &mut failures, &mut shutdown).await else {
                break;
            };
            if !pause(&mut shutdown, wait).await {
                break;
            }
        }
        if let Some(lock) = lock {
            if lock.is_held()
                && let Err(error) = statuses
                    .record_stopped(name, &self.instance, Utc::now())
                    .await
            {
                warn!(listener = name, %error, "listener stop status not recorded");
            }
            lock.release().await;
        }
        info!(listener = name, "listener stopped");
    }

    async fn hold(&self, lock: &mut Option<PgListenerLock>, key: i64) -> Result<bool, CoreError> {
        let connection = match lock {
            Some(connection) => connection,
            None => lock.insert(PgListenerLock::connect(&self.pool, key).await?),
        };
        connection.try_acquire().await
    }

    async fn iterate(
        &self,
        statuses: &PgListenerStatuses,
        failures: &mut u32,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Option<Duration> {
        let name = self.step.name();
        let active = match self.step.is_active().await {
            Ok(active) => active,
            Err(error) => {
                return Some(self.fail(statuses, failures, &error.to_string()).await);
            }
        };
        if !active {
            *failures = 0;
            self.record(
                statuses,
                ListenerState::IdleNoAgent,
                StepOutcome::default(),
                None,
            )
            .await;
            return Some(self.settings.idle);
        }
        match self.step.blocking_provider().await {
            Ok(None) => {
                self.blocked.store(false, Ordering::Relaxed);
            }
            Ok(Some(provider)) => {
                if !self.blocked.swap(true, Ordering::Relaxed) {
                    info!(
                        listener = name,
                        provider, "provider not ready, the listener waits"
                    );
                }
                *failures = 0;
                self.record(statuses, ListenerState::Idle, StepOutcome::default(), None)
                    .await;
                return Some(self.settings.idle);
            }
            Err(error) => {
                return Some(self.fail(statuses, failures, &error.to_string()).await);
            }
        }
        if let Err(error) = statuses
            .record_started(name, &self.instance, Utc::now())
            .await
        {
            warn!(listener = name, %error, "listener start status not recorded");
        }
        let step = self.step.run_once();
        tokio::pin!(step);
        let abandon = async {
            let _ = shutdown.wait_for(|stopped| *stopped).await;
            sleep(self.settings.shutdown_grace).await;
        };
        tokio::pin!(abandon);
        let mut beat = interval_at(Instant::now() + self.settings.idle, self.settings.idle);
        let result = loop {
            tokio::select! {
                result = &mut step => break result,
                _ = beat.tick() => {
                    if let Err(error) = statuses.touch(name, &self.instance, Utc::now()).await {
                        warn!(listener = name, %error, "listener heartbeat not recorded");
                    }
                }
                () = &mut abandon => {
                    warn!(listener = name, "listener step abandoned after the shutdown grace");
                    return None;
                }
            }
        };
        match result {
            Ok(outcome) => {
                *failures = 0;
                let state = if outcome.processed > 0 {
                    ListenerState::Listening
                } else {
                    ListenerState::Idle
                };
                self.record(statuses, state, outcome, None).await;
                Some(if outcome.processed > 0 {
                    self.settings.yield_pause
                } else {
                    self.settings.idle
                })
            }
            Err(error) => Some(self.fail(statuses, failures, &error.to_string()).await),
        }
    }

    async fn fail(
        &self,
        statuses: &PgListenerStatuses,
        failures: &mut u32,
        message: &str,
    ) -> Duration {
        *failures = failures.saturating_add(1);
        let wait = self.settings.backoff(*failures);
        warn!(listener = self.step.name(), error = message, retry_in = ?wait, "listener step failed");
        self.record(
            statuses,
            ListenerState::Error,
            StepOutcome::default(),
            Some(message),
        )
        .await;
        wait
    }

    async fn record(
        &self,
        statuses: &PgListenerStatuses,
        state: ListenerState,
        outcome: StepOutcome,
        error: Option<&str>,
    ) {
        let update = StatusUpdate {
            name: self.step.name(),
            holder: &self.instance,
            state,
            outcome,
            error,
            at: Utc::now(),
        };
        if let Err(error) = statuses.record(&update).await {
            warn!(listener = self.step.name(), %error, "listener status not recorded");
        }
    }
}

impl LodestoneUseCase {
    pub async fn has_enabled_agent(&self, source: SourceName) -> Result<bool, CoreError> {
        Ok(self.list_active_profiles().await?.iter().any(|active| {
            active
                .revision
                .definition
                .definition()
                .enabled_agents(source)
                .next()
                .is_some()
        }))
    }

    pub async fn list_listener_statuses(&self) -> Result<Vec<ListenerStatus>, CoreError> {
        let now = Utc::now();
        Ok(PgListenerStatuses::new(self.pool.clone())
            .list()
            .await?
            .into_iter()
            .map(|status| status.observed(now, self.listener_stale_after))
            .collect())
    }
}
