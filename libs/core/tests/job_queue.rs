mod support;

use std::{collections::HashSet, num::NonZeroU16, time::Duration};

use lodestone_core::{
    JobFailure, JobId, JobKind, JobQueue, JobStatus, NewJob,
    infrastructure::job::postgres::PgJobQueue,
};
use sqlx::PgPool;
use support::at;

const LEASE: Duration = Duration::from_secs(30);

fn new_job(run_at: i64, max_attempts: u16) -> NewJob {
    NewJob {
        kind: JobKind::parse("probe.http").unwrap(),
        payload: serde_json::json!({"host": "example.com"}),
        run_at: at(run_at),
        max_attempts: NonZeroU16::new(max_attempts).unwrap(),
        dedup_key: None,
    }
}

async fn enqueue(queue: &PgJobQueue, run_at: i64, max_attempts: u16) -> JobId {
    queue
        .enqueue(&new_job(run_at, max_attempts), at(0))
        .await
        .unwrap()
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn enqueue_then_claim_runs_once(pool: PgPool) {
    let queue = PgJobQueue::new(pool);
    let id = enqueue(&queue, 0, 3).await;

    let claimed = queue.claim(at(0), LEASE).await.unwrap().unwrap();
    assert_eq!(claimed.job.id, id);
    assert_eq!(claimed.job.status, JobStatus::Running);
    assert_eq!(claimed.job.attempts, 1);
    assert_eq!(claimed.lease_until, at(30));
    assert!(queue.claim(at(0), LEASE).await.unwrap().is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn future_job_is_claimable_only_at_run_at(pool: PgPool) {
    let queue = PgJobQueue::new(pool);
    let id = enqueue(&queue, 60, 3).await;

    assert!(queue.claim(at(59), LEASE).await.unwrap().is_none());
    let claimed = queue.claim(at(60), LEASE).await.unwrap().unwrap();
    assert_eq!(claimed.job.id, id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_workers_claim_each_job_exactly_once(pool: PgPool) {
    let queue = PgJobQueue::new(pool);
    for _ in 0..50 {
        enqueue(&queue, 0, 3).await;
    }

    let mut handles = Vec::new();
    for _ in 0..8 {
        let queue = queue.clone();
        handles.push(tokio::spawn(async move {
            let mut ids = Vec::new();
            while let Some(claimed) = queue.claim(at(0), Duration::from_secs(300)).await.unwrap() {
                ids.push(claimed.job.id);
            }
            ids
        }));
    }
    let mut all = Vec::new();
    for handle in handles {
        all.extend(handle.await.unwrap());
    }

    let distinct: HashSet<JobId> = all.iter().copied().collect();
    assert_eq!(all.len(), 50);
    assert_eq!(distinct.len(), 50);
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_lease_is_reclaimed_after_crash(pool: PgPool) {
    let queue = PgJobQueue::new(pool);
    let id = enqueue(&queue, 0, 3).await;

    let first = queue.claim(at(0), LEASE).await.unwrap().unwrap();
    assert!(queue.claim(at(10), LEASE).await.unwrap().is_none());
    let second = queue.claim(at(31), LEASE).await.unwrap().unwrap();

    assert_eq!(second.job.id, id);
    assert_eq!(second.job.attempts, 2);
    assert_ne!(second.token, first.token);
}

#[sqlx::test(migrations = "../../migrations")]
async fn stale_token_cannot_touch_the_job(pool: PgPool) {
    let queue = PgJobQueue::new(pool);
    let id = enqueue(&queue, 0, 3).await;
    let old = queue.claim(at(0), LEASE).await.unwrap().unwrap();
    let new = queue.claim(at(31), LEASE).await.unwrap().unwrap();
    let before = queue.find(id).await.unwrap().unwrap();

    let failure = JobFailure::GiveUp {
        error: "boom".to_owned(),
    };
    assert!(!queue.complete(id, old.token, at(32)).await.unwrap());
    assert!(
        !queue
            .extend_lease(id, old.token, at(32), LEASE)
            .await
            .unwrap()
    );
    assert!(!queue.fail(id, old.token, &failure, at(32)).await.unwrap());
    assert!(!queue.release(id, old.token, at(32)).await.unwrap());
    assert_eq!(queue.find(id).await.unwrap().unwrap(), before);

    assert!(queue.complete(id, new.token, at(33)).await.unwrap());
    let done = queue.find(id).await.unwrap().unwrap();
    assert_eq!(done.status, JobStatus::Succeeded);
    assert!(done.last_error.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn fail_retries_or_gives_up(pool: PgPool) {
    let queue = PgJobQueue::new(pool);
    let id = enqueue(&queue, 0, 3).await;

    let claimed = queue.claim(at(0), LEASE).await.unwrap().unwrap();
    let retry = JobFailure::RetryAt {
        at: at(100),
        error: "timeout".to_owned(),
    };
    assert!(queue.fail(id, claimed.token, &retry, at(1)).await.unwrap());
    let job = queue.find(id).await.unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Pending);
    assert_eq!(job.run_at, at(100));
    assert_eq!(job.last_error.as_deref(), Some("timeout"));
    assert!(queue.claim(at(99), LEASE).await.unwrap().is_none());

    let claimed = queue.claim(at(100), LEASE).await.unwrap().unwrap();
    let give_up = JobFailure::GiveUp {
        error: "fatal".to_owned(),
    };
    assert!(
        queue
            .fail(id, claimed.token, &give_up, at(101))
            .await
            .unwrap()
    );
    let job = queue.find(id).await.unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.last_error.as_deref(), Some("fatal"));
    assert!(queue.claim(at(1000), LEASE).await.unwrap().is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_gives_the_attempt_back(pool: PgPool) {
    let queue = PgJobQueue::new(pool);
    let id = enqueue(&queue, 0, 3).await;

    let claimed = queue.claim(at(0), LEASE).await.unwrap().unwrap();
    assert!(queue.release(id, claimed.token, at(5)).await.unwrap());
    let job = queue.find(id).await.unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Pending);
    assert_eq!(job.attempts, 0);
    assert_eq!(job.run_at, at(5));

    let again = queue.claim(at(5), LEASE).await.unwrap().unwrap();
    assert_eq!(again.job.id, id);
    assert_eq!(again.job.attempts, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_lease_on_last_attempt_fails_the_job(pool: PgPool) {
    let queue = PgJobQueue::new(pool);
    let id = enqueue(&queue, 0, 1).await;

    queue.claim(at(0), LEASE).await.unwrap().unwrap();
    assert!(queue.claim(at(31), LEASE).await.unwrap().is_none());
    let job = queue.find(id).await.unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.last_error.as_deref(), Some("lease expired"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn extended_lease_blocks_reclaim(pool: PgPool) {
    let queue = PgJobQueue::new(pool);
    let id = enqueue(&queue, 0, 3).await;

    let claimed = queue.claim(at(0), LEASE).await.unwrap().unwrap();
    assert!(
        queue
            .extend_lease(id, claimed.token, at(20), LEASE)
            .await
            .unwrap()
    );
    assert!(queue.claim(at(31), LEASE).await.unwrap().is_none());
    assert!(queue.claim(at(51), LEASE).await.unwrap().is_some());
}

fn keyed_job(key: &str) -> NewJob {
    NewJob {
        dedup_key: Some(key.to_owned()),
        ..new_job(0, 3)
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn dedup_key_allows_one_active_job(pool: PgPool) {
    let queue = PgJobQueue::new(pool);
    let first = queue
        .enqueue(&keyed_job("source:http_probe"), at(0))
        .await
        .unwrap();
    assert!(first.is_some());
    assert_eq!(
        queue
            .enqueue(&keyed_job("source:http_probe"), at(0))
            .await
            .unwrap(),
        None
    );
    assert!(
        queue
            .enqueue(
                &NewJob {
                    run_at: at(10),
                    ..keyed_job("source:dns_cname")
                },
                at(0),
            )
            .await
            .unwrap()
            .is_some()
    );

    let claimed = queue.claim(at(1), LEASE).await.unwrap().unwrap();
    assert_eq!(
        queue
            .enqueue(&keyed_job("source:http_probe"), at(1))
            .await
            .unwrap(),
        None
    );

    queue
        .complete(claimed.job.id, claimed.token, at(2))
        .await
        .unwrap();
    let after_completion = queue
        .enqueue(&keyed_job("source:http_probe"), at(3))
        .await
        .unwrap();
    assert_eq!(claimed.job.id, first.unwrap());
    assert!(after_completion.is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn jobs_without_dedup_key_never_conflict(pool: PgPool) {
    let queue = PgJobQueue::new(pool);
    assert!(
        queue
            .enqueue(&new_job(0, 3), at(0))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        queue
            .enqueue(&new_job(0, 3), at(0))
            .await
            .unwrap()
            .is_some()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn reschedule_turns_a_job_into_a_recurring_one(pool: PgPool) {
    let queue = PgJobQueue::new(pool);
    let id = queue
        .enqueue(&keyed_job("source:dns_cname"), at(0))
        .await
        .unwrap()
        .unwrap();
    let claimed = queue.claim(at(1), LEASE).await.unwrap().unwrap();

    assert!(
        queue
            .reschedule(id, claimed.token, at(3600), at(2))
            .await
            .unwrap()
    );
    assert!(
        !queue
            .reschedule(id, claimed.token, at(3600), at(2))
            .await
            .unwrap()
    );

    let job = queue.find(id).await.unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Pending);
    assert_eq!(job.attempts, 0);
    assert_eq!(job.run_at, at(3600));
    assert!(queue.claim(at(3599), LEASE).await.unwrap().is_none());
    assert_eq!(
        queue
            .enqueue(&keyed_job("source:dns_cname"), at(5))
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        queue.claim(at(3600), LEASE).await.unwrap().unwrap().job.id,
        id
    );
}
