mod support;

use chrono::NaiveDate;
use lodestone_core::{
    LodestoneUseCase, PartnerAward, PartnerRepository, ProfileId, Provenance, Siret,
    infrastructure::{partner::postgres::PgPartnerRepository, postgres::with_tx},
};
use sqlx::PgPool;
use support::{at, create_profile};

const HOLDER: &str = "83177947500027";
const BUYER: &str = "25351449100047";

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
}

fn award(holder: &str, contract: &str, day: u32) -> PartnerAward {
    PartnerAward {
        holder: Siret::parse(holder).unwrap(),
        holder_name: None,
        contract_id: contract.to_owned(),
        buyer: Siret::parse(BUYER).unwrap(),
        object: "Solution SSO".to_owned(),
        amount: Some(1234.5),
        notified_on: date(day),
        provenance: Provenance::parse(&format!(
            "https://www.data.gouv.fr/fr/datasets/r/abc#{contract}"
        ))
        .unwrap(),
        matched: vec!["SSO".to_owned()],
    }
}

async fn record(pool: &PgPool, profile: ProfileId, award: &PartnerAward, second: i64) -> bool {
    with_tx(pool, async |shared| {
        PgPartnerRepository::new(&shared)
            .record_award(profile, award, at(second))
            .await
    })
    .await
    .unwrap()
}

async fn list(pool: &PgPool, profile: ProfileId) -> Vec<lodestone_core::Partner> {
    with_tx(pool, async |shared| {
        PgPartnerRepository::new(&shared)
            .list_for_profile(profile)
            .await
    })
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn first_award_creates_the_partner(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let mut first = award(HOLDER, "c1", 3);
    first.holder_name = Some("ACME SAS".to_owned());
    assert!(record(&pool, profile, &first, 0).await);

    let partners = list(&pool, profile).await;
    assert_eq!(partners.len(), 1);
    assert_eq!(partners[0].siret.as_str(), HOLDER);
    assert_eq!(partners[0].name.as_deref(), Some("ACME SAS"));
    assert_eq!(partners[0].award_count, 1);
    assert_eq!(partners[0].last_award_at, Some(date(3)));
    assert_eq!(partners[0].first_seen_at, at(0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn replaying_an_award_changes_nothing(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let first = award(HOLDER, "c1", 3);
    assert!(record(&pool, profile, &first, 0).await);
    let before = list(&pool, profile).await;
    assert!(!record(&pool, profile, &first, 500).await);
    assert!(!record(&pool, profile, &first, 900).await);

    assert_eq!(list(&pool, profile).await, before);
    let awards = sqlx::query_scalar!("SELECT COUNT(*) AS \"count!\" FROM partner_awards")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(awards, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn count_and_last_date_follow_the_awards(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    record(&pool, profile, &award(HOLDER, "c1", 5), 0).await;
    record(&pool, profile, &award(HOLDER, "c2", 2), 1).await;
    record(&pool, profile, &award(HOLDER, "c3", 4), 2).await;
    record(&pool, profile, &award(HOLDER, "c2", 2), 3).await;

    let partners = list(&pool, profile).await;
    assert_eq!(partners.len(), 1);
    assert_eq!(partners[0].award_count, 3);
    assert_eq!(partners[0].last_award_at, Some(date(5)));
    assert_eq!(partners[0].first_seen_at, at(0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_later_name_fills_a_missing_one_and_a_missing_one_keeps_it(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    record(&pool, profile, &award(HOLDER, "c1", 1), 0).await;
    assert_eq!(list(&pool, profile).await[0].name, None);

    let mut named = award(HOLDER, "c2", 2);
    named.holder_name = Some("ACME SAS".to_owned());
    record(&pool, profile, &named, 1).await;
    record(&pool, profile, &award(HOLDER, "c3", 3), 2).await;
    assert_eq!(
        list(&pool, profile).await[0].name.as_deref(),
        Some("ACME SAS")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn profiles_are_isolated(pool: PgPool) {
    let alpha = create_profile(&pool, "alpha").await;
    let beta = create_profile(&pool, "beta").await;
    record(&pool, alpha, &award(HOLDER, "c1", 3), 0).await;
    record(&pool, alpha, &award(HOLDER, "c2", 4), 0).await;
    assert!(record(&pool, beta, &award(HOLDER, "c1", 3), 0).await);

    let alpha_partners = list(&pool, alpha).await;
    let beta_partners = list(&pool, beta).await;
    assert_eq!(alpha_partners[0].award_count, 2);
    assert_eq!(beta_partners[0].award_count, 1);
    assert_ne!(alpha_partners[0].id, beta_partners[0].id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn partners_are_listed_most_recent_first(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    record(&pool, profile, &award("11111111111111", "c1", 2), 0).await;
    record(&pool, profile, &award("22222222222222", "c2", 9), 0).await;

    let sirets: Vec<String> = list(&pool, profile)
        .await
        .iter()
        .map(|partner| partner.siret.to_string())
        .collect();
    assert_eq!(sirets, ["22222222222222", "11111111111111"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_use_case_skips_a_holder_equal_to_the_buyer(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let awards = [award(BUYER, "c1", 3), award(HOLDER, "c1", 3)];
    let inserted = usecase
        .record_partner_awards(profile, &awards, at(0))
        .await
        .unwrap();
    assert_eq!(inserted, 1);
    let again = usecase
        .record_partner_awards(profile, &awards, at(10))
        .await
        .unwrap();
    assert_eq!(again, 0);

    let partners = list(&pool, profile).await;
    assert_eq!(partners.len(), 1);
    assert_eq!(partners[0].siret.as_str(), HOLDER);
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_profile_removes_its_partners(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    record(&pool, profile, &award(HOLDER, "c1", 3), 0).await;
    sqlx::query!("DELETE FROM profiles WHERE id = $1", profile.0)
        .execute(&pool)
        .await
        .unwrap();
    let rows = sqlx::query_scalar!("SELECT COUNT(*) AS \"count!\" FROM partner_awards")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}
