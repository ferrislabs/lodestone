mod support;

use lodestone_core::{
    LeadRepository, LeadStatus, Score,
    infrastructure::{lead::postgres::PgLeadRepository, postgres::with_tx},
};
use sqlx::PgPool;
use support::{at, create_profile, domain, seen_lead};

#[sqlx::test(migrations = "../../migrations")]
async fn record_seen_creates_new_lead(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = seen_lead(&pool, profile, "example.com").await;

    assert_eq!(lead.profile_id, profile);
    assert_eq!(lead.domain, domain("example.com"));
    assert_eq!(lead.status, LeadStatus::New);
    assert_eq!(lead.score.get(), 0.0);
    assert_eq!(lead.first_seen_at, at(0));
    assert_eq!(lead.last_seen_at, at(0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn record_seen_moves_only_the_relevant_bound(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let first = seen_lead(&pool, profile, "example.com").await;

    let (after, before) = with_tx(&pool, async |shared| {
        let mut repo = PgLeadRepository::new(&shared);
        repo.update_status(first.id, LeadStatus::New, LeadStatus::Qualified, at(5))
            .await?;
        repo.update_score(first.id, total(42.0), at(5)).await?;
        let after = repo
            .record_seen(profile, &domain("example.com"), at(100))
            .await?;
        let before = repo
            .record_seen(profile, &domain("example.com"), at(-100))
            .await?;
        Ok((after, before))
    })
    .await
    .unwrap();

    assert_eq!(after.id, first.id);
    assert_eq!(after.first_seen_at, at(0));
    assert_eq!(after.last_seen_at, at(100));
    assert_eq!(before.id, first.id);
    assert_eq!(before.first_seen_at, at(-100));
    assert_eq!(before.last_seen_at, at(100));
    assert_eq!(before.status, LeadStatus::Qualified);
    assert_eq!(before.score.get(), 42.0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_domain_under_two_profiles_yields_two_leads(pool: PgPool) {
    let a = create_profile(&pool, "alpha").await;
    let b = create_profile(&pool, "beta").await;
    let one = seen_lead(&pool, a, "example.com").await;
    let two = seen_lead(&pool, b, "example.com").await;

    assert_ne!(one.id, two.id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_status_requires_matching_from(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = seen_lead(&pool, profile, "example.com").await;

    let (moved, stale, absent) = with_tx(&pool, async |shared| {
        let mut repo = PgLeadRepository::new(&shared);
        let moved = repo
            .update_status(lead.id, LeadStatus::New, LeadStatus::Qualified, at(10))
            .await?;
        let stale = repo
            .update_status(lead.id, LeadStatus::New, LeadStatus::Rejected, at(11))
            .await?;
        let absent = repo
            .update_status(
                lodestone_core::LeadId(uuid::Uuid::nil()),
                LeadStatus::New,
                LeadStatus::Rejected,
                at(12),
            )
            .await?;
        Ok((moved, stale, absent))
    })
    .await
    .unwrap();

    let moved = moved.unwrap();
    assert_eq!(moved.status, LeadStatus::Qualified);
    assert_eq!(moved.updated_at, at(10));
    assert!(stale.is_none());
    assert!(absent.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_score_persists_and_misses_absent(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = seen_lead(&pool, profile, "example.com").await;

    let (updated, absent, found) = with_tx(&pool, async |shared| {
        let mut repo = PgLeadRepository::new(&shared);
        let updated = repo.update_score(lead.id, total(7.5), at(3)).await?;
        let absent = repo
            .update_score(lodestone_core::LeadId(uuid::Uuid::nil()), total(1.0), at(3))
            .await?;
        let found = repo.find(lead.id).await?;
        Ok((updated, absent, found))
    })
    .await
    .unwrap();

    assert_eq!(updated.unwrap().score.get(), 7.5);
    assert!(absent.is_none());
    assert_eq!(found.unwrap().score.get(), 7.5);
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_by_profile_orders_by_score_then_domain(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let other = create_profile(&pool, "beta").await;
    seen_lead(&pool, other, "zzz.com").await;

    let listed = with_tx(&pool, async |shared| {
        let mut repo = PgLeadRepository::new(&shared);
        let mut ids = Vec::new();
        for host in ["b.com", "a.com", "c.com"] {
            ids.push(repo.record_seen(profile, &domain(host), at(0)).await?.id);
        }
        repo.update_score(ids[0], total(5.0), at(1)).await?;
        repo.list_by_profile(profile).await
    })
    .await
    .unwrap();

    let domains: Vec<&str> = listed.iter().map(|lead| lead.domain.as_str()).collect();
    assert_eq!(domains, ["b.com", "a.com", "c.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn find_by_domain_scopes_to_profile(pool: PgPool) {
    let a = create_profile(&pool, "alpha").await;
    let b = create_profile(&pool, "beta").await;
    let lead = seen_lead(&pool, a, "example.com").await;

    let (hit, wrong_profile, missing) = with_tx(&pool, async |shared| {
        let mut repo = PgLeadRepository::new(&shared);
        let hit = repo.find_by_domain(a, &domain("example.com")).await?;
        let wrong_profile = repo.find_by_domain(b, &domain("example.com")).await?;
        let missing = repo.find_by_domain(a, &domain("other.com")).await?;
        Ok((hit, wrong_profile, missing))
    })
    .await
    .unwrap();

    assert_eq!(hit, Some(lead));
    assert!(wrong_profile.is_none());
    assert!(missing.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_profile_cascades_to_leads(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    seen_lead(&pool, profile, "example.com").await;

    sqlx::query("DELETE FROM profiles WHERE id = $1")
        .bind(profile.0)
        .execute(&pool)
        .await
        .unwrap();

    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM leads")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
}

fn total(value: f64) -> Score {
    Score::new(value).unwrap()
}
