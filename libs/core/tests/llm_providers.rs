use std::time::Duration;

use common::{CoreError, EncryptionKey, Secret};
use lodestone_core::{
    CipherError, GenerationParams, LodestoneUseCase, MaxConcurrency, ProviderId, ProviderKind,
    ReasoningEffort, Tier, TierError, TokenPricing,
    infrastructure::crypto::AesGcmCipher,
    llm::{CreateLlmProvider, KeyChange, UpdateLlmProvider},
};
use sqlx::PgPool;

const CLEAR_KEY: &str = "sk-test-clear-key-0123456789";

fn cipher(byte: u8) -> AesGcmCipher {
    let encoded = match byte {
        1 => format!("{}QUE=", "QUFB".repeat(10)),
        _ => format!("{}QkI=", "QkJC".repeat(10)),
    };
    AesGcmCipher::new(&EncryptionKey::from_base64(&Secret::new(encoded)).unwrap()).unwrap()
}

fn create(name: &str, key: Option<&str>) -> CreateLlmProvider {
    CreateLlmProvider {
        name: name.to_owned(),
        kind: ProviderKind::Mistral,
        base_url: "https://api.example.com/v1".parse().unwrap(),
        model: "model-a".to_owned(),
        timeout: Duration::from_secs(30),
        max_retries: 2,
        max_concurrency: None,
        pricing: TokenPricing {
            input_cents_per_mtok: 100,
            output_cents_per_mtok: 300,
        },
        api_key: key.map(Secret::new),
        reasoning_effort: None,
        generation: GenerationParams::default(),
    }
}

fn update(id: ProviderId, api_key: KeyChange) -> UpdateLlmProvider {
    UpdateLlmProvider {
        id,
        name: "renamed".to_owned(),
        kind: ProviderKind::Mistral,
        base_url: "https://api.example.com/v2".parse().unwrap(),
        model: "model-b".to_owned(),
        timeout: Duration::from_secs(10),
        max_retries: 4,
        max_concurrency: MaxConcurrency::new(4).unwrap(),
        pricing: TokenPricing {
            input_cents_per_mtok: 1,
            output_cents_per_mtok: 2,
        },
        api_key,
        reasoning_effort: None,
        generation: GenerationParams::default(),
    }
}

async fn resolved_key(uc: &LodestoneUseCase, c: &AesGcmCipher) -> Option<String> {
    uc.resolve_llm_tier(Tier::Triage, c)
        .await
        .unwrap()
        .definition
        .api_key
        .map(|key| key.expose().to_owned())
}

#[sqlx::test(migrations = "../../migrations")]
async fn key_is_encrypted_at_rest(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool.clone());
    let c = cipher(1);
    let created = uc
        .create_llm_provider(create("a", Some(CLEAR_KEY)), &c)
        .await
        .unwrap();
    let listed = uc.list_llm_providers().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].api_key.is_some());
    assert!(!format!("{:?}", listed[0]).contains(CLEAR_KEY));

    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT api_key_ciphertext FROM llm_providers WHERE id = $1")
            .bind(created.id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        !stored
            .windows(CLEAR_KEY.len())
            .any(|w| w == CLEAR_KEY.as_bytes())
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn resolve_opens_key_only_with_the_right_cipher(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    let created = uc
        .create_llm_provider(create("a", Some(CLEAR_KEY)), &c)
        .await
        .unwrap();
    uc.assign_llm_tier(Tier::Triage, created.id).await.unwrap();

    assert_eq!(resolved_key(&uc, &c).await.as_deref(), Some(CLEAR_KEY));
    let error = uc
        .resolve_llm_tier(Tier::Triage, &cipher(2))
        .await
        .unwrap_err();
    assert_eq!(error, TierError::Cipher(CipherError::Open));
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_keep_replace_remove(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    let mut cmd = create("a", Some(CLEAR_KEY));
    cmd.kind = ProviderKind::OpenAiCompatible;
    let created = uc.create_llm_provider(cmd, &c).await.unwrap();
    uc.assign_llm_tier(Tier::Triage, created.id).await.unwrap();

    let mut keep = update(created.id, KeyChange::Keep);
    keep.kind = ProviderKind::OpenAiCompatible;
    let kept = uc.update_llm_provider(keep, &c).await.unwrap();
    assert_eq!(kept.name, "renamed");
    assert_eq!(resolved_key(&uc, &c).await.as_deref(), Some(CLEAR_KEY));

    let mut replace = update(created.id, KeyChange::Replace(Secret::new("sk-new")));
    replace.kind = ProviderKind::OpenAiCompatible;
    uc.update_llm_provider(replace, &c).await.unwrap();
    assert_eq!(resolved_key(&uc, &c).await.as_deref(), Some("sk-new"));

    let mut remove = update(created.id, KeyChange::Remove);
    remove.kind = ProviderKind::OpenAiCompatible;
    let removed = uc.update_llm_provider(remove, &c).await.unwrap();
    assert!(!removed.has_api_key());
    assert_eq!(resolved_key(&uc, &c).await, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_missing_is_not_found_and_keyless_mistral_is_refused(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    let error = uc
        .update_llm_provider(update(ProviderId(uuid::Uuid::nil()), KeyChange::Keep), &c)
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::NotFound));
    let error = uc
        .create_llm_provider(create("a", None), &c)
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::Invalid { ref field, .. } if field == "api_key"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn disabled_unassigned_and_duplicate(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    let error = uc.resolve_llm_tier(Tier::Triage, &c).await.unwrap_err();
    assert_eq!(error, TierError::Unassigned(Tier::Triage));

    let created = uc
        .create_llm_provider(create("a", Some(CLEAR_KEY)), &c)
        .await
        .unwrap();
    uc.assign_llm_tier(Tier::Triage, created.id).await.unwrap();
    uc.set_llm_provider_enabled(created.id, false)
        .await
        .unwrap();
    let error = uc.resolve_llm_tier(Tier::Triage, &c).await.unwrap_err();
    assert_eq!(error, TierError::Disabled(Tier::Triage));

    let error = uc
        .create_llm_provider(create("a", Some(CLEAR_KEY)), &c)
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::Conflict(_)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn tier_change_applies_on_next_call(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    let a = uc
        .create_llm_provider(create("a", Some("key-a")), &c)
        .await
        .unwrap();
    let b = uc
        .create_llm_provider(create("b", Some("key-b")), &c)
        .await
        .unwrap();

    uc.assign_llm_tier(Tier::Triage, a.id).await.unwrap();
    assert_eq!(
        uc.resolve_llm_tier(Tier::Triage, &c).await.unwrap().id,
        a.id
    );
    uc.assign_llm_tier(Tier::Triage, b.id).await.unwrap();
    assert_eq!(
        uc.resolve_llm_tier(Tier::Triage, &c).await.unwrap().id,
        b.id
    );
    assert_eq!(resolved_key(&uc, &c).await.as_deref(), Some("key-b"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn assigned_provider_cannot_be_deleted(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool.clone());
    let c = cipher(1);
    let created = uc
        .create_llm_provider(create("a", Some(CLEAR_KEY)), &c)
        .await
        .unwrap();
    uc.assign_llm_tier(Tier::Triage, created.id).await.unwrap();
    let result = sqlx::query("DELETE FROM llm_providers WHERE id = $1")
        .bind(created.id.0)
        .execute(&pool)
        .await;
    assert!(result.is_err());

    let missing = uc
        .assign_llm_tier(Tier::Qualification, ProviderId(uuid::Uuid::nil()))
        .await
        .unwrap_err();
    assert!(matches!(missing, CoreError::NotFound));
}

#[sqlx::test(migrations = "../../migrations")]
async fn delete_is_refused_while_assigned_and_allowed_after_unassign(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    let a = uc
        .create_llm_provider(create("a", Some("key-a")), &c)
        .await
        .unwrap();
    uc.assign_llm_tier(Tier::Qualification, a.id).await.unwrap();

    let error = uc.delete_llm_provider(a.id).await.unwrap_err();
    assert!(matches!(
        &error,
        CoreError::Conflict(message) if message == "provider is assigned to the qualification tier"
    ));
    assert_eq!(uc.list_llm_providers().await.unwrap().len(), 1);

    uc.unassign_llm_tier(Tier::Qualification).await.unwrap();
    uc.delete_llm_provider(a.id).await.unwrap();
    assert!(uc.list_llm_providers().await.unwrap().is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn delete_and_get_missing_are_not_found(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let missing = ProviderId(uuid::Uuid::nil());
    assert!(matches!(
        uc.delete_llm_provider(missing).await.unwrap_err(),
        CoreError::NotFound
    ));
    assert!(matches!(
        uc.get_llm_provider(missing).await.unwrap_err(),
        CoreError::NotFound
    ));
    assert!(matches!(
        uc.unassign_llm_tier(Tier::Triage).await.unwrap_err(),
        CoreError::NotFound
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn tiers_are_listed_and_provider_is_fetched(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    let a = uc
        .create_llm_provider(create("a", Some("key-a")), &c)
        .await
        .unwrap();
    assert!(uc.list_llm_tiers().await.unwrap().is_empty());
    uc.assign_llm_tier(Tier::Triage, a.id).await.unwrap();
    uc.assign_llm_tier(Tier::Qualification, a.id).await.unwrap();
    let mut tiers = uc.list_llm_tiers().await.unwrap();
    tiers.sort_by_key(|(tier, _)| tier.as_str());
    assert_eq!(
        tiers,
        vec![(Tier::Qualification, a.id), (Tier::Triage, a.id)]
    );
    assert_eq!(uc.get_llm_provider(a.id).await.unwrap().name, "a");
}

#[sqlx::test(migrations = "../../migrations")]
async fn reasoning_effort_round_trips_and_is_resolved(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool.clone());
    let c = cipher(1);
    let mut cmd = create("a", Some(CLEAR_KEY));
    cmd.reasoning_effort = Some(ReasoningEffort::None);
    let created = uc.create_llm_provider(cmd, &c).await.unwrap();
    assert_eq!(created.reasoning_effort, Some(ReasoningEffort::None));
    let stored: Option<String> =
        sqlx::query_scalar("SELECT reasoning_effort FROM llm_providers WHERE id = $1")
            .bind(created.id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored.as_deref(), Some("none"));
    assert_eq!(
        uc.get_llm_provider(created.id)
            .await
            .unwrap()
            .reasoning_effort,
        Some(ReasoningEffort::None)
    );
    uc.assign_llm_tier(Tier::Triage, created.id).await.unwrap();
    let resolved = uc.resolve_llm_tier(Tier::Triage, &c).await.unwrap();
    assert_eq!(
        resolved.definition.reasoning_effort,
        Some(ReasoningEffort::None)
    );

    let mut set = update(created.id, KeyChange::Keep);
    set.reasoning_effort = Some(ReasoningEffort::High);
    let updated = uc.update_llm_provider(set, &c).await.unwrap();
    assert_eq!(updated.reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(
        uc.list_llm_providers().await.unwrap()[0].reasoning_effort,
        Some(ReasoningEffort::High)
    );

    let cleared = uc
        .update_llm_provider(update(created.id, KeyChange::Keep), &c)
        .await
        .unwrap();
    assert_eq!(cleared.reasoning_effort, None);
    assert_eq!(
        uc.get_llm_provider(created.id)
            .await
            .unwrap()
            .reasoning_effort,
        None
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn reasoning_effort_outside_the_set_is_rejected_by_the_database(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool.clone());
    let created = uc
        .create_llm_provider(create("a", Some(CLEAR_KEY)), &cipher(1))
        .await
        .unwrap();
    let result = sqlx::query("UPDATE llm_providers SET reasoning_effort = 'max' WHERE id = $1")
        .bind(created.id.0)
        .execute(&pool)
        .await;
    assert!(result.is_err());
}

fn tuned() -> GenerationParams {
    GenerationParams {
        max_tokens: Some(900),
        temperature: Some(0.2),
        frequency_penalty: Some(0.4),
        presence_penalty: Some(-0.5),
        repeat_penalty: Some(1.3),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn generation_params_round_trip_and_are_resolved(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool.clone());
    let c = cipher(1);
    let mut cmd = create("a", Some(CLEAR_KEY));
    cmd.generation = tuned();
    let created = uc.create_llm_provider(cmd, &c).await.unwrap();
    assert_eq!(created.generation, tuned());
    assert_eq!(
        uc.get_llm_provider(created.id).await.unwrap().generation,
        tuned()
    );
    uc.assign_llm_tier(Tier::Triage, created.id).await.unwrap();
    let resolved = uc.resolve_llm_tier(Tier::Triage, &c).await.unwrap();
    assert_eq!(resolved.definition.generation, tuned());

    let mut change = update(created.id, KeyChange::Keep);
    change.generation = GenerationParams {
        temperature: Some(0.0),
        ..GenerationParams::default()
    };
    let updated = uc.update_llm_provider(change, &c).await.unwrap();
    assert_eq!(updated.generation.temperature, Some(0.0));
    assert_eq!(updated.generation.max_tokens, None);
    assert_eq!(updated.generation.repeat_penalty, None);
    assert_eq!(
        uc.list_llm_providers().await.unwrap()[0].generation,
        updated.generation
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn absent_generation_params_are_stored_as_null(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool.clone());
    let created = uc
        .create_llm_provider(create("a", Some(CLEAR_KEY)), &cipher(1))
        .await
        .unwrap();
    let nulls: bool = sqlx::query_scalar(
        "SELECT max_tokens IS NULL AND temperature IS NULL AND frequency_penalty IS NULL AND presence_penalty IS NULL AND repeat_penalty IS NULL FROM llm_providers WHERE id = $1",
    )
    .bind(created.id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(nulls);
}

#[sqlx::test(migrations = "../../migrations")]
async fn out_of_range_generation_params_are_refused_before_storage(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool.clone());
    let c = cipher(1);
    let mut cmd = create("a", Some(CLEAR_KEY));
    cmd.generation.temperature = Some(2.5);
    match uc.create_llm_provider(cmd, &c).await {
        Err(CoreError::Invalid { field, .. }) => assert_eq!(field, "temperature"),
        other => panic!("unexpected {other:?}"),
    }
    assert!(uc.list_llm_providers().await.unwrap().is_empty());
    let created = uc
        .create_llm_provider(create("b", Some(CLEAR_KEY)), &c)
        .await
        .unwrap();
    let mut change = update(created.id, KeyChange::Keep);
    change.generation.max_tokens = Some(0);
    match uc.update_llm_provider(change, &c).await {
        Err(CoreError::Invalid { field, .. }) => assert_eq!(field, "max_tokens"),
        other => panic!("unexpected {other:?}"),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn generation_params_outside_their_range_are_rejected_by_the_database(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool.clone());
    let created = uc
        .create_llm_provider(create("a", Some(CLEAR_KEY)), &cipher(1))
        .await
        .unwrap();
    for column in [
        "temperature = 3",
        "repeat_penalty = 0.5",
        "max_tokens = 0",
        "frequency_penalty = -3",
        "presence_penalty = 3",
    ] {
        let result = sqlx::query(&format!("UPDATE llm_providers SET {column} WHERE id = $1"))
            .bind(created.id.0)
            .execute(&pool)
            .await;
        assert!(result.is_err(), "{column}");
    }
}
