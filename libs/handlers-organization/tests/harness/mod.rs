//! Test harness driving the real organization router over HTTP.
//!
//! The harness wires the **production adapters** and substitutes only their
//! external dependencies:
//!
//! - `FerrisKeyRepository` points at a local JWKS server, and tests present
//!   genuinely RS256-signed tokens. The real decode / key-lookup / expiry path
//!   runs on every request, so an authentication regression is visible here.
//! - `RedisRateLimiter` points at a throwaway Redis container.
//! - The repositories point at a throwaway Postgres with the migrations
//!   applied, so tenant isolation and uniqueness are exercised where they
//!   actually live — in the schema.
//!
//! No production code is aware of these tests: everything below is built from
//! the crates' existing public constructors.
//!
//! Requires Docker. On a runtime that does not expose `/var/run/docker.sock`
//! — OrbStack, Colima — export `DOCKER_HOST` first.

#![allow(dead_code)] // helpers land ahead of the sub-issues that consume them

use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use args::Args;
use auth::{AuthService, FerrisKeyRepository};
use axum::{Router, routing::get};
use chrono::Utc;
use clap::Parser;
use handlers::AppState;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use oxid_core::{OxidUseCase, default_authorizer};
use rate_limit::{Quota, RateLimitService, RedisRateLimiter};
use reqwest::{Client, RequestBuilder};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::{postgres::Postgres, redis::Redis};

/// Signing material for the fake realm. Duplicated from the `auth` crate's own
/// unit tests rather than exported from it: test fixtures do not belong in a
/// library's public surface, and this key signs nothing outside this file.
const TEST_PRIVATE_KEY_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDHF23PRKIxgKZC
8b8NihRuCh/PTx8bdX+x5Mp6WR44eVFNirC/j9mhmtK5vezS3CokPgpl0g1CVfBp
iZR1OEZs+Y0cFFJBZTWxuDiUz3jAQIqQlN7WH6dNsOu14FJS799Tv4yAC/wtx7ig
yLpncaQYJE5CkSOAGd+P7YBT2ONtOs0dR0+bdTbDkpu1MlIEsVMojzJFVuGGKtJp
cgLrYVZKACNop5y84tQUJx7vLW2JfdEZleFJ6k4g9DNnl/Y6njLTsKVtCKakORf3
wiqbk80IQxN7labaVQlXd1GooBC+7mBxwyXbFW35eM31GgQBPVaqBOHpBLu60knW
t7hC0x/xAgMBAAECggEAHjaCfg1K1dtRn+Ai37GgJxDXQfUeYeLjZYI0bfu/N8/F
VFCjQPbaDom5x+E4IsmxhX16w3fsdjAng0STKHTJTzlRvjyhPPZYfydXQtH3X6mL
vaQx6umz0Hj0VE3+AEMRr5pmfnoTI3lnHdNIYnFe9yDvVW/EJOkIQcXHjzHfVZBt
ofFGHL8NjJ008VEVwDtscaCq+ibfoEghvI9GMffd/HqZAYd9qhrz+wiT8ZQAFbp5
kTlP6YBUJ+mo2K7OkNdGPivgaxQhijwqc9d53eFMrmnETxliAHN1Alniud16o1j8
TpaIwF0Y+y6trmHrKXWaQkVRbPfYT2QTSmpTeLe1jwKBgQDzEevZNdjBJWvjvqaX
5n5F3ZPQD67XKghgokkNa+uKrIvHrzG4HDXrR7R24SHBxTmHGgw2k3WRfaFBnoHN
n7BoJNK+M8ddP3b0ea2kFpPAkWuWmOxv0VQykt721vfkHohBu5ra5eoXXd4Efnj5
PqX50JCVPT+k5Xl4R9dpbniziwKBgQDRrp4QZoiX3GXEmddqIn5ZwMrY/ia9Z8M0
da3I/+PCUFw23HEP0T6LskS8g64dG63hhrCy0BZN+WrJQu/m82cAJaRsQCbzilIt
K6/3NtXlu4SmXotGxEpn26X03j0YO1osKLFgd2FiT/0KiIQYj1/Ipyst3YghCIjR
zYm1KKx58wKBgBAV4oa4UoTNpisnJb0tqrOS60I8l3RzuqQyeSUjPC4sJv/q7x5g
94x/bUjksygwlhMDvUUrUv9y0eYWyD5EUBdEQJIHuSzJk2SwXLZcLCD1Pqpzqkno
D2tdXtX0+eilwJyg/ql3x5sOQjAH8peD9tXmYHsP15NhAD3eeznl7qTrAoGBAIXj
8pqWXnJaEcHQWnUzQWseaGjXIPWg5E0DN805WL4jgj6l1Kw8+KtLUgjuLKf5nLZ9
wybrKNLxiPaq/3WBxyuY3b0h2b15fa/KTbqWEU94xeNWS6kMflaDMx2BK5HllFbO
RTVMBas5WGL5eSAVrRv7Yt8OrnYpdPRDQsOjDT9xAoGBAMq7pYVEJBWoyFYWDnSY
LoQgUrpiRssRjaCMHOpEBxjtOTv3TzeyzHWD7+r2+y/qToJXcdA8jEyhaSeUa7mr
9e2VtIC/6Ouhmfb0+mwgwO/zQHR0sd/ruyNc7v4FBgYfZ/XqvYtzzTZzhNmvX9gQ
HUim3t4M1KMtX1QmMKKCg4i4
-----END PRIVATE KEY-----"#;

const TEST_KID: &str = "test-kid";
const TEST_N: &str = "xxdtz0SiMYCmQvG_DYoUbgofz08fG3V_seTKelkeOHlRTYqwv4_ZoZrSub3s0twqJD4KZdINQlXwaYmUdThGbPmNHBRSQWU1sbg4lM94wECKkJTe1h-nTbDrteBSUu_fU7-MgAv8Lce4oMi6Z3GkGCROQpEjgBnfj-2AU9jjbTrNHUdPm3U2w5KbtTJSBLFTKI8yRVbhhirSaXIC62FWSgAjaKecvOLUFCce7y1tiX3RGZXhSepOIPQzZ5f2Op4y07ClbQimpDkX98Iqm5PNCEMTe5Wm2lUJV3dRqKAQvu5gccMl2xVt-XjN9RoEAT1WqgTh6QS7utJJ1re4QtMf8Q";
const TEST_E: &str = "AQAB";

pub struct TestApi {
    _postgres: ContainerAsync<Postgres>,
    _redis: ContainerAsync<Redis>,
    issuer: String,
    jwks_hits: Arc<AtomicUsize>,
    base_url: String,
    client: Client,
    pub pool: PgPool,
}

impl TestApi {
    pub async fn start() -> Self {
        let (issuer, jwks_hits) = start_jwks_server().await;

        // Same image as docker-compose: the migrations call `gen_random_uuid()`,
        // built in only from PostgreSQL 13 onwards and declared nowhere.
        let postgres = Postgres::default()
            .with_tag("16-alpine")
            .start()
            .await
            .expect("start postgres container");
        let pg_port = postgres
            .get_host_port_ipv4(5432)
            .await
            .expect("mapped postgres port");
        let pg_host = postgres.get_host().await.expect("postgres host");

        let redis = Redis::default()
            .start()
            .await
            .expect("start redis container");
        let redis_port = redis
            .get_host_port_ipv4(6379)
            .await
            .expect("mapped redis port");
        let redis_host = redis.get_host().await.expect("redis host");

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&format!(
                "postgres://postgres:postgres@{pg_host}:{pg_port}/postgres"
            ))
            .await
            .expect("connect to postgres");

        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("run migrations");

        let limiter = RedisRateLimiter::connect(&format!("redis://{redis_host}:{redis_port}"))
            .await
            .expect("connect to redis");

        let state = AppState {
            // Secrets are required, so they must be supplied explicitly - which
            // is the point: a deployment that forgets them fails to start.
            args: Arc::new(Args::parse_from([
                "oxid",
                "--database-name",
                "postgres",
                "--database-password",
                "postgres",
                "--auth-client-secret",
                "test-secret",
            ])),
            auth: AuthService::new(FerrisKeyRepository::new(issuer.clone(), None)),
            usecase: OxidUseCase::new(pool.clone(), default_authorizer()),
            rate_limit: RateLimitService::new(limiter),
            // Well above what any single test issues, so throttling never masks
            // the behaviour under test.
            rate_limit_quota: Quota::per_minute(10_000),
        };

        let router = Router::new()
            .merge(handlers_organization::router(&state))
            // Mirrors apps/api: rate limiting wraps authentication.
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                handlers::rate_limit::rate_limit_middleware,
            ))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");

        tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .expect("serve test api");
        });

        Self {
            _postgres: postgres,
            _redis: redis,
            issuer,
            jwks_hits,
            base_url: format!("http://{addr}"),
            client: Client::new(),
            pool,
        }
    }

    /// Mints a token the production validator accepts: RS256, signed by the key
    /// the fake realm publishes, with a future expiry.
    ///
    /// `subject` is the OIDC subject. The handler layer currently parses it as
    /// an internal user id, so it has to be a UUID until that is separated.
    /// A token carrying realm roles, the way the provider sends them.
    pub fn token_with_realm_roles(&self, subject: &str, username: &str, roles: &[&str]) -> String {
        self.sign(json!({
            "sub": subject,
            "iss": self.issuer,
            "aud": "oxid",
            "exp": Utc::now().timestamp() + 3_600,
            "scope": "openid profile",
            "preferred_username": username,
            "realm_access": { "roles": roles },
        }))
    }

    /// A token from a realm that does not release the email claim at all.
    pub fn token_without_email(&self, subject: &str, username: &str) -> String {
        self.sign(json!({
            "sub": subject,
            "iss": self.issuer,
            "aud": "oxid",
            "exp": Utc::now().timestamp() + 3_600,
            "scope": "openid profile",
            "preferred_username": username,
        }))
    }

    pub fn token(&self, subject: &str, username: &str, email: &str) -> String {
        self.sign(json!({
            "sub": subject,
            "iss": self.issuer,
            "aud": "oxid",
            "exp": Utc::now().timestamp() + 3_600,
            "scope": "openid profile email",
            "preferred_username": username,
            "email": email,
            "email_verified": true,
        }))
    }

    fn sign(&self, claims: serde_json::Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(TEST_KID.to_owned());
        let key = EncodingKey::from_rsa_pem(TEST_PRIVATE_KEY_PEM.as_bytes())
            .expect("build rsa encoding key");
        encode(&header, &claims, &key).expect("encode jwt")
    }

    pub fn get(&self, path: &str) -> RequestBuilder {
        self.client.get(format!("{}{path}", self.base_url))
    }

    pub fn post(&self, path: &str) -> RequestBuilder {
        self.client.post(format!("{}{path}", self.base_url))
    }

    pub fn patch(&self, path: &str) -> RequestBuilder {
        self.client.patch(format!("{}{path}", self.base_url))
    }

    pub fn delete(&self, path: &str) -> RequestBuilder {
        self.client.delete(format!("{}{path}", self.base_url))
    }

    /// Serves the realm's JWKS at the path `FerrisKeyRepository` derives from the
    /// issuer. It must stay up for the whole test: the validator refetches the key
    /// set on every single token validation.
    /// How many times the realm's key set has been fetched. Every validation used
    /// to cause one; a cache should make that stop growing.
    pub fn jwks_fetches(&self) -> usize {
        self.jwks_hits.load(Ordering::SeqCst)
    }
}

async fn start_jwks_server() -> (String, Arc<AtomicUsize>) {
    let jwks = json!({
        "keys": [{ "kid": TEST_KID, "n": TEST_N, "e": TEST_E }]
    });

    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let router = Router::new().route(
        "/protocol/openid-connect/certs",
        get(move || {
            let jwks = jwks.clone();
            counter.fetch_add(1, Ordering::SeqCst);
            async move { axum::Json(jwks) }
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind jwks port");
    let addr = listener.local_addr().expect("jwks local addr");

    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve jwks");
    });

    (format!("http://{addr}"), hits)
}
