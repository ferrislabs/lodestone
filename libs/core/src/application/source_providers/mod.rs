use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use common::{CoreError, Secret};
use lodestone_macros::transactional;
use serde_json::{Map, Value};
use thiserror::Error;

use crate::{
    UserId,
    application::LodestoneUseCase,
    domain::{
        llm::provider::{CipherError, SealedSecret, SecretCipher},
        source_provider::{
            AuthSpec, ConnectedAccount, FieldKind, FieldProblem, OAUTH_STATE_TTL, OAuthState,
            ProviderConfiguration, ProviderStatus, SourceProviderDescriptor,
            SourceProviderRepository, TOKEN_ACCESS, TOKEN_REFRESH, missing_secrets, secret_context,
            stamp_attestations, validate_settings,
        },
    },
};

#[derive(Debug, Clone)]
pub enum SecretChange {
    Replace(Secret),
    Remove,
}

#[derive(Debug, Clone)]
pub struct SaveProviderConfiguration {
    pub settings: Map<String, Value>,
    pub secrets: BTreeMap<String, SecretChange>,
}

#[derive(Debug, Error)]
pub enum ProviderConfigError {
    #[error("unknown source provider")]
    UnknownProvider,
    #[error("the configuration is invalid")]
    Invalid(Vec<FieldProblem>),
    #[error("a secret could not be sealed")]
    Cipher,
    #[error(transparent)]
    Core(#[from] CoreError),
}

#[derive(Debug, Error)]
pub enum OAuthStateError {
    #[error("unknown or already used authorisation state")]
    Unknown,
    #[error("the authorisation state has expired")]
    Expired,
    #[error("the code verifier could not be opened")]
    Cipher,
    #[error(transparent)]
    Core(#[from] CoreError),
}

#[derive(Debug, Clone)]
pub struct NewOAuthState {
    pub state: String,
    pub provider_key: String,
    pub user_id: UserId,
    pub code_verifier: Secret,
    pub redirect_uri: String,
}

#[derive(Debug, Clone)]
pub struct ConsumedOAuthState {
    pub provider_key: String,
    pub user_id: UserId,
    pub code_verifier: Secret,
    pub redirect_uri: String,
}

#[derive(Debug, Clone)]
pub struct TokenUpdate {
    pub access: Secret,
    pub refresh: Option<Secret>,
}

enum Merged {
    Saved(Box<ProviderConfiguration>),
    Missing(Vec<FieldProblem>),
}

pub fn open_secrets<C: SecretCipher>(
    configuration: &ProviderConfiguration,
    cipher: &C,
) -> Result<BTreeMap<String, Secret>, CipherError> {
    configuration
        .secrets
        .iter()
        .map(|(name, sealed)| {
            cipher
                .open(sealed, &secret_context(&configuration.key, name))
                .map(|secret| (name.clone(), secret))
        })
        .collect()
}

fn seal<C: SecretCipher>(
    cipher: &C,
    key: &str,
    name: &str,
    secret: &Secret,
) -> Result<SealedSecret, CoreError> {
    cipher
        .seal(secret, &secret_context(key, name))
        .map_err(|_| CoreError::Internal("provider secret could not be sealed".to_owned()))
}

impl LodestoneUseCase {
    #[transactional(source_provider)]
    pub async fn get_provider_configuration(
        &self,
        key: &str,
    ) -> Result<Option<ProviderConfiguration>, CoreError> {
        let mut repo = source_provider_repository;
        repo.find(key).await
    }

    #[transactional(source_provider)]
    pub async fn list_provider_configurations(
        &self,
    ) -> Result<Vec<ProviderConfiguration>, CoreError> {
        let mut repo = source_provider_repository;
        repo.list().await
    }

    pub async fn is_provider_usable(&self, key: &str) -> Result<bool, CoreError> {
        Ok(self
            .get_provider_configuration(key)
            .await?
            .is_some_and(|configuration| configuration.is_usable()))
    }

    pub async fn save_provider_configuration<C: SecretCipher>(
        &self,
        descriptor: &SourceProviderDescriptor,
        command: SaveProviderConfiguration,
        allow_http: bool,
        cipher: &C,
    ) -> Result<ProviderConfiguration, ProviderConfigError> {
        let mut problems = Vec::new();
        let settings = match validate_settings(descriptor, &command.settings, allow_http) {
            Ok(settings) => settings,
            Err(found) => {
                problems.extend(found);
                Map::new()
            }
        };
        for name in command.secrets.keys() {
            if !descriptor
                .field(name)
                .is_some_and(|field| field.is_secret())
            {
                problems.push(FieldProblem {
                    field: name.clone(),
                    message: "is not a secret field of this provider".to_owned(),
                });
            }
        }
        if !problems.is_empty() {
            let stored = self.get_provider_configuration(descriptor.key).await?;
            let mut present: BTreeSet<&str> = stored
                .as_ref()
                .map(|stored| stored.secrets.keys().map(String::as_str).collect())
                .unwrap_or_default();
            for (name, change) in &command.secrets {
                match change {
                    SecretChange::Replace(_) => present.insert(name.as_str()),
                    SecretChange::Remove => present.remove(name.as_str()),
                };
            }
            problems.extend(missing_secrets(descriptor, &present));
            return Err(ProviderConfigError::Invalid(problems));
        }
        match self
            .merge_provider_configuration(descriptor, command, settings, cipher)
            .await?
        {
            Merged::Saved(configuration) => Ok(*configuration),
            Merged::Missing(problems) => Err(ProviderConfigError::Invalid(problems)),
        }
    }

    #[transactional(source_provider)]
    async fn merge_provider_configuration<C: SecretCipher>(
        &self,
        descriptor: &SourceProviderDescriptor,
        command: SaveProviderConfiguration,
        mut settings: Map<String, Value>,
        cipher: &C,
    ) -> Result<Merged, CoreError> {
        let mut repo = source_provider_repository;
        let now = Utc::now();
        let current = repo.find(descriptor.key).await?;
        stamp_attestations(
            descriptor,
            current.as_ref().map(|current| &current.settings),
            &mut settings,
            now,
        );
        let created_at = current.as_ref().map_or(now, |current| current.created_at);
        let url_changed = current.as_ref().is_some_and(|current| {
            descriptor.fields.iter().any(|field| {
                field.kind == FieldKind::Url
                    && current.settings.get(field.key) != settings.get(field.key)
            })
        });
        if url_changed && let Some(current) = &current {
            let unsupplied: Vec<FieldProblem> = descriptor
                .secret_fields()
                .filter(|field| current.has_secret(field.key))
                .filter(|field| {
                    !matches!(
                        command.secrets.get(field.key),
                        Some(SecretChange::Replace(_))
                    )
                })
                .map(|field| FieldProblem {
                    field: format!("secrets.{}", field.key),
                    message: "must be provided again when a URL setting changes".to_owned(),
                })
                .collect();
            if !unsupplied.is_empty() {
                return Ok(Merged::Missing(unsupplied));
            }
        }
        let reconnect =
            url_changed && matches!(descriptor.auth, AuthSpec::OAuth2AuthorizationCode { .. });
        let mut connected_account = current
            .as_ref()
            .and_then(|current| current.connected_account.clone());
        let mut secrets = current.map(|current| current.secrets).unwrap_or_default();
        if reconnect {
            secrets.remove(TOKEN_ACCESS);
            secrets.remove(TOKEN_REFRESH);
            connected_account = None;
        }
        for (name, change) in &command.secrets {
            match change {
                SecretChange::Replace(secret) => {
                    secrets.insert(name.clone(), seal(cipher, descriptor.key, name, secret)?);
                }
                SecretChange::Remove => {
                    secrets.remove(name);
                }
            }
        }
        let present: BTreeSet<&str> = secrets.keys().map(String::as_str).collect();
        let missing = missing_secrets(descriptor, &present);
        if !missing.is_empty() {
            return Ok(Merged::Missing(missing));
        }
        let configuration = ProviderConfiguration {
            key: descriptor.key.to_owned(),
            settings,
            secrets,
            status: ProviderStatus::Untested,
            last_test_at: None,
            last_test_message: None,
            connected_account,
            created_at,
            updated_at: now,
        };
        repo.save(&configuration).await?;
        Ok(Merged::Saved(Box::new(configuration)))
    }

    #[transactional(source_provider)]
    pub async fn record_provider_status(
        &self,
        key: &str,
        status: ProviderStatus,
        message: Option<String>,
        at: DateTime<Utc>,
    ) -> Result<ProviderConfiguration, CoreError> {
        let mut repo = source_provider_repository;
        let mut configuration = repo.find(key).await?.ok_or(CoreError::NotFound)?;
        configuration.status = status;
        configuration.last_test_at = Some(at);
        configuration.last_test_message = message;
        configuration.updated_at = at;
        repo.save(&configuration).await?;
        Ok(configuration)
    }

    #[transactional(source_provider)]
    pub async fn clear_provider_configuration(&self, key: &str) -> Result<(), CoreError> {
        let mut repo = source_provider_repository;
        if repo.delete(key).await? {
            Ok(())
        } else {
            Err(CoreError::NotFound)
        }
    }

    #[transactional(source_provider)]
    pub async fn begin_oauth_state<C: SecretCipher>(
        &self,
        command: NewOAuthState,
        cipher: &C,
    ) -> Result<OAuthState, CoreError> {
        let mut repo = source_provider_repository;
        let now = Utc::now();
        repo.purge_oauth_states(now).await?;
        let expires_at = ChronoDuration::from_std(OAUTH_STATE_TTL)
            .map(|ttl| now + ttl)
            .map_err(|_| CoreError::Internal("oauth state ttl out of range".to_owned()))?;
        let state = OAuthState {
            code_verifier: cipher
                .seal(
                    &command.code_verifier,
                    &secret_context(&command.provider_key, "oauth_state"),
                )
                .map_err(|_| CoreError::Internal("code verifier could not be sealed".to_owned()))?,
            state: command.state,
            provider_key: command.provider_key,
            user_id: command.user_id,
            redirect_uri: command.redirect_uri,
            created_at: now,
            expires_at,
        };
        repo.insert_oauth_state(&state).await?;
        Ok(state)
    }

    pub async fn consume_oauth_state<C: SecretCipher>(
        &self,
        state: &str,
        cipher: &C,
    ) -> Result<ConsumedOAuthState, OAuthStateError> {
        let taken = self.take_oauth_state(state).await?;
        let taken = taken.ok_or(OAuthStateError::Unknown)?;
        if taken.is_expired(Utc::now()) {
            return Err(OAuthStateError::Expired);
        }
        let code_verifier = cipher
            .open(
                &taken.code_verifier,
                &secret_context(&taken.provider_key, "oauth_state"),
            )
            .map_err(|_| OAuthStateError::Cipher)?;
        Ok(ConsumedOAuthState {
            provider_key: taken.provider_key,
            user_id: taken.user_id,
            code_verifier,
            redirect_uri: taken.redirect_uri,
        })
    }

    #[transactional(source_provider)]
    async fn take_oauth_state(&self, state: &str) -> Result<Option<OAuthState>, CoreError> {
        let mut repo = source_provider_repository;
        repo.take_oauth_state(state).await
    }

    #[transactional(source_provider)]
    pub async fn store_provider_tokens<C: SecretCipher>(
        &self,
        key: &str,
        tokens: TokenUpdate,
        account: ConnectedAccount,
        cipher: &C,
    ) -> Result<ProviderConfiguration, CoreError> {
        let mut repo = source_provider_repository;
        let mut configuration = repo.find(key).await?.ok_or(CoreError::NotFound)?;
        configuration.secrets.insert(
            TOKEN_ACCESS.to_owned(),
            seal(cipher, key, TOKEN_ACCESS, &tokens.access)?,
        );
        if let Some(refresh) = &tokens.refresh {
            configuration.secrets.insert(
                TOKEN_REFRESH.to_owned(),
                seal(cipher, key, TOKEN_REFRESH, refresh)?,
            );
        }
        configuration.connected_account = Some(account);
        configuration.updated_at = Utc::now();
        repo.save(&configuration).await?;
        Ok(configuration)
    }

    #[transactional(source_provider)]
    pub async fn clear_provider_tokens(
        &self,
        key: &str,
        message: Option<String>,
    ) -> Result<ProviderConfiguration, CoreError> {
        let mut repo = source_provider_repository;
        let mut configuration = repo.find(key).await?.ok_or(CoreError::NotFound)?;
        configuration.secrets.remove(TOKEN_ACCESS);
        configuration.secrets.remove(TOKEN_REFRESH);
        configuration.connected_account = None;
        configuration.status = ProviderStatus::Untested;
        configuration.last_test_message = message;
        configuration.last_test_at = None;
        configuration.updated_at = Utc::now();
        repo.save(&configuration).await?;
        Ok(configuration)
    }
}
