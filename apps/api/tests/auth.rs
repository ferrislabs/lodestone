use std::{net::SocketAddr, sync::Arc};

use api::{internal_router::internal_router, router::router};
use args::Args;
use auth::{AuthService, FerrisKeyRepository};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::ConnectInfo,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use clap::Parser;
use handlers::AppState;
use http::{Request, StatusCode, header::AUTHORIZATION};
use httpmock::{Method::GET, MockServer};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use lodestone_core::LodestoneUseCase;
use rate_limit::{InMemoryRateLimiter, Quota, RateLimitService};
use rsa::{RsaPrivateKey, pkcs1::EncodeRsaPrivateKey, rand_core::OsRng, traits::PublicKeyParts};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const KID: &str = "lodestone-test-key";

struct TestKey {
    encoding: EncodingKey,
    n: String,
    e: String,
}

fn generate_key() -> TestKey {
    let private = RsaPrivateKey::new(&mut OsRng, 2048).expect("generate rsa key");
    let pem = private
        .to_pkcs1_pem(Default::default())
        .expect("encode pem");
    let public = private.to_public_key();
    TestKey {
        encoding: EncodingKey::from_rsa_pem(pem.as_bytes()).expect("encoding key"),
        n: URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
        e: URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64
}

fn sign(key: &TestKey, kid: &str, claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_owned());
    encode(&header, claims, &key.encoding).expect("sign token")
}

fn user_claims(issuer: &str, exp: i64) -> Value {
    json!({
        "sub": "subject-42",
        "iss": issuer,
        "aud": "lodestone",
        "exp": exp,
        "scope": "openid profile",
        "preferred_username": "alice",
        "name": "Alice",
        "email": "alice@example.com",
    })
}

fn client_claims(issuer: &str, exp: i64) -> Value {
    json!({
        "sub": "service-account-7",
        "iss": issuer,
        "aud": "lodestone",
        "exp": exp,
        "scope": "openid",
        "client_id": "lead-exporter",
    })
}

struct Harness {
    server: MockServer,
    key: TestKey,
    state: AppState,
}

impl Harness {
    async fn new(pool: PgPool) -> Self {
        let key = generate_key();
        let server = MockServer::start_async().await;
        let jwks = json!({ "keys": [{ "kid": KID, "n": key.n, "e": key.e }] });
        server
            .mock_async(|when, then| {
                when.method(GET).path("/protocol/openid-connect/certs");
                then.status(200)
                    .header("content-type", "application/json")
                    .body(jwks.to_string());
            })
            .await;

        let args = Args::try_parse_from([
            "api",
            "--auth-client-secret",
            "secret",
            "--database-name",
            "lodestone",
            "--outbound-contact",
            "security@example.com",
            "--llm-secrets-encryption-key",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        ])
        .expect("parse default args");
        let repository = FerrisKeyRepository::new(server.base_url(), Some("lodestone".to_owned()));
        let state = AppState {
            args: Arc::new(args),
            auth: AuthService::new(repository),
            usecase: LodestoneUseCase::new(pool),
            rate_limit: RateLimitService::new(InMemoryRateLimiter::new()),
            rate_limit_quota: Quota::per_minute(1000),
        };

        Self { server, key, state }
    }

    fn public(&self) -> Router {
        router(self.state.clone()).expect("public router")
    }

    fn valid_user_token(&self) -> String {
        sign(
            &self.key,
            KID,
            &user_claims(&self.server.base_url(), now() + 300),
        )
    }
}

fn lazy_pool() -> PgPool {
    PgPool::connect_lazy("postgres://unused:unused@127.0.0.1:1/unused").expect("lazy pool")
}

async fn call(app: Router, uri: &str, authorization: Option<&str>) -> (StatusCode, Value, String) {
    let mut builder = Request::builder().uri(uri);
    if let Some(value) = authorization {
        builder = builder.header(AUTHORIZATION, value);
    }
    let mut request = builder.body(Body::empty()).expect("request");
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));

    let response = app.oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let raw = String::from_utf8_lossy(&bytes).into_owned();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json, raw)
}

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
        let app = internal_router().expect("internal router");
        let (status, _, _) = call(app, uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
    }
}

#[tokio::test]
async fn business_api_is_not_on_the_internal_router() {
    let app = internal_router().expect("internal router");
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
