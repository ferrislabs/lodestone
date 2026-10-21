mod support;

use api::internal_router::internal_router;
use http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{Harness, KID, call, client_claims, generate_key, lazy_pool, now, sign, user_claims};
use telemetry::Metrics;

fn assert_rejected(status: StatusCode, body: &Value, raw: &str, code: &str, token: Option<&str>) {
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], code);
    assert_eq!(body["status"], 401);
    assert!(body["message"].is_string());
    assert!(body.get("details").is_none());
    if let Some(token) = token {
        assert!(!raw.contains(token), "response leaks the token");
        for part in token.split('.') {
            assert!(!raw.contains(part), "response leaks a token segment");
        }
    }
}

#[tokio::test]
async fn missing_authorization_header_is_rejected() {
    let harness = Harness::new(lazy_pool()).await;
    let (status, body, raw) = call(harness.public(), "/api/v1/me", None).await;
    assert_rejected(status, &body, &raw, "E_MISSING_AUTH_HEADER", None);
}

#[tokio::test]
async fn malformed_authorization_header_is_rejected() {
    let harness = Harness::new(lazy_pool()).await;
    let token = harness.valid_user_token();

    for header in [
        format!("Basic {token}"),
        token.clone(),
        format!("bearer {token}"),
    ] {
        let (status, body, raw) = call(harness.public(), "/api/v1/me", Some(&header)).await;
        assert_rejected(status, &body, &raw, "E_INVALID_AUTH_HEADER", Some(&token));
    }
}

#[tokio::test]
async fn garbage_bearer_token_is_rejected() {
    let harness = Harness::new(lazy_pool()).await;
    let (status, body, raw) = call(
        harness.public(),
        "/api/v1/me",
        Some("Bearer not-a-jwt-at-all"),
    )
    .await;
    assert_rejected(
        status,
        &body,
        &raw,
        "E_AUTHENTICATION_FAILED",
        Some("not-a-jwt-at-all"),
    );
}

#[tokio::test]
async fn token_signed_by_another_key_is_rejected() {
    let harness = Harness::new(lazy_pool()).await;
    let other = generate_key();
    let token = sign(
        &other,
        KID,
        &user_claims(&harness.server.base_url(), now() + 300),
    );

    let (status, body, raw) = call(
        harness.public(),
        "/api/v1/me",
        Some(&format!("Bearer {token}")),
    )
    .await;
    assert_rejected(status, &body, &raw, "E_AUTHENTICATION_FAILED", Some(&token));
}

#[tokio::test]
async fn token_with_unknown_key_id_is_rejected() {
    let harness = Harness::new(lazy_pool()).await;
    let token = sign(
        &harness.key,
        "rotated-away",
        &user_claims(&harness.server.base_url(), now() + 300),
    );

    let (status, body, raw) = call(
        harness.public(),
        "/api/v1/me",
        Some(&format!("Bearer {token}")),
    )
    .await;
    assert_rejected(status, &body, &raw, "E_AUTHENTICATION_FAILED", Some(&token));
}

#[tokio::test]
async fn expired_token_is_rejected() {
    let harness = Harness::new(lazy_pool()).await;
    let token = sign(
        &harness.key,
        KID,
        &user_claims(&harness.server.base_url(), now() - 300),
    );

    let (status, body, raw) = call(
        harness.public(),
        "/api/v1/me",
        Some(&format!("Bearer {token}")),
    )
    .await;
    assert_rejected(status, &body, &raw, "E_AUTHENTICATION_FAILED", Some(&token));
}

#[tokio::test]
async fn token_without_expiry_is_rejected() {
    let harness = Harness::new(lazy_pool()).await;
    let mut claims = user_claims(&harness.server.base_url(), 0);
    claims.as_object_mut().expect("object").remove("exp");
    let token = sign(&harness.key, KID, &claims);

    let (status, body, raw) = call(
        harness.public(),
        "/api/v1/me",
        Some(&format!("Bearer {token}")),
    )
    .await;
    assert_rejected(status, &body, &raw, "E_AUTHENTICATION_FAILED", Some(&token));
}

#[sqlx::test(migrations = "../../migrations")]
async fn valid_user_token_reaches_me(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let token = harness.valid_user_token();

    let (status, body, _) = call(
        harness.public(),
        "/api/v1/me",
        Some(&format!("Bearer {token}")),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["kind"], "user");
    assert_eq!(body["data"]["subject"], "subject-42");
    assert!(body["data"]["user_id"].is_string());
}

#[tokio::test]
async fn valid_client_token_reaches_me() {
    let harness = Harness::new(lazy_pool()).await;
    let token = sign(
        &harness.key,
        KID,
        &client_claims(&harness.server.base_url(), now() + 300),
    );

    let (status, body, _) = call(
        harness.public(),
        "/api/v1/me",
        Some(&format!("Bearer {token}")),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["kind"], "client");
    assert_eq!(body["data"]["client_id"], "lead-exporter");
}

#[tokio::test]
async fn health_and_metrics_are_not_on_the_public_router() {
    let harness = Harness::new(lazy_pool()).await;
    let token = harness.valid_user_token();
    let bearer = format!("Bearer {token}");

    for uri in ["/health", "/metrics"] {
        let (status, _, _) = call(harness.public(), uri, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} anonymous");
        let (status, _, _) = call(harness.public(), uri, Some(&bearer)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} authenticated");
    }
}

#[tokio::test]
async fn health_and_metrics_are_served_by_the_internal_router() {
    for uri in ["/health", "/metrics"] {
        let app = internal_router(Metrics::new(), lazy_pool()).expect("internal router");
        let (status, _, _) = call(app, uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
    }
}

#[tokio::test]
async fn business_api_is_not_on_the_internal_router() {
    let app = internal_router(Metrics::new(), lazy_pool()).expect("internal router");
    let (status, _, _) = call(app, "/api/v1/me", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn assert_claims_rejected(edit: impl FnOnce(&mut serde_json::Map<String, Value>)) {
    let harness = Harness::new(lazy_pool()).await;
    let mut claims = client_claims(&harness.server.base_url(), now() + 300);
    edit(claims.as_object_mut().expect("object"));
    let token = sign(&harness.key, KID, &claims);

    let (status, body, raw) = call(
        harness.public(),
        "/api/v1/me",
        Some(&format!("Bearer {token}")),
    )
    .await;
    assert_rejected(status, &body, &raw, "E_AUTHENTICATION_FAILED", Some(&token));
}

#[tokio::test]
async fn token_from_another_issuer_is_rejected() {
    assert_claims_rejected(|claims| {
        claims.insert(
            "iss".to_owned(),
            json!("https://auth.example.com/realms/other"),
        );
    })
    .await;
}

#[tokio::test]
async fn token_without_issuer_is_rejected() {
    assert_claims_rejected(|claims| {
        claims.remove("iss");
    })
    .await;
}

#[tokio::test]
async fn token_for_another_audience_is_rejected() {
    assert_claims_rejected(|claims| {
        claims.insert("aud".to_owned(), json!("another-api"));
    })
    .await;
}
