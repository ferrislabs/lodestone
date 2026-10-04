mod support;

use chrono::Utc;
use common::CoreError;
use lodestone_core::{
    EmailAddress, SuppressionEntry, SuppressionId, SuppressionRepository, SuppressionTarget,
    infrastructure::{postgres::with_tx, suppression::postgres::PgSuppressionRepository},
};
use sqlx::PgPool;
use support::domain;

fn entry(target: SuppressionTarget) -> SuppressionEntry {
    SuppressionEntry {
        id: SuppressionId(common::generate_uuid_v7()),
        target,
        reason: Some("opt-out".to_owned()),
        created_by: None,
        created_at: Utc::now(),
    }
}

fn email(raw: &str) -> EmailAddress {
    EmailAddress::parse(raw).unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn domain_entry_suppresses_the_domain(pool: PgPool) {
    let stored = entry(SuppressionTarget::Domain(domain("example.com")));

    let (hit, miss, listed) = with_tx(&pool, async |shared| {
        let mut repo = PgSuppressionRepository::new(&shared);
        repo.insert(&stored).await?;
        let hit = repo.is_domain_suppressed(&domain("example.com")).await?;
        let miss = repo.is_domain_suppressed(&domain("example.org")).await?;
        let listed = repo.list().await?;
        Ok((hit, miss, listed))
    })
    .await
    .unwrap();

    assert!(hit);
    assert!(!miss);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].target, stored.target);
    assert_eq!(listed[0].reason, stored.reason);
}

#[sqlx::test(migrations = "../../migrations")]
async fn email_entry_suppresses_that_address_only(pool: PgPool) {
    let stored = entry(SuppressionTarget::Email(email("it@example.com")));

    let (direct, other, domain_hit) = with_tx(&pool, async |shared| {
        let mut repo = PgSuppressionRepository::new(&shared);
        repo.insert(&stored).await?;
        let direct = repo.is_email_suppressed(&email("IT@example.com")).await?;
        let other = repo.is_email_suppressed(&email("ops@example.com")).await?;
        let domain_hit = repo.is_domain_suppressed(&domain("example.com")).await?;
        Ok((direct, other, domain_hit))
    })
    .await
    .unwrap();

    assert!(direct);
    assert!(!other);
    assert!(!domain_hit);
}

#[sqlx::test(migrations = "../../migrations")]
async fn email_is_suppressed_through_its_registrable_domain(pool: PgPool) {
    let stored = entry(SuppressionTarget::Domain(domain("example.com")));

    let (through_domain, unrelated) = with_tx(&pool, async |shared| {
        let mut repo = PgSuppressionRepository::new(&shared);
        repo.insert(&stored).await?;
        let through_domain = repo
            .is_email_suppressed(&email("it@sub.example.com"))
            .await?;
        let unrelated = repo.is_email_suppressed(&email("it@example.org")).await?;
        Ok((through_domain, unrelated))
    })
    .await
    .unwrap();

    assert!(through_domain);
    assert!(!unrelated);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unparseable_email_domain_only_matches_the_email_row(pool: PgPool) {
    let stored = entry(SuppressionTarget::Email(email("a@localhost.invalidtld1")));

    let (hit, miss) = with_tx(&pool, async |shared| {
        let mut repo = PgSuppressionRepository::new(&shared);
        repo.insert(&stored).await?;
        let hit = repo
            .is_email_suppressed(&email("a@localhost.invalidtld1"))
            .await?;
        let miss = repo
            .is_email_suppressed(&email("b@localhost.invalidtld1"))
            .await?;
        Ok((hit, miss))
    })
    .await
    .unwrap();

    assert!(hit);
    assert!(!miss);
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_target_conflicts(pool: PgPool) {
    let first = entry(SuppressionTarget::Domain(domain("example.com")));
    let second = entry(SuppressionTarget::Domain(domain("example.com")));

    let result = with_tx(&pool, async |shared| {
        let mut repo = PgSuppressionRepository::new(&shared);
        repo.insert(&first).await?;
        repo.insert(&second).await
    })
    .await;

    assert!(matches!(result, Err(CoreError::Conflict(_))));
}

#[sqlx::test(migrations = "../../migrations")]
async fn remove_reports_whether_a_row_was_deleted(pool: PgPool) {
    let stored = entry(SuppressionTarget::Domain(domain("example.com")));

    let (first, second, still) = with_tx(&pool, async |shared| {
        let mut repo = PgSuppressionRepository::new(&shared);
        repo.insert(&stored).await?;
        let first = repo.remove(stored.id).await?;
        let second = repo.remove(stored.id).await?;
        let still = repo.is_domain_suppressed(&domain("example.com")).await?;
        Ok((first, second, still))
    })
    .await
    .unwrap();

    assert!(first);
    assert!(!second);
    assert!(!still);
}
