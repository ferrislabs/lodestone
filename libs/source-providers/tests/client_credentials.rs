mod support;

use std::time::Duration;

use httpmock::Method::{GET, POST};
use lodestone_core::{Credential, ProviderStatus, provider_descriptor};
use serde_json::{Value, json};
use source_providers::{AuthFailure, EngineError};
use sqlx::PgPool;
use support::Rig;
use tokio::{join, time::sleep};

const SECRET: &str = "ft-client-secret-0123456789";

fn settings(rig: &Rig) -> Value {
    json!({
        "client_id": "client-1",
        "token_url": rig.server.url("/token"),
        "api_base_url": rig.server.url("/api"),
    })
}

fn bearer(credential: Credential) -> String {
    match credential {
        Credential::Bearer(token) => token.expose().to_owned(),
        other => panic!("expected a bearer credential, got {other:?}"),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_valid_service_account_is_tested_and_becomes_ready(pool: PgPool) {
    let rig = Rig::new(pool).await;
    let descriptor = provider_descriptor("france_travail").unwrap();
    rig.save(descriptor, settings(&rig), &[("client_secret", SECRET)])
        .await;
    let token = rig
        .server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/token")
                .header("content-type", "application/x-www-form-urlencoded")
                .body_contains("grant_type=client_credentials")
                .body_contains("client_id=client-1")
                .body_contains(format!("client_secret={SECRET}"))
                .body_contains("scope=api_offresdemploiv2+o2dsoffre");
            then.status(200)
                .json_body(json!({"access_token": "tok-1", "expires_in": 1499}));
        })
        .await;
    let api = rig
        .server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/api/offres/search")
                .query_param("range", "0-0")
                .header("authorization", "Bearer tok-1");
            then.status(206).body("{}");
        })
        .await;

    let tested = rig
        .engine
        .test(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();

    assert_eq!(tested.status, ProviderStatus::Ready);
    assert!(tested.last_test_at.is_some());
    assert!(tested.last_test_message.is_some());
    assert!(rig.uc.is_provider_usable("france_travail").await.unwrap());
    token.assert_async().await;
    api.assert_async().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_rejected_token_request_fails_the_test_without_leaking_secrets(pool: PgPool) {
    let rig = Rig::new(pool).await;
    let descriptor = provider_descriptor("france_travail").unwrap();
    rig.save(descriptor, settings(&rig), &[("client_secret", SECRET)])
        .await;
    rig.server
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(401)
                .json_body(json!({"error": "invalid_client", "echo": SECRET}));
        })
        .await;

    let tested = rig
        .engine
        .test(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();

    assert_eq!(tested.status, ProviderStatus::Failing);
    let message = tested.last_test_message.unwrap();
    assert!(message.contains("401"), "{message}");
    assert!(!message.contains(SECRET));
    assert!(!rig.uc.is_provider_usable("france_travail").await.unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_api_refusal_after_a_valid_token_fails_the_test(pool: PgPool) {
    let rig = Rig::new(pool).await;
    let descriptor = provider_descriptor("france_travail").unwrap();
    rig.save(descriptor, settings(&rig), &[("client_secret", SECRET)])
        .await;
    rig.server
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .json_body(json!({"access_token": "tok-1", "expires_in": 600}));
        })
        .await;
    rig.server
        .mock_async(|when, then| {
            when.method(GET).path("/api/offres/search");
            then.status(403);
        })
        .await;

    let tested = rig
        .engine
        .test(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();

    assert_eq!(tested.status, ProviderStatus::Failing);
    assert!(tested.last_test_message.unwrap().contains("403"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failing_provider_recovers_when_the_next_test_succeeds(pool: PgPool) {
    let rig = Rig::new(pool).await;
    let descriptor = provider_descriptor("france_travail").unwrap();
    rig.save(descriptor, settings(&rig), &[("client_secret", SECRET)])
        .await;
    let refusal = rig
        .server
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(401);
        })
        .await;
    let failed = rig
        .engine
        .test(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();
    assert_eq!(failed.status, ProviderStatus::Failing);
    refusal.delete_async().await;

    rig.server
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .json_body(json!({"access_token": "tok-1", "expires_in": 600}));
        })
        .await;
    rig.server
        .mock_async(|when, then| {
            when.method(GET).path("/api/offres/search");
            then.status(200).body("{}");
        })
        .await;
    let recovered = rig
        .engine
        .test(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();
    assert_eq!(recovered.status, ProviderStatus::Ready);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_token_is_cached_until_the_expiry_margin_then_renewed(pool: PgPool) {
    let rig = Rig::new(pool).await;
    let descriptor = provider_descriptor("france_travail").unwrap();
    rig.save(descriptor, settings(&rig), &[("client_secret", SECRET)])
        .await;
    let token = rig
        .server
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .json_body(json!({"access_token": "tok-1", "expires_in": 120}));
        })
        .await;

    let first = bearer(
        rig.engine
            .credential_for(&rig.uc, &rig.cipher, "france_travail")
            .await
            .unwrap(),
    );
    rig.clock.advance(Duration::from_secs(30));
    let second = bearer(
        rig.engine
            .credential_for(&rig.uc, &rig.cipher, "france_travail")
            .await
            .unwrap(),
    );
    assert_eq!((first.as_str(), second.as_str()), ("tok-1", "tok-1"));
    token.assert_hits_async(1).await;

    rig.clock.advance(Duration::from_secs(40));
    rig.engine
        .credential_for(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();
    token.assert_hits_async(2).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn saving_again_forgets_the_cached_token(pool: PgPool) {
    let rig = Rig::new(pool).await;
    let descriptor = provider_descriptor("france_travail").unwrap();
    rig.save(descriptor, settings(&rig), &[("client_secret", SECRET)])
        .await;
    let token = rig
        .server
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .json_body(json!({"access_token": "tok-1", "expires_in": 1000}));
        })
        .await;
    rig.engine
        .credential_for(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();

    rig.engine.forget("france_travail");
    rig.engine
        .credential_for(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();

    token.assert_hits_async(2).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_token_endpoint_error_surfaces_as_an_auth_failure(pool: PgPool) {
    let rig = Rig::new(pool).await;
    let descriptor = provider_descriptor("france_travail").unwrap();
    rig.save(descriptor, settings(&rig), &[("client_secret", SECRET)])
        .await;
    rig.server
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(401);
        })
        .await;

    let error = rig
        .engine
        .credential_for(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        EngineError::Auth(AuthFailure::Rejected(401))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn testing_an_unconfigured_provider_is_an_error(pool: PgPool) {
    let rig = Rig::new(pool).await;

    let error = rig
        .engine
        .test(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap_err();

    assert!(matches!(error, EngineError::NotConfigured));
    let unknown = rig
        .engine
        .test(&rig.uc, &rig.cipher, "nope")
        .await
        .unwrap_err();
    assert!(matches!(unknown, EngineError::UnknownProvider));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_token_minted_before_a_configuration_change_is_never_served_after_it(pool: PgPool) {
    let rig = Rig::new(pool).await;
    let descriptor = provider_descriptor("france_travail").unwrap();
    rig.save(descriptor, settings(&rig), &[("client_secret", SECRET)])
        .await;
    let token = rig
        .server
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .delay(Duration::from_millis(500))
                .json_body(json!({"access_token": "stale", "expires_in": 1000}));
        })
        .await;

    let (_, ()) = join!(
        async {
            rig.engine
                .credential_for(&rig.uc, &rig.cipher, "france_travail")
                .await
                .unwrap();
        },
        async {
            sleep(Duration::from_millis(150)).await;
            rig.engine.forget("france_travail");
        }
    );
    token.assert_hits_async(1).await;

    rig.engine
        .credential_for(&rig.uc, &rig.cipher, "france_travail")
        .await
        .unwrap();
    token.assert_hits_async(2).await;
}
