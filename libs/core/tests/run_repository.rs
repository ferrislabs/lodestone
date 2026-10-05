mod support;

use lodestone_core::{
    Run, RunId, RunOutcome, RunRepository, RunStatus, SourceName,
    infrastructure::{postgres::with_tx, run::postgres::PgRunRepository},
};
use sqlx::PgPool;
use support::{at, create_profile};

fn started(profile: lodestone_core::ProfileId, seconds: i64) -> Run {
    Run::start(
        RunId(common::generate_uuid_v7()),
        profile,
        SourceName::HttpProbe,
        None,
        at(seconds),
    )
}

#[sqlx::test(migrations = "../../migrations")]
async fn insert_running_run(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let run = started(profile, 0);

    let found = with_tx(&pool, async |shared| {
        let mut repo = PgRunRepository::new(&shared);
        repo.insert(&run).await?;
        repo.find(run.id).await
    })
    .await
    .unwrap();

    let found = found.unwrap();
    assert_eq!(found, run);
    assert_eq!(found.status, RunStatus::Running);
    assert!(found.finished_at.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn finish_succeeded_then_second_finish_is_none(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let run = started(profile, 0);

    let (first, second) = with_tx(&pool, async |shared| {
        let mut repo = PgRunRepository::new(&shared);
        repo.insert(&run).await?;
        let first = repo
            .finish(
                run.id,
                &RunOutcome::Succeeded {
                    volume: 120,
                    signals: 7,
                },
                at(30),
            )
            .await?;
        let second = repo
            .finish(
                run.id,
                &RunOutcome::Failed {
                    volume: 1,
                    signals: 1,
                    error: "late".to_owned(),
                },
                at(40),
            )
            .await?;
        Ok((first, second))
    })
    .await
    .unwrap();

    let first = first.unwrap();
    assert_eq!(first.status, RunStatus::Succeeded);
    assert_eq!(first.volume, 120);
    assert_eq!(first.signals, 7);
    assert_eq!(first.finished_at, Some(at(30)));
    assert!(first.error.is_none());
    assert!(second.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_run_keeps_its_error(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let run = started(profile, 0);

    let found = with_tx(&pool, async |shared| {
        let mut repo = PgRunRepository::new(&shared);
        repo.insert(&run).await?;
        repo.finish(
            run.id,
            &RunOutcome::Failed {
                volume: 3,
                signals: 1,
                error: "timeout".to_owned(),
            },
            at(5),
        )
        .await?;
        repo.find(run.id).await
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(found.status, RunStatus::Failed);
    assert_eq!(found.error.as_deref(), Some("timeout"));
    assert_eq!(found.volume, 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn finish_of_unknown_run_is_none(pool: PgPool) {
    let result = with_tx(&pool, async |shared| {
        PgRunRepository::new(&shared)
            .finish(
                RunId(uuid::Uuid::nil()),
                &RunOutcome::Succeeded {
                    volume: 0,
                    signals: 0,
                },
                at(1),
            )
            .await
    })
    .await
    .unwrap();

    assert!(result.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn counters_out_of_range_are_rejected(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let mut run = started(profile, 0);
    run.volume = u64::MAX;

    let result = with_tx(&pool, async |shared| {
        PgRunRepository::new(&shared).insert(&run).await
    })
    .await;

    assert!(matches!(result, Err(common::CoreError::Internal(_))));
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_recent_orders_and_limits(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let oldest = started(profile, 0);
    let middle = started(profile, 10);
    let newest = started(profile, 20);

    let (all, limited) = with_tx(&pool, async |shared| {
        let mut repo = PgRunRepository::new(&shared);
        repo.insert(&middle).await?;
        repo.insert(&oldest).await?;
        repo.insert(&newest).await?;
        let all = repo.list_recent(10).await?;
        let limited = repo.list_recent(2).await?;
        Ok((all, limited))
    })
    .await
    .unwrap();

    assert_eq!(all, [newest.clone(), middle.clone(), oldest]);
    assert_eq!(limited, [newest, middle]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_agent_round_trips_and_survives_finish(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let mut run = started(profile, 0);
    run.agent = Some(lodestone_core::definition::AgentId::new("ats-en"));

    let (found, finished) = with_tx(&pool, async |shared| {
        let mut repo = PgRunRepository::new(&shared);
        repo.insert(&run).await?;
        let found = repo.find(run.id).await?;
        let finished = repo
            .finish(
                run.id,
                &RunOutcome::Succeeded {
                    volume: 1,
                    signals: 1,
                },
                at(5),
            )
            .await?;
        Ok((found, finished))
    })
    .await
    .unwrap();

    assert_eq!(found.unwrap().agent, run.agent);
    assert_eq!(finished.unwrap().agent, run.agent);
}
