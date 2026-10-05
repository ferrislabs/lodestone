mod support;

use std::num::NonZeroU16;

use common::CoreError;
use lodestone_core::{
    JobKind, JobQueue, JobStatus, LodestoneUseCase, NewJob, Run, RunId, RunRepository, RunStatus,
    SourceName,
    infrastructure::{
        job::postgres::PgJobQueue, postgres::with_tx, run::postgres::PgRunRepository,
    },
    schedule::schedule_key,
};
use sqlx::PgPool;
use support::{at, create_profile};

fn job(kind: &str, key: &str, run_at: i64) -> NewJob {
    NewJob {
        kind: JobKind::parse(kind).unwrap(),
        payload: serde_json::json!({}),
        run_at: at(run_at),
        max_attempts: NonZeroU16::new(3).unwrap(),
        dedup_key: Some(key.to_owned()),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_now_moves_a_pending_job_to_now(pool: PgPool) {
    let queue = PgJobQueue::new(pool.clone());
    queue
        .enqueue(&job("source.run", "source:boamp", 4_000_000_000), at(0))
        .await
        .unwrap();
    let usecase = LodestoneUseCase::new(pool);

    let updated = usecase
        .run_job_now(&schedule_key(SourceName::Boamp))
        .await
        .unwrap();

    assert_eq!(updated.status, JobStatus::Pending);
    assert!(updated.run_at <= chrono::Utc::now());
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_now_never_postpones_an_overdue_job(pool: PgPool) {
    let queue = PgJobQueue::new(pool.clone());
    let mut overdue = job("source.run", "source:boamp", 0);
    overdue.run_at = chrono::Utc::now() - chrono::Duration::hours(1);
    queue.enqueue(&overdue, at(0)).await.unwrap();
    let usecase = LodestoneUseCase::new(pool);

    let updated = usecase.run_job_now("source:boamp").await.unwrap();

    assert_eq!(
        updated.run_at.timestamp_micros(),
        overdue.run_at.timestamp_micros()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_now_refuses_a_running_job(pool: PgPool) {
    let queue = PgJobQueue::new(pool.clone());
    queue
        .enqueue(&job("source.run", "source:boamp", 0), at(0))
        .await
        .unwrap();
    queue
        .claim(at(0), std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let usecase = LodestoneUseCase::new(pool);

    let error = usecase.run_job_now("source:boamp").await.unwrap_err();

    assert!(
        matches!(error, CoreError::Conflict(message) if message == "source is already running")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_now_without_a_job_is_not_found(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool);

    let error = usecase.run_job_now("source:boamp").await.unwrap_err();

    assert!(matches!(error, CoreError::NotFound));
}

#[sqlx::test(migrations = "../../migrations")]
async fn qualification_can_be_run_now(pool: PgPool) {
    let queue = PgJobQueue::new(pool.clone());
    queue
        .enqueue(&job("llm.qualify", "llm:qualify", 4_000_000_000), at(0))
        .await
        .unwrap();
    let usecase = LodestoneUseCase::new(pool);

    let updated = usecase
        .run_job_now(&schedule_key(SourceName::LlmQualification))
        .await
        .unwrap();

    assert!(updated.run_at <= chrono::Utc::now());
}

#[sqlx::test(migrations = "../../migrations")]
async fn other_job_kinds_are_not_listed_nor_runnable(pool: PgPool) {
    let queue = PgJobQueue::new(pool.clone());
    queue
        .enqueue(&job("webhook.deliver", "webhook:1", 4_000_000_000), at(0))
        .await
        .unwrap();
    let usecase = LodestoneUseCase::new(pool);

    assert!(
        usecase
            .list_source_schedule()
            .await
            .unwrap()
            .jobs
            .is_empty()
    );
    assert!(matches!(
        usecase.run_job_now("webhook:1").await.unwrap_err(),
        CoreError::NotFound
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn schedule_lists_jobs_and_the_latest_five_runs_per_source(pool: PgPool) {
    let queue = PgJobQueue::new(pool.clone());
    queue
        .enqueue(&job("source.run", "source:boamp", 50), at(0))
        .await
        .unwrap();
    let profile = create_profile(&pool, "alpha").await;
    with_tx(&pool, async |shared| {
        let mut repo = PgRunRepository::new(&shared);
        for second in 0..7 {
            repo.insert(&Run::start(
                RunId(common::generate_uuid_v7()),
                profile,
                SourceName::Boamp,
                None,
                at(second),
            ))
            .await?;
        }
        repo.insert(&Run::start(
            RunId(common::generate_uuid_v7()),
            profile,
            SourceName::Decp,
            None,
            at(1),
        ))
        .await
    })
    .await
    .unwrap();
    let usecase = LodestoneUseCase::new(pool);

    let schedule = usecase.list_source_schedule().await.unwrap();

    assert_eq!(schedule.jobs.len(), 1);
    assert_eq!(schedule.jobs[0].dedup_key, "source:boamp");
    assert_eq!(schedule.jobs[0].run_at, at(50));
    let boamp: Vec<_> = schedule
        .runs
        .iter()
        .filter(|run| run.source == SourceName::Boamp)
        .collect();
    assert_eq!(boamp.len(), 5);
    assert_eq!(boamp[0].started_at, at(6));
    assert_eq!(boamp[0].profile_name, "alpha");
    assert_eq!(boamp[0].status, RunStatus::Running);
    assert_eq!(schedule.runs.len(), 6);
}
