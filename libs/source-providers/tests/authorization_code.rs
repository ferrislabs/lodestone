mod support;

use std::collections::HashMap;

use aws_lc_rs::digest::{SHA256, digest};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use common::Secret;
use httpmock::Method::{GET, POST};
use lodestone_core::{
    AuthSpec, Capabilities, Credential, FieldSpec, FilterSupport, ProviderCategory, ProviderStatus,
    SealedSecret, SecretCipher, SourceProviderDescriptor, TOKEN_ACCESS, TOKEN_REFRESH,
    secret_context,
    source_providers::{SaveProviderConfiguration, SecretChange, open_secrets},
};
use serde_json::json;
use source_providers::{AuthFailure, CallbackFailure, EngineError};
use sqlx::{PgPool, Row, query, query_as, query_scalar};
use support::Rig;
use url::Url;
use uuid::Uuid;

const REDIRECT: &str = "https://lodestone.example.com/api/v1/source-providers/oauth/callback";
const CLIENT_SECRET: &str = "oauth-client-secret-0123456789";

static FIELDS: [FieldSpec; 7] = [
    FieldSpec::text("client_id", "Identifiant client", true),
    FieldSpec::secret("client_secret", "Secret client", false),
    FieldSpec::url("authorize_url", "Autorisation", false)
        .with_default("https://auth.example.com/authorize"),
    FieldSpec::url("token_url", "Jeton", false).with_default("https://auth.example.com/token"),
    FieldSpec::url("revoke_url", "Révocation", false)
        .with_default("https://auth.example.com/revoke"),
    FieldSpec::url("userinfo_url", "Profil", false)
        .with_default("https://auth.example.com/userinfo"),
    FieldSpec::url("api_base_url", "API", false).with_default("https://api.example.com"),
];

static CATALOG: [SourceProviderDescriptor; 1] = [SourceProviderDescriptor {
    key: "france_travail",
    name: "Test provider",
    description: "",
    category: ProviderCategory::JobsAndMissions,
    auth: AuthSpec::OAuth2AuthorizationCode {
        authorize_url: "https://auth.example.com/authorize",
        token_url: "https://auth.example.com/token",
        revoke_url: Some("https://auth.example.com/revoke"),
        userinfo_url: Some("https://auth.example.com/userinfo"),
        scopes: &["read", "offline"],
        pkce: true,
    },
    fields: &FIELDS,
    capabilities: Capabilities {
        keyword_search: false,
        filters: FilterSupport {
            remote: false,
            countries: false,
            contract: false,
        },
    },
    terms_note: None,
}];

async fn rig(pool: PgPool) -> Rig {
    let rig = Rig::with_catalog(pool, Some(&CATALOG)).await;
    rig.save(
        &CATALOG[0],
        json!({
            "client_id": "client-1",
            "authorize_url": rig.server.url("/authorize"),
            "token_url": rig.server.url("/token"),
            "revoke_url": rig.server.url("/revoke"),
            "userinfo_url": rig.server.url("/userinfo"),
            "api_base_url": rig.server.url("/api"),
        }),
        &[("client_secret", CLIENT_SECRET)],
    )
    .await;
    rig
}

fn params(url: &Url) -> HashMap<String, String> {
    url.query_pairs()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect()
}

async fn stored_verifier(rig: &Rig, state: &str) -> String {
    let row = query(
        "SELECT code_verifier_nonce, code_verifier_cipher FROM source_provider_oauth_states WHERE state = $1",
    )
    .bind(state)
    .fetch_one(&rig.pool)
    .await
    .unwrap();
    let sealed = SealedSecret {
        nonce: row.get("code_verifier_nonce"),
        ciphertext: row.get("code_verifier_cipher"),
    };
    rig.cipher
        .open(&sealed, &secret_context("france_travail", "oauth_state"))
        .unwrap()
        .expose()
        .to_owned()
}

async fn mock_provider(rig: &Rig) {
    rig.server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/userinfo")
                .header("authorization", "Bearer acc-1");
            then.status(200)
                .json_body(json!({"name": "Alice Martin", "id": 42}));
        })
        .await;
    rig.server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/api/offres/search")
                .header("authorization", "Bearer acc-1");
            then.status(200).body("{}");
        })
        .await;
}

async fn connect(rig: &Rig) {
    let user = rig.user().await;
    let url = rig
        .engine
        .start_oauth(&rig.uc, &rig.cipher, "france_travail", user, REDIRECT)
        .await
        .unwrap();
    let state = params(&url)["state"].clone();
    rig.server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/token")
                .body_contains("grant_type=authorization_code");
            then.status(200).json_body(json!({
                "access_token": "acc-1",
                "refresh_token": "ref-1",
                "expires_in": 3600,
                "scope": "read offline"
            }));
        })
        .await;
    mock_provider(rig).await;
    rig.engine
        .complete_oauth(&rig.uc, &rig.cipher, &state, "the-code")
        .await
        .unwrap();
}

async fn expire_access_token(rig: &Rig) {
    query(
        "UPDATE source_provider_configurations SET connected_account = jsonb_set(connected_account, '{expires_at}', to_jsonb(to_char(now() - interval '1 minute', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')))",
    )
    .execute(&rig.pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn start_builds_the_authorize_url_and_stores_the_state_and_pkce_verifier(pool: PgPool) {
    let rig = rig(pool).await;
    let user = rig.user().await;

    let url = rig
        .engine
        .start_oauth(&rig.uc, &rig.cipher, "france_travail", user, REDIRECT)
        .await
        .unwrap();

    assert_eq!(url.path(), "/authorize");
    let params = params(&url);
    assert_eq!(params["response_type"], "code");
    assert_eq!(params["client_id"], "client-1");
    assert_eq!(params["redirect_uri"], REDIRECT);
    assert_eq!(params["scope"], "read offline");
    assert_eq!(params["code_challenge_method"], "S256");
    assert!(params["state"].len() >= 43);
    assert!(!url.as_str().contains(CLIENT_SECRET));

    let verifier = stored_verifier(&rig, &params["state"]).await;
    assert_eq!(
        URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes())),
        params["code_challenge"]
    );
    let bound: (Uuid, i64) = query_as(
        "SELECT user_id, extract(epoch FROM (expires_at - created_at))::bigint FROM source_provider_oauth_states",
    )
    .fetch_one(&rig.pool)
    .await
    .unwrap();
    assert_eq!(bound.0, user.0);
    assert_eq!(bound.1, 600);
}

#[sqlx::test(migrations = "../../migrations")]
async fn start_requires_a_configured_authorization_code_provider(pool: PgPool) {
    let rig = Rig::with_catalog(pool, Some(&CATALOG)).await;
    let user = rig.user().await;
    let error = rig
        .engine
        .start_oauth(&rig.uc, &rig.cipher, "france_travail", user, REDIRECT)
        .await
        .unwrap_err();
    assert!(matches!(error, EngineError::NotConfigured));

    let other = Rig::new(rig.pool.clone()).await;
    let wrong = other
        .engine
        .start_oauth(&other.uc, &other.cipher, "boamp", user, REDIRECT)
        .await
        .unwrap_err();
    assert!(matches!(wrong, EngineError::WrongAuthMode));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_callback_exchanges_the_code_and_stores_sealed_tokens_and_the_account(pool: PgPool) {
    let rig = rig(pool).await;
    let user = rig.user().await;
    let url = rig
        .engine
        .start_oauth(&rig.uc, &rig.cipher, "france_travail", user, REDIRECT)
        .await
        .unwrap();
    let state = params(&url)["state"].clone();
    let verifier = stored_verifier(&rig, &state).await;
    let token = rig
        .server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/token")
                .body_contains("grant_type=authorization_code")
                .body_contains("code=the-code")
                .body_contains(format!("code_verifier={verifier}"))
                .body_contains(format!("client_secret={CLIENT_SECRET}"))
                .body_contains("client_id=client-1");
            then.status(200).json_body(json!({
                "access_token": "acc-1",
                "refresh_token": "ref-1",
                "expires_in": 3600,
                "scope": "read offline"
            }));
        })
        .await;
    mock_provider(&rig).await;

    let key = rig
        .engine
        .complete_oauth(&rig.uc, &rig.cipher, &state, "the-code")
        .await
        .unwrap();

    assert_eq!(key, "france_travail");
    token.assert_async().await;
    let stored = rig
        .uc
        .get_provider_configuration("france_travail")
        .await
        .unwrap()
        .unwrap();
    let account = stored.connected_account.clone().unwrap();
    assert_eq!(account.display_name, "Alice Martin");
    assert_eq!(account.account_id.as_deref(), Some("42"));
    assert_eq!(account.scopes, vec!["read", "offline"]);
    assert_eq!(account.connected_by, Some(user.to_string()));
    assert!(account.expires_at.is_some());
    assert_eq!(stored.status, ProviderStatus::Ready);
    let opened = open_secrets(&stored, &rig.cipher).unwrap();
    assert_eq!(opened[TOKEN_ACCESS].expose(), "acc-1");
    assert_eq!(opened[TOKEN_REFRESH].expose(), "ref-1");
    let raw: Vec<u8> = query_scalar(
        "SELECT ciphertext FROM source_provider_secrets WHERE name = 'oauth_access_token'",
    )
    .fetch_one(&rig.pool)
    .await
    .unwrap();
    assert!(!raw.windows(5).any(|window| window == b"acc-1"));
    assert!(!format!("{stored:?}").contains("acc-1"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unknown_state_is_refused_without_calling_the_provider(pool: PgPool) {
    let rig = rig(pool).await;
    let token = rig
        .server
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(200).json_body(json!({"access_token": "x"}));
        })
        .await;

    let error = rig
        .engine
        .complete_oauth(&rig.uc, &rig.cipher, "forged", "code")
        .await
        .unwrap_err();

    assert_eq!(error.failure, CallbackFailure::InvalidState);
    assert_eq!(error.provider_key, None);
    token.assert_hits_async(0).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_state_cannot_be_replayed(pool: PgPool) {
    let rig = rig(pool).await;
    let user = rig.user().await;
    let url = rig
        .engine
        .start_oauth(&rig.uc, &rig.cipher, "france_travail", user, REDIRECT)
        .await
        .unwrap();
    let state = params(&url)["state"].clone();
    rig.server
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .json_body(json!({"access_token": "acc-1", "expires_in": 60}));
        })
        .await;
    mock_provider(&rig).await;
    rig.engine
        .complete_oauth(&rig.uc, &rig.cipher, &state, "code")
        .await
        .unwrap();

    let replay = rig
        .engine
        .complete_oauth(&rig.uc, &rig.cipher, &state, "code")
        .await
        .unwrap_err();

    assert_eq!(replay.failure, CallbackFailure::InvalidState);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_expired_state_is_refused(pool: PgPool) {
    let rig = rig(pool).await;
    let user = rig.user().await;
    let url = rig
        .engine
        .start_oauth(&rig.uc, &rig.cipher, "france_travail", user, REDIRECT)
        .await
        .unwrap();
    let state = params(&url)["state"].clone();
    query("UPDATE source_provider_oauth_states SET expires_at = now() - interval '1 second'")
        .execute(&rig.pool)
        .await
        .unwrap();
    let token = rig
        .server
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(200).json_body(json!({"access_token": "x"}));
        })
        .await;

    let error = rig
        .engine
        .complete_oauth(&rig.uc, &rig.cipher, &state, "code")
        .await
        .unwrap_err();

    assert_eq!(error.failure, CallbackFailure::Expired);
    token.assert_hits_async(0).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_refused_code_exchange_reports_the_provider_and_stores_nothing(pool: PgPool) {
    let rig = rig(pool).await;
    let user = rig.user().await;
    let url = rig
        .engine
        .start_oauth(&rig.uc, &rig.cipher, "france_travail", user, REDIRECT)
        .await
        .unwrap();
    let state = params(&url)["state"].clone();
    rig.server
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(400)
                .json_body(json!({"error": "invalid_grant"}));
        })
        .await;

    let error = rig
        .engine
        .complete_oauth(&rig.uc, &rig.cipher, &state, "bad")
        .await
        .unwrap_err();

    assert_eq!(error.failure, CallbackFailure::ExchangeFailed);
    assert_eq!(error.provider_key.as_deref(), Some("france_travail"));
    let stored = rig
        .uc
        .get_provider_configuration("france_travail")
        .await
        .unwrap()
        .unwrap();
    assert!(stored.connected_account.is_none());
    assert!(!stored.has_secret(TOKEN_ACCESS));
}

#[sqlx::test(migrations = "../../migrations")]
async fn testing_without_a_connected_account_leaves_the_provider_untested(pool: PgPool) {
    let rig = rig(pool).await;

    let tested = rig
        .engine
        .test(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();

    assert_eq!(tested.status, ProviderStatus::Untested);
    assert!(tested.last_test_message.is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_expired_access_token_is_refreshed_and_stored(pool: PgPool) {
    let rig = rig(pool).await;
    connect(&rig).await;
    expire_access_token(&rig).await;
    let refresh = rig
        .server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/token")
                .body_contains("grant_type=refresh_token")
                .body_contains("refresh_token=ref-1");
            then.status(200)
                .json_body(json!({"access_token": "acc-2", "expires_in": 3600}));
        })
        .await;

    let credential = rig
        .engine
        .credential_for(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();
    match credential {
        Credential::Bearer(token) => assert_eq!(token.expose(), "acc-2"),
        other => panic!("expected a bearer, got {other:?}"),
    }
    rig.engine
        .credential_for(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();

    refresh.assert_hits_async(1).await;
    let stored = rig
        .uc
        .get_provider_configuration("france_travail")
        .await
        .unwrap()
        .unwrap();
    let opened = open_secrets(&stored, &rig.cipher).unwrap();
    assert_eq!(opened[TOKEN_ACCESS].expose(), "acc-2");
    assert_eq!(opened[TOKEN_REFRESH].expose(), "ref-1");
    assert_eq!(
        stored.connected_account.unwrap().display_name,
        "Alice Martin"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_revoked_refresh_token_marks_the_provider_failing(pool: PgPool) {
    let rig = rig(pool).await;
    connect(&rig).await;
    expire_access_token(&rig).await;
    rig.server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/token")
                .body_contains("grant_type=refresh_token");
            then.status(400)
                .json_body(json!({"error": "invalid_grant"}));
        })
        .await;

    let error = rig
        .engine
        .credential_for(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        EngineError::Auth(AuthFailure::Rejected(400))
    ));
    let stored = rig
        .uc
        .get_provider_configuration("france_travail")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, ProviderStatus::Failing);
    assert!(stored.last_test_message.is_some());
    assert!(!rig.uc.is_provider_usable("france_travail").await.unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn disconnect_revokes_the_token_then_clears_the_account(pool: PgPool) {
    let rig = rig(pool).await;
    connect(&rig).await;
    let revoke = rig
        .server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/revoke")
                .body_contains("token=ref-1")
                .body_contains("token_type_hint=refresh_token")
                .body_contains("client_id=client-1");
            then.status(200);
        })
        .await;

    let cleared = rig
        .engine
        .disconnect(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();

    revoke.assert_async().await;
    assert!(cleared.connected_account.is_none());
    assert!(!cleared.has_secret(TOKEN_ACCESS));
    assert!(!cleared.has_secret(TOKEN_REFRESH));
    assert!(cleared.has_secret("client_secret"));
    assert_eq!(cleared.status, ProviderStatus::Untested);
    assert!(!rig.uc.is_provider_usable("france_travail").await.unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn disconnect_still_clears_when_the_revocation_fails(pool: PgPool) {
    let rig = rig(pool).await;
    connect(&rig).await;
    rig.server
        .mock_async(|when, then| {
            when.method(POST).path("/revoke");
            then.status(500);
        })
        .await;

    let cleared = rig
        .engine
        .disconnect(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();

    assert!(cleared.connected_account.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn changing_a_url_clears_the_tokens_and_the_connected_account(pool: PgPool) {
    let rig = rig(pool).await;
    connect(&rig).await;
    let saved = rig
        .uc
        .save_provider_configuration(
            &CATALOG[0],
            SaveProviderConfiguration {
                settings: support::object(json!({
                    "client_id": "client-1",
                    "authorize_url": rig.server.url("/other-authorize"),
                    "token_url": rig.server.url("/token"),
                    "revoke_url": rig.server.url("/revoke"),
                    "userinfo_url": rig.server.url("/userinfo"),
                    "api_base_url": rig.server.url("/api"),
                })),
                secrets: [(
                    "client_secret".to_owned(),
                    SecretChange::Replace(Secret::new(CLIENT_SECRET)),
                )]
                .into(),
            },
            true,
            &rig.cipher,
        )
        .await
        .unwrap();

    assert!(saved.connected_account.is_none());
    assert!(!saved.has_secret(TOKEN_ACCESS));
    assert!(!saved.has_secret(TOKEN_REFRESH));
    assert!(saved.has_secret("client_secret"));
    assert_eq!(saved.status, ProviderStatus::Untested);
}
