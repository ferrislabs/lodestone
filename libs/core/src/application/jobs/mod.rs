use std::{num::NonZeroU16, pin::pin, sync::Arc, time::Duration};

use thiserror::Error;
use tokio::{
    sync::watch,
    task::JoinSet,
    time::{Instant, interval_at, sleep, sleep_until},
};
use tracing::{Instrument, info_span, warn};

use crate::domain::{
    agent_work::CancelReason,
    job::{
        ClaimedJob, FailureDecision, Job, RetryPolicy,
        ports::{Clock, JobFailure, JobQueue},
    },
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum JobError {
    #[error("retryable job failure: {0}")]
    Retryable(String),
    #[error("permanent job failure: {0}")]
    Permanent(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobOutcome {
    Done,
    RescheduleAt(chrono::DateTime<chrono::Utc>),
    Cancelled(CancelReason),
}

pub trait JobHandler: Send + Sync {
    fn handle(&self, job: &Job) -> impl Future<Output = Result<JobOutcome, JobError>> + Send;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WorkerSettings {
    pub workers: NonZeroU16,
    pub lease: Duration,
    pub poll_interval: Duration,
    pub retry: RetryPolicy,
    pub shutdown_grace: Duration,
}

pub struct WorkerPool<Q, H, C>
where
    Q: JobQueue + 'static,
    H: JobHandler + 'static,
    C: Clock + 'static,
{
    queue: Arc<Q>,
    handler: Arc<H>,
    clock: Arc<C>,
    settings: WorkerSettings,
}

impl<Q, H, C> WorkerPool<Q, H, C>
where
    Q: JobQueue + 'static,
    H: JobHandler + 'static,
    C: Clock + 'static,
{
    pub fn new(queue: Arc<Q>, handler: Arc<H>, clock: Arc<C>, settings: WorkerSettings) -> Self {
        Self {
            queue,
            handler,
            clock,
            settings,
        }
    }

    pub async fn run(self, shutdown: watch::Receiver<bool>) {
        let mut tasks = JoinSet::new();
        for index in 0..self.settings.workers.get() {
            let worker = Worker {
                queue: Arc::clone(&self.queue),
                handler: Arc::clone(&self.handler),
                clock: Arc::clone(&self.clock),
                settings: self.settings,
            };
            let rx = shutdown.clone();
            tasks.spawn(
                async move { worker.run(rx).await }.instrument(info_span!("job_worker", index)),
            );
        }
        while let Some(joined) = tasks.join_next().await {
            if let Err(error) = joined {
                warn!(%error, "job worker terminated abnormally");
            }
        }
    }
}

enum Outcome {
    Finished(Result<JobOutcome, JobError>),
    LeaseLost,
    Abandoned,
}

struct Worker<Q, H, C> {
    queue: Arc<Q>,
    handler: Arc<H>,
    clock: Arc<C>,
    settings: WorkerSettings,
}

async fn shutdown_requested(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow_and_update() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

impl<Q, H, C> Worker<Q, H, C>
where
    Q: JobQueue,
    H: JobHandler,
    C: Clock,
{
    async fn run(&self, mut shutdown: watch::Receiver<bool>) {
        loop {
            if *shutdown.borrow_and_update() {
                return;
            }
            match self
                .queue
                .claim(self.clock.now(), self.settings.lease)
                .await
            {
                Ok(Some(claimed)) => {
                    let span = info_span!(
                        "job",
                        job.id = %claimed.job.id,
                        job.kind = claimed.job.kind.as_str()
                    );
                    self.process(&claimed, &mut shutdown).instrument(span).await;
                }
                Ok(None) => self.idle(&mut shutdown).await,
                Err(error) => {
                    warn!(%error, "job claim failed");
                    self.idle(&mut shutdown).await;
                }
            }
        }
    }

    async fn idle(&self, shutdown: &mut watch::Receiver<bool>) {
        tokio::select! {
            () = sleep(self.settings.poll_interval) => {}
            () = shutdown_requested(shutdown) => {}
        }
    }

    async fn process(&self, claimed: &ClaimedJob, shutdown: &mut watch::Receiver<bool>) {
        let job = &claimed.job;
        match self.drive(claimed, shutdown).await {
            Outcome::Finished(Ok(JobOutcome::Done)) => {
                let result = self
                    .queue
                    .complete(job.id, claimed.token, self.clock.now())
                    .await;
                report("complete", result);
            }
            Outcome::Finished(Ok(JobOutcome::RescheduleAt(at))) => {
                let result = self
                    .queue
                    .reschedule(job.id, claimed.token, at, self.clock.now())
                    .await;
                report("reschedule", result);
            }
            Outcome::Finished(Ok(JobOutcome::Cancelled(reason))) => {
                let result = self
                    .queue
                    .cancel(job.id, claimed.token, reason, self.clock.now())
                    .await;
                report("cancel", result);
            }
            Outcome::Finished(Err(error)) => {
                let failure = self.failure(job, error);
                let result = self
                    .queue
                    .fail(job.id, claimed.token, &failure, self.clock.now())
                    .await;
                report("fail", result);
            }
            Outcome::LeaseLost => {
                warn!("job lease lost, abandoning handler");
            }
            Outcome::Abandoned => {
                warn!("shutdown grace elapsed, releasing job");
                let result = self
                    .queue
                    .release(job.id, claimed.token, self.clock.now())
                    .await;
                report("release", result);
            }
        }
    }

    fn failure(&self, job: &Job, error: JobError) -> JobFailure {
        match error {
            JobError::Retryable(error) => match self.settings.retry.decide(job, self.clock.now()) {
                FailureDecision::RetryAt(at) => JobFailure::RetryAt { at, error },
                FailureDecision::GiveUp => JobFailure::GiveUp { error },
            },
            JobError::Permanent(error) => JobFailure::GiveUp { error },
        }
    }

    async fn drive(&self, claimed: &ClaimedJob, shutdown: &mut watch::Receiver<bool>) -> Outcome {
        let lease = self.settings.lease;
        let period = (lease / 3).max(Duration::from_millis(1));
        let mut heartbeat = interval_at(Instant::now() + period, period);
        let mut handler = pin!(self.handler.handle(&claimed.job));
        let mut deadline: Option<Instant> = None;
        loop {
            tokio::select! {
                result = &mut handler => return Outcome::Finished(result),
                _ = heartbeat.tick() => {
                    let extended = self
                        .queue
                        .extend_lease(claimed.job.id, claimed.token, self.clock.now(), lease)
                        .await;
                    match extended {
                        Ok(true) => {}
                        Ok(false) => return Outcome::LeaseLost,
                        Err(error) => warn!(%error, "job heartbeat failed"),
                    }
                }
                () = shutdown_requested(shutdown), if deadline.is_none() => {
                    deadline = Some(Instant::now() + self.settings.shutdown_grace);
                }
                () = sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => {
                    return Outcome::Abandoned;
                }
            }
        }
    }
}

fn report(action: &str, result: Result<bool, common::CoreError>) {
    match result {
        Ok(true) => {}
        Ok(false) => warn!(action, "job lease no longer held"),
        Err(error) => warn!(action, %error, "job queue update failed"),
    }
}
