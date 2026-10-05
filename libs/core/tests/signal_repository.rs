mod support;

use lodestone_core::{
    Provenance, Signal, SignalId, SignalKind, SignalRepository, SourceName, UnitInterval,
    definition::AgentId,
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
        agent: None,
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
        repo.record(&late).await?;
        repo.record(&early).await?;
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
        repo.record(&stored).await?;
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
        repo.record(&zero).await?;
        repo.record(&one).await?;
        repo.list_for_lead(lead).await
    })
    .await
    .unwrap();

    assert_eq!(listed.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_fact_observed_again_refreshes_instead_of_duplicating(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead_id(&pool, profile, "example.com").await;
    let first = signal(
        lead,
        "product_detected",
        Some("keycloak"),
        10,
        serde_json::json!({"run": 1}),
    );
    let mut again = signal(
        lead,
        "product_detected",
        Some("keycloak"),
        30,
        serde_json::json!({"run": 2}),
    );
    again.strength = UnitInterval::new(0.9).unwrap();
    let older = signal(
        lead,
        "product_detected",
        Some("keycloak"),
        5,
        serde_json::json!({"run": 0}),
    );

    let (inserted, listed) = with_tx(&pool, async |shared| {
        let mut repo = PgSignalRepository::new(&shared);
        let inserted = [
            repo.record(&first).await?,
            repo.record(&again).await?,
            repo.record(&older).await?,
        ];
        Ok((inserted, repo.list_for_lead(lead).await?))
    })
    .await
    .unwrap();

    assert_eq!(inserted, [true, false, false]);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].observed_at, at(30));
    assert_eq!(listed[0].raw, serde_json::json!({"run": 2}));
    assert_eq!(listed[0].strength.get(), 0.9);
}

#[sqlx::test(migrations = "../../migrations")]
async fn different_value_or_provenance_is_a_different_signal(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead_id(&pool, profile, "example.com").await;
    let keycloak = signal(
        lead,
        "product_detected",
        Some("keycloak"),
        1,
        serde_json::json!({}),
    );
    let authentik = signal(
        lead,
        "product_detected",
        Some("authentik"),
        1,
        serde_json::json!({}),
    );
    let mut other_host = signal(
        lead,
        "product_detected",
        Some("keycloak"),
        1,
        serde_json::json!({}),
    );
    other_host.provenance = Provenance::parse("https://sso.example.com/realms/master").unwrap();

    let listed = with_tx(&pool, async |shared| {
        let mut repo = PgSignalRepository::new(&shared);
        repo.record(&keycloak).await?;
        repo.record(&authentik).await?;
        repo.record(&other_host).await?;
        repo.list_for_lead(lead).await
    })
    .await
    .unwrap();

    assert_eq!(listed.len(), 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_agent_is_stored_and_the_first_one_is_kept_on_replay(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead_id(&pool, profile, "example.com").await;
    let mut first = signal(
        lead,
        "product_detected",
        Some("keycloak"),
        10,
        serde_json::json!({}),
    );
    first.agent = Some(AgentId::new("probe-a"));
    let mut second = signal(
        lead,
        "product_detected",
        Some("keycloak"),
        20,
        serde_json::json!({}),
    );
    second.agent = Some(AgentId::new("probe-b"));
    let mut legacy = signal(lead, "version_detected", None, 5, serde_json::json!({}));
    legacy.agent = None;

    let (inserted, listed) = with_tx(&pool, async |shared| {
        let mut repo = PgSignalRepository::new(&shared);
        let a = repo.record(&first).await?;
        let b = repo.record(&second).await?;
        repo.record(&legacy).await?;
        Ok(((a, b), repo.list_for_lead(lead).await?))
    })
    .await
    .unwrap();

    assert_eq!(inserted, (true, false));
    assert_eq!(first.fingerprint(), second.fingerprint());
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].agent, None);
    assert_eq!(listed[1].agent, Some(AgentId::new("probe-a")));
}
