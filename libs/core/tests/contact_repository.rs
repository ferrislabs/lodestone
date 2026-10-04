mod support;

use common::CoreError;
use lodestone_core::{
    ContactEmail, ContactId, ContactKind, ContactMethod, ContactRepository, EmailAddress,
    Provenance, UnitInterval,
    infrastructure::{contact::postgres::PgContactRepository, postgres::with_tx},
};
use sqlx::PgPool;
use support::{at, create_profile, lead_id};

fn contact(lead: lodestone_core::LeadId, address: &str, confidence: f64) -> ContactEmail {
    ContactEmail {
        id: ContactId(common::generate_uuid_v7()),
        lead_id: lead,
        address: EmailAddress::parse(address).unwrap(),
        provenance: Provenance::parse("https://example.com/.well-known/security.txt").unwrap(),
        method: ContactMethod::SecurityTxt,
        confidence: UnitInterval::new(confidence).unwrap(),
        kind: ContactKind::Role,
        found_at: at(0),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn insert_then_list_orders_by_confidence_then_address(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead_id(&pool, profile, "example.com").await;
    let low = contact(lead, "info@example.com", 0.2);
    let high_b = contact(lead, "b@example.com", 0.9);
    let high_a = contact(lead, "a@example.com", 0.9);

    let listed = with_tx(&pool, async |shared| {
        let mut repo = PgContactRepository::new(&shared);
        repo.insert(&low).await?;
        repo.insert(&high_b).await?;
        repo.insert(&high_a).await?;
        repo.list_for_lead(lead).await
    })
    .await
    .unwrap();

    assert_eq!(listed, [high_a, high_b, low]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_address_for_a_lead_conflicts(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead_id(&pool, profile, "example.com").await;
    let first = contact(lead, "security@example.com", 0.9);
    let second = contact(lead, "security@example.com", 0.5);

    let result = with_tx(&pool, async |shared| {
        let mut repo = PgContactRepository::new(&shared);
        repo.insert(&first).await?;
        repo.insert(&second).await
    })
    .await;

    assert!(matches!(result, Err(CoreError::Conflict(_))));
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_address_on_two_leads_is_allowed(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let one = lead_id(&pool, profile, "example.com").await;
    let two = lead_id(&pool, profile, "example.org").await;

    with_tx(&pool, async |shared| {
        let mut repo = PgContactRepository::new(&shared);
        repo.insert(&contact(one, "security@example.com", 0.9))
            .await?;
        repo.insert(&contact(two, "security@example.com", 0.9))
            .await
    })
    .await
    .unwrap();
}
