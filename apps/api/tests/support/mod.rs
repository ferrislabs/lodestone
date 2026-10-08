#![allow(dead_code)]

use std::{net::SocketAddr, num::NonZeroU32, sync::Arc, time::Duration};

use api::router::router;
use args::Args;
use auth::{AuthService, FerrisKeyRepository};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::ConnectInfo,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use clap::Parser;
use common::{EncryptionKey, Secret};
use fetch::{FetchSettings, PoliteFetcher};
use handlers::AppState;
use http::{
    Method, Request, StatusCode,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use httpmock::{Method::GET, MockServer};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use lodestone_core::{LodestoneUseCase, infrastructure::crypto::AesGcmCipher};
use rate_limit::{InMemoryRateLimiter, Quota, RateLimitService};
use rsa::{RsaPrivateKey, pkcs1::EncodeRsaPrivateKey, rand_core::OsRng, traits::PublicKeyParts};
use serde_json::{Value, json};
use source_providers::ProviderEngine;
use sqlx::PgPool;
use tower::ServiceExt;

pub const KID: &str = "lodestone-test-key";
pub const KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

pub struct TestKey {
    encoding: EncodingKey,
    n: String,
    e: String,
}

pub fn generate_key() -> TestKey {
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

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64
}

pub fn sign(key: &TestKey, kid: &str, claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_owned());
    encode(&header, claims, &key.encoding).expect("sign token")
}

pub fn user_claims(issuer: &str, exp: i64) -> Value {
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

pub fn client_claims(issuer: &str, exp: i64) -> Value {
    json!({
        "sub": "service-account-7",
        "iss": issuer,
        "aud": "lodestone",
        "exp": exp,
        "scope": "openid",
        "client_id": "lead-exporter",
    })
}

pub struct Harness {
    pub server: MockServer,
    pub key: TestKey,
    pub state: AppState,
}

impl Harness {
    pub async fn new(pool: PgPool) -> Self {
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
            KEY,
        ])
        .expect("parse default args");
        let repository = FerrisKeyRepository::new(server.base_url(), Some("lodestone".to_owned()));
        let enrichment_wait = args.enrichment.wait();
        let state = AppState {
            args: Arc::new(args),
            auth: AuthService::new(repository),
            usecase: LodestoneUseCase::new(pool).with_enrichment_wait(enrichment_wait),
            rate_limit: RateLimitService::new(InMemoryRateLimiter::new()),
            rate_limit_quota: Quota::per_minute(1000),
            providers: Arc::new(
                ProviderEngine::new(Arc::new(
                    PoliteFetcher::new(FetchSettings {
                        contact: "bot@example.com".to_owned(),
                        requests_per_host_per_minute: NonZeroU32::MIN.saturating_add(9_999),
                        request_timeout: Duration::from_secs(5),
                        max_body_bytes: 64 * 1024,
                        max_redirects: 3,
                        robots_ttl: Duration::from_secs(3600),
                        source_quotas: Vec::new(),
                        allow_private_targets: true,
                    })
                    .expect("fetcher"),
                ))
                .with_allow_http(true),
            ),
            cipher: Arc::new(
                AesGcmCipher::new(&EncryptionKey::from_base64(&Secret::new(KEY)).expect("key"))
                    .expect("cipher"),
            ),
        };

        Self { server, key, state }
    }

    pub fn public(&self) -> Router {
        router(self.state.clone()).expect("public router")
    }

    pub fn valid_client_token(&self) -> String {
        sign(
            &self.key,
            KID,
            &client_claims(&self.server.base_url(), now() + 300),
        )
    }

    pub fn valid_user_token(&self) -> String {
        sign(
            &self.key,
            KID,
            &user_claims(&self.server.base_url(), now() + 300),
        )
    }
}

pub fn lazy_pool() -> PgPool {
    PgPool::connect_lazy("postgres://unused:unused@127.0.0.1:1/unused").expect("lazy pool")
}

pub async fn call(
    app: Router,
    uri: &str,
    authorization: Option<&str>,
) -> (StatusCode, Value, String) {
    send(app, Method::GET, uri, authorization, None).await
}

pub async fn send(
    app: Router,
    method: Method,
    uri: &str,
    authorization: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value, String) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(value) = authorization {
        builder = builder.header(AUTHORIZATION, value);
    }
    let payload = match body {
        Some(json) => {
            builder = builder.header(CONTENT_TYPE, "application/json");
            Body::from(json.to_string())
        }
        None => Body::empty(),
    };
    let mut request = builder.body(payload).expect("request");
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

pub async fn send_raw(
    app: Router,
    method: Method,
    uri: &str,
    authorization: Option<&str>,
    body: Option<String>,
) -> (StatusCode, http::HeaderMap, String) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(value) = authorization {
        builder = builder.header(AUTHORIZATION, value);
    }
    let payload = match body {
        Some(text) => {
            builder = builder.header(CONTENT_TYPE, "application/json");
            Body::from(text)
        }
        None => Body::empty(),
    };
    let mut request = builder.body(payload).expect("request");
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));

    let response = app.oneshot(request).await.expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}
