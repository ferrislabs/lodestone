use std::time::Duration;

use common::{CoreError, EncryptionKey, Secret};
use lodestone_core::{
    CipherError, LodestoneUseCase, ProviderId, ProviderKind, Tier, TierError, TokenPricing,
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
        pricing: TokenPricing {
            input_cents_per_mtok: 100,
            output_cents_per_mtok: 300,
        },
        api_key: key.map(Secret::new),
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
        pricing: TokenPricing {
            input_cents_per_mtok: 1,
            output_cents_per_mtok: 2,
        },
        api_key,
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
    assert!(matches!(error, CoreError::Conflict(_)));
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
