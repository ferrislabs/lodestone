mod support;

use std::time::Duration;

use lodestone_core::{
    HostVisitRepository,
    infrastructure::{host_visit::postgres::PgHostVisitRepository, postgres::with_tx},
};
use sqlx::PgPool;
use support::at;

const REVISIT: Duration = Duration::from_secs(3600);

async fn reserve(pool: &PgPool, host: &str, seconds: i64) -> bool {
    let host = host.to_owned();
    with_tx(pool, async |shared| {
        PgHostVisitRepository::new(&shared)
            .reserve_probe(&host, at(seconds), REVISIT)
            .await
    })
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn first_probe_of_a_host_is_reserved(pool: PgPool) {
    assert!(reserve(&pool, "sso.example.com", 0).await);
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_host_within_revisit_interval_is_refused(pool: PgPool) {
    assert!(reserve(&pool, "sso.example.com", 0).await);
    assert!(!reserve(&pool, "sso.example.com", 1800).await);
    assert!(!reserve(&pool, "SSO.Example.com", 3599).await);
}

#[sqlx::test(migrations = "../../migrations")]
async fn host_is_reserved_again_once_the_interval_elapsed(pool: PgPool) {
    assert!(reserve(&pool, "sso.example.com", 0).await);
    assert!(reserve(&pool, "sso.example.com", 3600).await);
    assert!(!reserve(&pool, "sso.example.com", 3601).await);
}

#[sqlx::test(migrations = "../../migrations")]
async fn hosts_are_independent(pool: PgPool) {
    assert!(reserve(&pool, "sso.example.com", 0).await);
    assert!(reserve(&pool, "auth.example.com", 0).await);
}
