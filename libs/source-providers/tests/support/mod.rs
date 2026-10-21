#![allow(dead_code)]

use std::{
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use common::{EncryptionKey, Secret};
use fetch::{FetchSettings, PoliteFetcher};
use httpmock::MockServer;
use lodestone_core::{
    CreateUserCommand, LodestoneUseCase, SourceProviderDescriptor, UserId,
    infrastructure::crypto::AesGcmCipher,
    source_providers::{SaveProviderConfiguration, SecretChange},
};
use rate_limit::Clock;
use serde_json::{Map, Value};
use source_providers::ProviderEngine;
use sqlx::PgPool;

#[derive(Clone)]
pub struct FakeClock {
    base: Instant,
    offset: Arc<Mutex<Duration>>,
}

impl FakeClock {
    pub fn new() -> Self {
        Self {
            base: Instant::now(),
            offset: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }

    pub fn advance(&self, by: Duration) {
        if let Ok(mut offset) = self.offset.lock() {
            *offset += by;
        }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        let offset = self.offset.lock().map(|offset| *offset).unwrap_or_default();
        self.base + offset
    }
}

pub fn cipher() -> AesGcmCipher {
    let encoded = format!("{}QUE=", "QUFB".repeat(10));
    AesGcmCipher::new(&EncryptionKey::from_base64(&Secret::new(encoded)).unwrap()).unwrap()
}

pub fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

pub struct Rig {
    pub server: MockServer,
    pub uc: LodestoneUseCase,
    pub cipher: AesGcmCipher,
    pub engine: ProviderEngine<PoliteFetcher, FakeClock>,
    pub clock: FakeClock,
    pub pool: PgPool,
}

impl Rig {
    pub async fn new(pool: PgPool) -> Self {
        Self::with_catalog(pool, None).await
    }

    pub async fn with_catalog(
        pool: PgPool,
        catalog: Option<&'static [SourceProviderDescriptor]>,
    ) -> Self {
        let fetcher = PoliteFetcher::new(FetchSettings {
            contact: "bot@example.com".to_owned(),
            requests_per_host_per_minute: NonZeroU32::new(10_000).unwrap(),
            request_timeout: Duration::from_secs(5),
            max_body_bytes: 64 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(3600),
            source_quotas: Vec::new(),
            allow_private_targets: true,
        })
        .unwrap();
        let clock = FakeClock::new();
        let mut engine = ProviderEngine::with_clock(Arc::new(fetcher), clock.clone())
            .with_margin(Duration::from_secs(60))
            .with_allow_http(true);
        if let Some(catalog) = catalog {
            engine = engine.with_catalog(catalog);
        }
        Self {
            server: MockServer::start_async().await,
            uc: LodestoneUseCase::new(pool.clone()),
            cipher: cipher(),
            engine,
            clock,
            pool,
        }
    }

    pub async fn save(
        &self,
        descriptor: &SourceProviderDescriptor,
        settings: Value,
        secrets: &[(&str, &str)],
    ) {
        self.uc
            .save_provider_configuration(
                descriptor,
                SaveProviderConfiguration {
                    settings: object(settings),
                    secrets: secrets
                        .iter()
                        .map(|(name, value)| {
                            (
                                (*name).to_owned(),
                                SecretChange::Replace(Secret::new(*value)),
                            )
                        })
                        .collect(),
                },
                true,
                &self.cipher,
            )
            .await
            .unwrap();
    }

    pub async fn user(&self) -> UserId {
        self.uc
            .create_user(CreateUserCommand {
                name: "Alice".to_owned(),
                username: "alice".to_owned(),
                email: None,
                sub: "subject-1".to_owned(),
            })
            .await
            .unwrap()
            .id
    }
}
