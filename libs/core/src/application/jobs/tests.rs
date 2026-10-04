use std::{
    collections::{HashMap, VecDeque},
    num::NonZeroU16,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, Utc};
use common::CoreError;
use tokio::{sync::watch, task::JoinHandle, time::Instant};

use super::*;
use crate::domain::job::{
    JobId, JobKind, JobStatus, LeaseToken, NewJob, RetryPolicy, new_lease_token,
};

type Entry = (Job, Option<(LeaseToken, DateTime<Utc>)>);

#[derive(Default)]
struct MemoryQueue {
    jobs: Mutex<HashMap<JobId, Entry>>,
    claims: Mutex<Vec<JobId>>,
}

impl MemoryQueue {
    fn snapshot(&self, id: JobId) -> Option<Job> {
        self.jobs
            .lock()
            .unwrap()
            .get(&id)
            .map(|(job, _)| job.clone())
    }

    fn claim_count(&self, id: JobId) -> usize {
        self.claims
            .lock()
            .unwrap()
            .iter()
            .filter(|claimed| **claimed == id)
            .count()
    }

    fn total_claims(&self) -> usize {
        self.claims.lock().unwrap().len()
    }
}

fn chrono_of(duration: Duration) -> chrono::Duration {
    chrono::Duration::from_std(duration).unwrap()
}

impl JobQueue for MemoryQueue {
    async fn enqueue(&self, job: &NewJob, now: DateTime<Utc>) -> Result<JobId, CoreError> {
        let id = JobId(uuid::Uuid::new_v4());
        let stored = Job {
            id,
            kind: job.kind.clone(),
            payload: job.payload.clone(),
            status: JobStatus::Pending,
            attempts: 0,
            max_attempts: job.max_attempts,
            run_at: job.run_at,
            last_error: None,
            created_at: now,
            updated_at: now,
        };
        self.jobs.lock().unwrap().insert(id, (stored, None));
        Ok(id)
    }

    async fn claim(
        &self,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<Option<ClaimedJob>, CoreError> {
        let mut jobs = self.jobs.lock().unwrap();
        let mut candidates: Vec<JobId> = jobs
            .iter()
            .filter(|(_, (job, held))| match job.status {
                JobStatus::Pending => job.run_at <= now,
                JobStatus::Running => held.is_some_and(|(_, until)| until <= now),
                _ => false,
            })
            .map(|(id, _)| *id)
            .collect();
        candidates.sort_by_key(|id| (jobs[id].0.created_at, id.0));
        let Some(id) = candidates.first().copied() else {
            return Ok(None);
        };
        let token = new_lease_token();
        let until = now + chrono_of(lease);
        let Some((job, held)) = jobs.get_mut(&id) else {
            return Ok(None);
        };
        job.status = JobStatus::Running;
        job.attempts += 1;
        job.updated_at = now;
        *held = Some((token, until));
        self.claims.lock().unwrap().push(id);
        Ok(Some(ClaimedJob {
            job: job.clone(),
            token,
            lease_until: until,
        }))
    }

    async fn extend_lease(
        &self,
        id: JobId,
        token: LeaseToken,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<bool, CoreError> {
        let mut jobs = self.jobs.lock().unwrap();
        match jobs.get_mut(&id) {
            Some((_, held)) if held.is_some_and(|(t, _)| t == token) => {
                *held = Some((token, now + chrono_of(lease)));
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn complete(
        &self,
        id: JobId,
        token: LeaseToken,
        now: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let mut jobs = self.jobs.lock().unwrap();
        match jobs.get_mut(&id) {
            Some((job, held)) if held.is_some_and(|(t, _)| t == token) => {
                job.status = JobStatus::Succeeded;
                job.updated_at = now;
                *held = None;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn fail(
        &self,
        id: JobId,
        token: LeaseToken,
        failure: &JobFailure,
        now: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let mut jobs = self.jobs.lock().unwrap();
        match jobs.get_mut(&id) {
            Some((job, held)) if held.is_some_and(|(t, _)| t == token) => {
                match failure {
                    JobFailure::RetryAt { at, error } => {
                        job.status = JobStatus::Pending;
                        job.run_at = *at;
                        job.last_error = Some(error.clone());
                    }
                    JobFailure::GiveUp { error } => {
                        job.status = JobStatus::Failed;
                        job.last_error = Some(error.clone());
                    }
                }
                job.updated_at = now;
                *held = None;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn release(
        &self,
        id: JobId,
        token: LeaseToken,
        now: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let mut jobs = self.jobs.lock().unwrap();
        match jobs.get_mut(&id) {
            Some((job, held)) if held.is_some_and(|(t, _)| t == token) => {
                job.status = JobStatus::Pending;
                job.attempts = job.attempts.saturating_sub(1);
                job.updated_at = now;
                *held = None;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn find(&self, id: JobId) -> Result<Option<Job>, CoreError> {
        Ok(self.snapshot(id))
    }
}

struct TestClock {
    base: DateTime<Utc>,
    start: Instant,
}

impl TestClock {
    fn new() -> Self {
        Self {
            base: Utc::now(),
            start: Instant::now(),
        }
    }
}

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        self.base + chrono_of(self.start.elapsed())
    }
}

struct ScriptedHandler {
    delay: Duration,
    results: Mutex<VecDeque<Result<(), JobError>>>,
    fallback: Result<(), JobError>,
    seen: Mutex<Vec<JobId>>,
}

impl ScriptedHandler {
    fn new(delay: Duration, results: Vec<Result<(), JobError>>) -> Self {
        Self::with_fallback(delay, results, Ok(()))
    }

    fn with_fallback(
        delay: Duration,
        results: Vec<Result<(), JobError>>,
        fallback: Result<(), JobError>,
    ) -> Self {
        Self {
            delay,
            results: Mutex::new(results.into()),
            fallback,
            seen: Mutex::new(Vec::new()),
        }
    }

    fn seen(&self) -> Vec<JobId> {
        self.seen.lock().unwrap().clone()
    }
}

impl JobHandler for ScriptedHandler {
    async fn handle(&self, job: &Job) -> Result<(), JobError> {
        self.seen.lock().unwrap().push(job.id);
        tokio::time::sleep(self.delay).await;
        self.results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| self.fallback.clone())
    }
}

fn settings(workers: u16) -> WorkerSettings {
    WorkerSettings {
        workers: NonZeroU16::new(workers).unwrap(),
        lease: Duration::from_secs(30),
        poll_interval: Duration::from_secs(1),
        retry: RetryPolicy {
            base_delay: Duration::from_secs(10),
            max_delay: Duration::from_secs(600),
        },
        shutdown_grace: Duration::from_secs(5),
    }
}

async fn enqueue(queue: &MemoryQueue, clock: &TestClock, max_attempts: u16) -> JobId {
    queue
        .enqueue(
            &NewJob {
                kind: JobKind::parse("probe.http").unwrap(),
                payload: serde_json::Value::Null,
                run_at: clock.now(),
                max_attempts: NonZeroU16::new(max_attempts).unwrap(),
            },
            clock.now(),
        )
        .await
        .unwrap()
}

struct Harness {
    queue: Arc<MemoryQueue>,
    handler: Arc<ScriptedHandler>,
    stop: watch::Sender<bool>,
    pool: JoinHandle<()>,
}

impl Harness {
    async fn start(
        handler: ScriptedHandler,
        settings: WorkerSettings,
        jobs: &[u16],
    ) -> (Self, Vec<JobId>) {
        let queue = Arc::new(MemoryQueue::default());
        let clock = Arc::new(TestClock::new());
        let handler = Arc::new(handler);
        let mut ids = Vec::new();
        for max in jobs {
            ids.push(enqueue(&queue, &clock, *max).await);
        }
        let (stop, rx) = watch::channel(false);
        let pool = WorkerPool::new(
            Arc::clone(&queue),
            Arc::clone(&handler),
            Arc::clone(&clock),
            settings,
        );
        let pool = tokio::spawn(pool.run(rx));
        (
            Self {
                queue,
                handler,
                stop,
                pool,
            },
            ids,
        )
    }

    async fn wait_for(&self, id: JobId, status: JobStatus) -> Job {
        for _ in 0..10_000 {
            if let Some(job) = self.queue.snapshot(id)
                && job.status == status
            {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("job never reached {status:?}");
    }

    async fn shutdown(self) {
        self.stop.send(true).unwrap();
        self.pool.await.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn succeeding_job_is_completed_once() {
    let handler = ScriptedHandler::new(Duration::from_secs(1), vec![]);
    let (h, ids) = Harness::start(handler, settings(1), &[3]).await;
    let job = h.wait_for(ids[0], JobStatus::Succeeded).await;
    assert_eq!(job.attempts, 1);
    tokio::time::sleep(Duration::from_secs(60)).await;
    assert_eq!(h.queue.claim_count(ids[0]), 1);
    assert_eq!(h.handler.seen(), vec![ids[0]]);
    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn retryable_failure_is_retried_after_backoff() {
    let handler = ScriptedHandler::new(
        Duration::from_millis(10),
        vec![Err(JobError::Retryable("boom".into()))],
    );
    let (h, ids) = Harness::start(handler, settings(1), &[3]).await;
    let started = Instant::now();
    let job = h.wait_for(ids[0], JobStatus::Succeeded).await;
    assert_eq!(job.attempts, 2);
    assert!(started.elapsed() >= Duration::from_secs(10));
    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn permanent_failure_gives_up_immediately() {
    let handler = ScriptedHandler::new(
        Duration::from_millis(10),
        vec![Err(JobError::Permanent("bad".into()))],
    );
    let (h, ids) = Harness::start(handler, settings(1), &[5]).await;
    let job = h.wait_for(ids[0], JobStatus::Failed).await;
    assert_eq!(job.attempts, 1);
    assert_eq!(job.last_error.as_deref(), Some("bad"));
    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn max_attempts_reached_fails_with_last_error() {
    let handler = ScriptedHandler::with_fallback(
        Duration::from_millis(10),
        vec![],
        Err(JobError::Retryable("boom".into())),
    );
    let (h, ids) = Harness::start(handler, settings(1), &[2]).await;
    let job = h.wait_for(ids[0], JobStatus::Failed).await;
    assert_eq!(job.attempts, 2);
    assert_eq!(job.last_error.as_deref(), Some("boom"));
    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn heartbeat_keeps_lease_during_long_handler() {
    let handler = ScriptedHandler::new(Duration::from_secs(200), vec![]);
    let (h, ids) = Harness::start(handler, settings(2), &[3]).await;
    let job = h.wait_for(ids[0], JobStatus::Succeeded).await;
    assert_eq!(job.attempts, 1);
    assert_eq!(h.queue.claim_count(ids[0]), 1);
    assert_eq!(h.handler.seen().len(), 1);
    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn shutdown_lets_in_flight_job_finish_within_grace() {
    let handler = ScriptedHandler::new(Duration::from_secs(3), vec![]);
    let (h, ids) = Harness::start(handler, settings(1), &[3]).await;
    h.wait_for(ids[0], JobStatus::Running).await;
    h.stop.send(true).unwrap();
    h.pool.await.unwrap();
    let job = h.queue.snapshot(ids[0]).unwrap();
    assert_eq!(job.status, JobStatus::Succeeded);
}

#[tokio::test(start_paused = true)]
async fn shutdown_releases_job_exceeding_grace() {
    let handler = ScriptedHandler::new(Duration::from_secs(1000), vec![]);
    let (h, ids) = Harness::start(handler, settings(1), &[3]).await;
    h.wait_for(ids[0], JobStatus::Running).await;
    h.stop.send(true).unwrap();
    h.pool.await.unwrap();
    let job = h.queue.snapshot(ids[0]).unwrap();
    assert_eq!(job.status, JobStatus::Pending);
    assert_eq!(job.attempts, 0);
}

#[tokio::test(start_paused = true)]
async fn no_job_is_claimed_after_shutdown() {
    let queue = Arc::new(MemoryQueue::default());
    let clock = Arc::new(TestClock::new());
    let handler = Arc::new(ScriptedHandler::new(Duration::from_secs(1), vec![]));
    let id = enqueue(&queue, &clock, 3).await;
    let (stop, rx) = watch::channel(true);
    WorkerPool::new(
        Arc::clone(&queue),
        Arc::clone(&handler),
        Arc::clone(&clock),
        settings(3),
    )
    .run(rx)
    .await;
    drop(stop);
    assert_eq!(queue.total_claims(), 0);
    assert_eq!(queue.snapshot(id).unwrap().status, JobStatus::Pending);
}

#[tokio::test(start_paused = true)]
async fn workers_handle_each_job_exactly_once() {
    let handler = ScriptedHandler::new(Duration::from_secs(2), vec![]);
    let (h, ids) = Harness::start(handler, settings(4), &[3; 20]).await;
    for id in &ids {
        h.wait_for(*id, JobStatus::Succeeded).await;
    }
    let mut seen = h.handler.seen();
    seen.sort_by_key(|id| id.0);
    let mut expected = ids.clone();
    expected.sort_by_key(|id| id.0);
    assert_eq!(seen, expected);
    assert_eq!(h.queue.total_claims(), 20);
    h.shutdown().await;
}
