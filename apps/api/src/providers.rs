use std::{collections::BTreeMap, fmt::Display, sync::Arc, time::Duration};

use args::Args;
use common::{CoreError, Secret};
use fetch::PoliteFetcher;
use lodestone_core::{
    AuthSpec, Credential, FRANCE_TRAVAIL, HACKER_NEWS_HIRING, LodestoneUseCase, ProviderStatus,
    SourceError, catalog,
    infrastructure::crypto::AesGcmCipher,
    provider_descriptor, provider_key_of_source,
    source_providers::{SaveProviderConfiguration, SecretChange},
};
use serde_json::{Map, Value};
use source_providers::ProviderEngine;
use sources::france_travail::{AccessProvider, FranceTravailAccess};
use tracing::{debug, info, warn};
use url::Url;

pub const NOT_READY_RECHECK: Duration = Duration::from_secs(300);

pub async fn blocking_provider(
    usecase: &LodestoneUseCase,
    source: &str,
) -> Result<Option<&'static str>, CoreError> {
    let Some(key) = provider_key_of_source(source) else {
        return Ok(None);
    };
    Ok((!usecase.is_provider_usable(key).await?).then_some(key))
}

pub struct CatalogAccess {
    usecase: LodestoneUseCase,
    engine: Arc<ProviderEngine<PoliteFetcher>>,
    cipher: Arc<AesGcmCipher>,
}

impl CatalogAccess {
    pub fn new(
        usecase: LodestoneUseCase,
        engine: Arc<ProviderEngine<PoliteFetcher>>,
        cipher: Arc<AesGcmCipher>,
    ) -> Self {
        Self {
            usecase,
            engine,
            cipher,
        }
    }
}

fn unavailable(reason: impl Display) -> SourceError {
    SourceError::Upstream(format!("france travail access unavailable: {reason}"))
}

impl AccessProvider for CatalogAccess {
    async fn access(&self) -> Result<FranceTravailAccess, SourceError> {
        let configuration = self
            .usecase
            .get_provider_configuration(FRANCE_TRAVAIL)
            .await
            .map_err(unavailable)?
            .ok_or_else(|| unavailable("not configured in the catalog"))?;
        let descriptor = provider_descriptor(FRANCE_TRAVAIL)
            .ok_or_else(|| unavailable("missing from the catalog"))?;
        let credential = self
            .engine
            .credential(&self.usecase, self.cipher.as_ref(), &configuration)
            .await
            .map_err(unavailable)?;
        let Credential::Bearer(token) = credential else {
            return Err(unavailable("no bearer credential"));
        };
        let default = descriptor
            .field("api_base_url")
            .and_then(|field| field.default)
            .unwrap_or_default();
        let api_base_url =
            Url::parse(descriptor.endpoint(&configuration.settings, "api_base_url", default))
                .map_err(unavailable)?;
        Ok(FranceTravailAccess {
            token,
            api_base_url,
        })
    }

    fn invalidate(&self) {
        self.engine.forget(FRANCE_TRAVAIL);
    }
}

#[derive(Debug, Default)]
pub struct ProviderSeed {
    pub settings: BTreeMap<&'static str, Map<String, Value>>,
    pub secrets: BTreeMap<&'static str, BTreeMap<&'static str, Secret>>,
}

fn url_setting(name: &str, url: &Url) -> (String, Value) {
    (name.to_owned(), Value::String(url.to_string()))
}

impl ProviderSeed {
    pub fn from_args(args: &Args) -> Self {
        let mut seed = Self::default();
        seed.settings.insert(
            "boamp",
            Map::from_iter([url_setting("base_url", &args.boamp.base_url)]),
        );
        seed.settings.insert(
            "decp",
            Map::from_iter([url_setting("dataset_api_url", &args.decp.dataset_api_url)]),
        );
        seed.settings.insert(
            "ted",
            Map::from_iter([url_setting("base_url", &args.ted.base_url)]),
        );
        seed.settings.insert(
            HACKER_NEWS_HIRING,
            Map::from_iter([url_setting("base_url", &args.hacker_news.base_url)]),
        );
        seed.settings.insert(
            "certificate_transparency",
            Map::from_iter([url_setting("log_list_url", &args.ct.log_list_url)]),
        );
        seed.settings.insert(
            "company_research",
            Map::from_iter([url_setting("entreprises_url", &args.decp.entreprises_url)]),
        );
        if let Some((client_id, client_secret)) = args.france_travail.credentials() {
            seed.settings.insert(
                FRANCE_TRAVAIL,
                Map::from_iter([
                    (
                        "client_id".to_owned(),
                        Value::String(client_id.expose().to_owned()),
                    ),
                    url_setting("token_url", &args.france_travail.token_url),
                    url_setting("api_base_url", &args.france_travail.api_base_url),
                ]),
            );
            seed.secrets.insert(
                FRANCE_TRAVAIL,
                BTreeMap::from([("client_secret", client_secret.clone())]),
            );
        }
        seed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provisioned {
    pub key: &'static str,
    pub created: bool,
    pub status: ProviderStatus,
}

pub async fn provision_providers(
    usecase: &LodestoneUseCase,
    engine: &ProviderEngine<PoliteFetcher>,
    cipher: &AesGcmCipher,
    seed: &ProviderSeed,
) -> Result<Vec<Provisioned>, CoreError> {
    let mut report = Vec::new();
    for descriptor in catalog() {
        let key = descriptor.key;
        let public = matches!(descriptor.auth, AuthSpec::None);
        let seeded = seed.settings.contains_key(key) || seed.secrets.contains_key(key);
        let existing = usecase.get_provider_configuration(key).await?;
        let created = match &existing {
            None if public || seeded => {
                if !save_seed(usecase, engine, cipher, seed, key).await {
                    continue;
                }
                true
            }
            None => continue,
            Some(configuration) if public && !configuration.is_usable() => false,
            Some(_) => {
                if seeded {
                    debug!(
                        provider = key,
                        "provider already configured, the environment seed is ignored"
                    );
                }
                continue;
            }
        };
        match engine.test(usecase, cipher, key).await {
            Ok(tested) => {
                info!(
                    provider = key,
                    status = tested.status.as_str(),
                    created,
                    "provider tested at startup"
                );
                report.push(Provisioned {
                    key,
                    created,
                    status: tested.status,
                });
            }
            Err(error) => warn!(provider = key, %error, "provider could not be tested at startup"),
        }
    }
    Ok(report)
}

async fn save_seed(
    usecase: &LodestoneUseCase,
    engine: &ProviderEngine<PoliteFetcher>,
    cipher: &AesGcmCipher,
    seed: &ProviderSeed,
    key: &'static str,
) -> bool {
    let Some(descriptor) = provider_descriptor(key) else {
        return false;
    };
    let secrets: BTreeMap<String, SecretChange> = seed
        .secrets
        .get(key)
        .map(|secrets| {
            secrets
                .iter()
                .map(|(name, secret)| ((*name).to_owned(), SecretChange::Replace(secret.clone())))
                .collect()
        })
        .unwrap_or_default();
    let attempts = [seed.settings.get(key).cloned(), Some(Map::new())];
    for settings in attempts.into_iter().flatten() {
        let command = SaveProviderConfiguration {
            settings,
            secrets: secrets.clone(),
        };
        match usecase
            .save_provider_configuration(descriptor, command, engine.allows_http(), cipher)
            .await
        {
            Ok(_) => return true,
            Err(error) => {
                warn!(provider = key, %error, "provider configuration could not be created from the environment")
            }
        }
        if !secrets.is_empty() {
            break;
        }
    }
    false
}
