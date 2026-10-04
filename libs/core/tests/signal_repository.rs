mod support;

use lodestone_core::{
    Provenance, Signal, SignalId, SignalKind, SignalRepository, SourceName, UnitInterval,
    infrastructure::{postgres::with_tx, signal::postgres::PgSignalRepository},
};
use sqlx::PgPool;
use support::{at, create_profile, lead_id};

fn signal(
    lead: lodestone_core::LeadId,
    kind: &str,
    value: Option<&str>,
    observed: i64,
    raw: serde_json::Value,
) -> Signal {
    Signal {
        id: SignalId(common::generate_uuid_v7()),
        lead_id: lead,
        kind: SignalKind::parse(kind).unwrap(),
        value: value.map(str::to_owned),
        source: SourceName::HttpProbe,
        raw,
        provenance: Provenance::parse("https://example.com/realms/master").unwrap(),
        strength: UnitInterval::new(0.8).unwrap(),
        observed_at: at(observed),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn insert_then_list_in_observation_order(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead_id(&pool, profile, "example.com").await;
    let late = signal(
        lead,
        "product_detected",
        Some("keycloak"),
        20,
        serde_json::json!({}),
    );
    let early = signal(lead, "version_detected", None, 10, serde_json::json!({}));

    let listed = with_tx(&pool, async |shared| {
        let mut repo = PgSignalRepository::new(&shared);
        repo.insert(&late).await?;
        repo.insert(&early).await?;
        repo.list_for_lead(lead).await
    })
    .await
    .unwrap();

    assert_eq!(listed, [early, late]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn raw_json_round_trips(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead_id(&pool, profile, "example.com").await;
    let raw = serde_json::json!({"headers": {"server": "nginx"}, "ports": [80, 443], "ok": true, "n": null});
    let stored = signal(lead, "product_detected", Some("keycloak"), 1, raw.clone());

    let listed = with_tx(&pool, async |shared| {
        let mut repo = PgSignalRepository::new(&shared);
        repo.insert(&stored).await?;
        repo.list_for_lead(lead).await
    })
    .await
    .unwrap();

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].raw, raw);
    assert_eq!(listed[0], stored);
}

#[sqlx::test(migrations = "../../migrations")]
async fn boundary_strengths_are_accepted(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead_id(&pool, profile, "example.com").await;
    let mut zero = signal(lead, "a", None, 1, serde_json::json!({}));
    zero.strength = UnitInterval::new(0.0).unwrap();
    let mut one = signal(lead, "b", None, 2, serde_json::json!({}));
    one.strength = UnitInterval::new(1.0).unwrap();

    let listed = with_tx(&pool, async |shared| {
        let mut repo = PgSignalRepository::new(&shared);
        repo.insert(&zero).await?;
        repo.insert(&one).await?;
        repo.list_for_lead(lead).await
    })
    .await
    .unwrap();

    assert_eq!(listed.len(), 2);
}
