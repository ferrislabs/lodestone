use std::collections::BTreeMap;

use chrono::Utc;
use common::{CoreError, EncryptionKey, Secret};
use lodestone_core::{
    ConnectedAccount, CreateUserCommand, LodestoneUseCase, ProviderStatus, SecretCipher,
    SourceProviderDescriptor, TOKEN_ACCESS, UserId,
    infrastructure::crypto::AesGcmCipher,
    provider_descriptor, secret_context,
    source_providers::{
        NewOAuthState, OAuthStateError, ProviderConfigError, SaveProviderConfiguration,
        SecretChange, TokenUpdate, open_secrets,
    },
};
use serde_json::{Map, Value, json};
use sqlx::{PgPool, query, query_scalar};

const SECRET: &str = "fts-secret-0123456789-abcd";

fn cipher(byte: u8) -> AesGcmCipher {
    let encoded = match byte {
        1 => format!("{}QUE=", "QUFB".repeat(10)),
        _ => format!("{}QkI=", "QkJC".repeat(10)),
    };
    AesGcmCipher::new(&EncryptionKey::from_base64(&Secret::new(encoded)).unwrap()).unwrap()
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn command(client_id: &str, secret: Option<SecretChange>) -> SaveProviderConfiguration {
    let mut secrets = BTreeMap::new();
    if let Some(change) = secret {
        secrets.insert("client_secret".to_owned(), change);
    }
    SaveProviderConfiguration {
        settings: object(json!({"client_id": client_id})),
        secrets,
    }
}

fn france_travail() -> &'static SourceProviderDescriptor {
    provider_descriptor("france_travail").unwrap()
}

fn problems(error: ProviderConfigError) -> Vec<(String, String)> {
    match error {
        ProviderConfigError::Invalid(problems) => problems
            .into_iter()
            .map(|problem| (problem.field, problem.message))
            .collect(),
        other => panic!("expected invalid, got {other:?}"),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn saving_stores_settings_with_defaults_and_marks_the_provider_untested(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);

    let saved = uc
        .save_provider_configuration(
            france_travail(),
            command("client-1", Some(SecretChange::Replace(Secret::new(SECRET)))),
            false,
            &c,
        )
        .await
        .unwrap();

    assert_eq!(saved.status, ProviderStatus::Untested);
    assert_eq!(saved.settings["client_id"], "client-1");
    assert!(saved.settings["token_url"].as_str().is_some());
    assert!(!uc.is_provider_usable("france_travail").await.unwrap());
    let stored = uc
        .get_provider_configuration("france_travail")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.settings, saved.settings);
    assert!(stored.has_secret("client_secret"));
    assert_eq!(uc.list_provider_configurations().await.unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn secrets_are_encrypted_at_rest_and_round_trip(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool.clone());
    let c = cipher(1);
    uc.save_provider_configuration(
        france_travail(),
        command("client-1", Some(SecretChange::Replace(Secret::new(SECRET)))),
        false,
        &c,
    )
    .await
    .unwrap();

    let ciphertext: Vec<u8> = query_scalar(
        "SELECT ciphertext FROM source_provider_secrets WHERE provider_key = 'france_travail'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !ciphertext
            .windows(SECRET.len())
            .any(|window| window == SECRET.as_bytes())
    );

    let stored = uc
        .get_provider_configuration("france_travail")
        .await
        .unwrap()
        .unwrap();
    assert!(!format!("{stored:?}").contains(SECRET));
    let opened = open_secrets(&stored, &c).unwrap();
    assert_eq!(opened["client_secret"].expose(), SECRET);
    assert!(open_secrets(&stored, &cipher(2)).is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_secret_is_bound_to_its_provider_and_name(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    let saved = uc
        .save_provider_configuration(
            france_travail(),
            command("client-1", Some(SecretChange::Replace(Secret::new(SECRET)))),
            false,
            &c,
        )
        .await
        .unwrap();
    let sealed = &saved.secrets["client_secret"];
    assert!(
        c.open(sealed, &secret_context("france_travail", "client_secret"))
            .is_ok()
    );
    assert!(
        c.open(sealed, &secret_context("other", "client_secret"))
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn absent_keeps_the_secret_replace_changes_it_and_a_required_one_cannot_be_removed(
    pool: PgPool,
) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    uc.save_provider_configuration(
        france_travail(),
        command("client-1", Some(SecretChange::Replace(Secret::new(SECRET)))),
        false,
        &c,
    )
    .await
    .unwrap();

    let kept = uc
        .save_provider_configuration(france_travail(), command("client-2", None), false, &c)
        .await
        .unwrap();
    assert_eq!(
        open_secrets(&kept, &c).unwrap()["client_secret"].expose(),
        SECRET
    );

    let replaced = uc
        .save_provider_configuration(
            france_travail(),
            command(
                "client-2",
                Some(SecretChange::Replace(Secret::new("another"))),
            ),
            false,
            &c,
        )
        .await
        .unwrap();
    assert_eq!(
        open_secrets(&replaced, &c).unwrap()["client_secret"].expose(),
        "another"
    );

    let error = uc
        .save_provider_configuration(
            france_travail(),
            command("client-2", Some(SecretChange::Remove)),
            false,
            &c,
        )
        .await
        .unwrap_err();
    assert_eq!(
        problems(error),
        vec![("client_secret".to_owned(), "is required".to_owned())]
    );
    let still = uc
        .get_provider_configuration("france_travail")
        .await
        .unwrap()
        .unwrap();
    assert!(still.has_secret("client_secret"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_settings_and_unknown_secrets_are_reported_and_nothing_is_stored(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    let mut bad = command("", Some(SecretChange::Replace(Secret::new(SECRET))));
    bad.settings
        .insert("token_url".to_owned(), json!("ftp://x"));
    bad.secrets
        .insert("nope".to_owned(), SecretChange::Replace(Secret::new("x")));

    let found = problems(
        uc.save_provider_configuration(france_travail(), bad, false, &c)
            .await
            .unwrap_err(),
    );

    let fields: Vec<&str> = found.iter().map(|(field, _)| field.as_str()).collect();
    assert!(fields.contains(&"client_id"));
    assert!(fields.contains(&"token_url"));
    assert!(fields.contains(&"nope"));
    assert!(
        uc.get_provider_configuration("france_travail")
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_a_successful_test_makes_a_provider_usable(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    uc.save_provider_configuration(
        france_travail(),
        command("client-1", Some(SecretChange::Replace(Secret::new(SECRET)))),
        false,
        &c,
    )
    .await
    .unwrap();
    assert!(!uc.is_provider_usable("france_travail").await.unwrap());

    uc.record_provider_status(
        "france_travail",
        ProviderStatus::Failing,
        Some("boom".to_owned()),
        Utc::now(),
    )
    .await
    .unwrap();
    assert!(!uc.is_provider_usable("france_travail").await.unwrap());

    let ready = uc
        .record_provider_status("france_travail", ProviderStatus::Ready, None, Utc::now())
        .await
        .unwrap();
    assert!(ready.last_test_at.is_some());
    assert!(uc.is_provider_usable("france_travail").await.unwrap());

    uc.save_provider_configuration(france_travail(), command("client-2", None), false, &c)
        .await
        .unwrap();
    assert!(!uc.is_provider_usable("france_travail").await.unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn failing_keeps_its_message_and_unknown_providers_are_not_usable(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    uc.save_provider_configuration(
        france_travail(),
        command("client-1", Some(SecretChange::Replace(Secret::new(SECRET)))),
        false,
        &c,
    )
    .await
    .unwrap();
    uc.record_provider_status(
        "france_travail",
        ProviderStatus::Failing,
        Some("refused".to_owned()),
        Utc::now(),
    )
    .await
    .unwrap();
    let stored = uc
        .get_provider_configuration("france_travail")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, ProviderStatus::Failing);
    assert_eq!(stored.last_test_message.as_deref(), Some("refused"));
    assert!(!uc.is_provider_usable("missing").await.unwrap());
    assert!(matches!(
        uc.record_provider_status("missing", ProviderStatus::Ready, None, Utc::now())
            .await
            .unwrap_err(),
        CoreError::NotFound
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn clearing_removes_the_configuration_and_its_secrets(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool.clone());
    let c = cipher(1);
    uc.save_provider_configuration(
        france_travail(),
        command("client-1", Some(SecretChange::Replace(Secret::new(SECRET)))),
        false,
        &c,
    )
    .await
    .unwrap();

    uc.clear_provider_configuration("france_travail")
        .await
        .unwrap();

    assert!(
        uc.get_provider_configuration("france_travail")
            .await
            .unwrap()
            .is_none()
    );
    let left: i64 = query_scalar("SELECT count(*) FROM source_provider_secrets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
    assert!(matches!(
        uc.clear_provider_configuration("france_travail")
            .await
            .unwrap_err(),
        CoreError::NotFound
    ));
}

async fn user(uc: &LodestoneUseCase) -> UserId {
    uc.create_user(CreateUserCommand {
        name: "Alice".to_owned(),
        username: "alice".to_owned(),
        email: None,
        sub: "subject-1".to_owned(),
    })
    .await
    .unwrap()
    .id
}

fn state(user: UserId, value: &str) -> NewOAuthState {
    NewOAuthState {
        state: value.to_owned(),
        provider_key: "france_travail".to_owned(),
        user_id: user,
        code_verifier: Secret::new("verifier-value"),
        redirect_uri: "https://api.example.com/callback".to_owned(),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_oauth_state_is_bound_to_its_user_single_use_and_stores_the_verifier_sealed(
    pool: PgPool,
) {
    let uc = LodestoneUseCase::new(pool.clone());
    let c = cipher(1);
    let user = user(&uc).await;
    let stored = uc
        .begin_oauth_state(state(user, "state-1"), &c)
        .await
        .unwrap();
    assert!(stored.expires_at > stored.created_at);
    assert_eq!((stored.expires_at - stored.created_at).num_seconds(), 600);
    let raw: Vec<u8> = query_scalar(
        "SELECT code_verifier_cipher FROM source_provider_oauth_states WHERE state = 'state-1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!raw.windows(14).any(|w| w == b"verifier-value"));

    let consumed = uc.consume_oauth_state("state-1", &c).await.unwrap();
    assert_eq!(consumed.user_id, user);
    assert_eq!(consumed.code_verifier.expose(), "verifier-value");
    assert!(matches!(
        uc.consume_oauth_state("state-1", &c).await.unwrap_err(),
        OAuthStateError::Unknown
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_expired_oauth_state_is_refused_and_purged(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool.clone());
    let c = cipher(1);
    let user = user(&uc).await;
    uc.begin_oauth_state(state(user, "old"), &c).await.unwrap();
    query("UPDATE source_provider_oauth_states SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();

    assert!(matches!(
        uc.consume_oauth_state("old", &c).await.unwrap_err(),
        OAuthStateError::Expired
    ));

    uc.begin_oauth_state(state(user, "stale"), &c)
        .await
        .unwrap();
    query("UPDATE source_provider_oauth_states SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    uc.begin_oauth_state(state(user, "fresh"), &c)
        .await
        .unwrap();
    let left: i64 = query_scalar("SELECT count(*) FROM source_provider_oauth_states")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn tokens_are_stored_sealed_and_cleared_on_disconnect(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    uc.save_provider_configuration(
        france_travail(),
        command("client-1", Some(SecretChange::Replace(Secret::new(SECRET)))),
        false,
        &c,
    )
    .await
    .unwrap();
    let account = ConnectedAccount {
        display_name: "Alice".to_owned(),
        account_id: Some("42".to_owned()),
        scopes: vec!["read".to_owned()],
        expires_at: None,
        connected_by: None,
    };

    let connected = uc
        .store_provider_tokens(
            "france_travail",
            TokenUpdate {
                access: Secret::new("access-1"),
                refresh: Some(Secret::new("refresh-1")),
            },
            account.clone(),
            &c,
        )
        .await
        .unwrap();
    assert_eq!(connected.connected_account, Some(account));
    assert_eq!(
        open_secrets(&connected, &c).unwrap()[TOKEN_ACCESS].expose(),
        "access-1"
    );

    let cleared = uc
        .clear_provider_tokens("france_travail", Some("bye".to_owned()))
        .await
        .unwrap();
    assert!(cleared.connected_account.is_none());
    assert!(!cleared.has_secret(TOKEN_ACCESS));
    assert!(cleared.has_secret("client_secret"));
    assert_eq!(cleared.status, ProviderStatus::Untested);
}

#[sqlx::test(migrations = "../../migrations")]
async fn changing_a_url_requires_every_secret_again(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    uc.save_provider_configuration(
        france_travail(),
        command("client-1", Some(SecretChange::Replace(Secret::new(SECRET)))),
        false,
        &c,
    )
    .await
    .unwrap();
    let mut moved = command("client-1", None);
    moved.settings.insert(
        "token_url".to_owned(),
        json!("https://evil.example.com/token"),
    );

    let error = uc
        .save_provider_configuration(france_travail(), moved.clone(), false, &c)
        .await
        .unwrap_err();

    assert_eq!(
        problems(error),
        vec![(
            "secrets.client_secret".to_owned(),
            "must be provided again when a URL setting changes".to_owned()
        )]
    );
    let stored = uc
        .get_provider_configuration("france_travail")
        .await
        .unwrap()
        .unwrap();
    assert_ne!(
        stored.settings["token_url"],
        "https://evil.example.com/token"
    );

    moved.secrets.insert(
        "client_secret".to_owned(),
        SecretChange::Replace(Secret::new("again")),
    );
    let saved = uc
        .save_provider_configuration(france_travail(), moved, false, &c)
        .await
        .unwrap();
    assert_eq!(
        saved.settings["token_url"],
        "https://evil.example.com/token"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn plain_http_urls_are_refused_unless_allowed(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let c = cipher(1);
    let mut insecure = command("client-1", Some(SecretChange::Replace(Secret::new(SECRET))));
    insecure
        .settings
        .insert("token_url".to_owned(), json!("http://example.com/token"));

    let error = uc
        .save_provider_configuration(france_travail(), insecure.clone(), false, &c)
        .await
        .unwrap_err();
    assert_eq!(
        problems(error),
        vec![("token_url".to_owned(), "must be an https URL".to_owned())]
    );
    assert!(
        uc.save_provider_configuration(france_travail(), insecure, true, &c)
            .await
            .is_ok()
    );
}
