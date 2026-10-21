use chrono::{DateTime, Duration, TimeZone, Utc};
use common::RobotsPolicy;
use lodestone_core::{
    CreateUserCommand, LodestoneUseCase, SettingOrigin, UserId, settings::CrawlSettingsUpdate,
};
use sqlx::{PgPool, query, query_as};

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

async fn user(uc: &LodestoneUseCase) -> UserId {
    uc.create_user(CreateUserCommand {
        name: "Alice Martin".to_owned(),
        username: "alice".to_owned(),
        email: None,
        sub: "subject-1".to_owned(),
    })
    .await
    .unwrap()
    .id
}

async fn dossier(pool: &PgPool, domain: &str, state: &str, error: Option<&str>) {
    query(
        "INSERT INTO company_dossiers (domain, state, pages, attempts, next_attempt_at, error)
         VALUES ($1, $2, '[]'::jsonb, 3, $3, $4)",
    )
    .bind(domain)
    .bind(state)
    .bind(at(0) + Duration::days(30))
    .bind(error)
    .execute(pool)
    .await
    .unwrap();
}

async fn schedule(pool: &PgPool, domain: &str) -> (i32, DateTime<Utc>) {
    query_as("SELECT attempts, next_attempt_at FROM company_dossiers WHERE domain = $1")
        .bind(domain)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_a_saved_value_the_environment_seed_applies(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);

    let respect = uc.crawl_settings(RobotsPolicy::Respect).await.unwrap();
    let ignore = uc.crawl_settings(RobotsPolicy::Ignore).await.unwrap();

    assert_eq!(respect.robots_policy, RobotsPolicy::Respect);
    assert_eq!(ignore.robots_policy, RobotsPolicy::Ignore);
    assert_eq!(ignore.origin, SettingOrigin::Environment);
    assert!(ignore.updated_by.is_none());
    assert!(ignore.updated_at.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_saved_value_records_who_and_when_and_wins_over_the_seed(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let alice = user(&uc).await;

    let CrawlSettingsUpdate { settings, .. } = uc
        .update_crawl_settings(RobotsPolicy::Ignore, alice, at(10), RobotsPolicy::Respect)
        .await
        .unwrap();
    assert_eq!(settings.robots_policy, RobotsPolicy::Ignore);

    let read = uc.crawl_settings(RobotsPolicy::Respect).await.unwrap();
    assert_eq!(read.robots_policy, RobotsPolicy::Ignore);
    assert_eq!(read.origin, SettingOrigin::Organisation);
    assert_eq!(read.updated_by, Some(alice));
    assert_eq!(read.updated_by_name.as_deref(), Some("Alice Martin"));
    assert_eq!(read.updated_at, Some(at(10)));

    uc.update_crawl_settings(RobotsPolicy::Respect, alice, at(20), RobotsPolicy::Ignore)
        .await
        .unwrap();
    let read = uc.crawl_settings(RobotsPolicy::Ignore).await.unwrap();
    assert_eq!(read.robots_policy, RobotsPolicy::Respect);
    assert_eq!(read.updated_at, Some(at(20)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn switching_to_ignore_retries_dossiers_refused_by_robots(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool.clone());
    let alice = user(&uc).await;
    dossier(
        &pool,
        "refused.fr",
        "unreachable",
        Some("robots.txt disallows the home page"),
    )
    .await;
    dossier(&pool, "down.fr", "unreachable", Some("request timed out")).await;
    dossier(
        &pool,
        "kept.fr",
        "partial",
        Some("robots.txt disallows the home page"),
    )
    .await;

    let respected = uc
        .update_crawl_settings(RobotsPolicy::Respect, alice, at(5), RobotsPolicy::Respect)
        .await
        .unwrap();
    assert_eq!(respected.dossiers_retried, 0);
    assert_eq!(schedule(&pool, "refused.fr").await.0, 3);

    let ignored = uc
        .update_crawl_settings(RobotsPolicy::Ignore, alice, at(10), RobotsPolicy::Respect)
        .await
        .unwrap();
    assert_eq!(ignored.dossiers_retried, 1);
    assert_eq!(schedule(&pool, "refused.fr").await, (0, at(10)));
    assert_eq!(
        schedule(&pool, "down.fr").await,
        (3, at(0) + Duration::days(30))
    );
    assert_eq!(
        schedule(&pool, "kept.fr").await,
        (3, at(0) + Duration::days(30))
    );
}
