use std::{collections::BTreeMap, sync::Arc, time::Duration};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use common::{CoreError, Secret};
use fetch::{ApiFetcher, ApiRequest};
use lodestone_core::{
    AuthSpec, ConnectedAccount, Credential, FetchError, FetchResponse, LodestoneUseCase,
    ProbeContext, ProviderConfiguration, ProviderProbe, ProviderStatus, SecretCipher, SourceName,
    SourceProviderDescriptor, TOKEN_ACCESS, TOKEN_REFRESH, UserId, catalog,
    source_providers::{
        ConsumedOAuthState, NewOAuthState, OAuthStateError, TokenUpdate, open_secrets,
    },
};
use rate_limit::{Clock, SystemClock};
use serde_json::{Value, from_slice};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::warn;
use url::Url;

use crate::{
    cache::TokenCache,
    error::{AuthFailure, fetch_reason},
    pkce,
    probe::{Probe, fetch_source},
    token::{TokenResponse, parse_token},
};

const DEFAULT_MARGIN: Duration = Duration::from_secs(60);
const REVOKED_MESSAGE: &str = "L'accès a été révoqué ou a expiré : reconnectez le compte.";
const DISCONNECTED_MESSAGE: &str = "Compte déconnecté.";
const NOT_CONNECTED_MESSAGE: &str = "Connectez un compte pour tester la connexion.";
const NO_PROBE_MESSAGE: &str = "Aucun test n'existe pour cette source.";

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("unknown source provider")]
    UnknownProvider,
    #[error("the provider is not configured")]
    NotConfigured,
    #[error("the provider does not use this authentication mode")]
    WrongAuthMode,
    #[error(transparent)]
    Auth(#[from] AuthFailure),
    #[error(transparent)]
    Core(#[from] CoreError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackFailure {
    InvalidState,
    Expired,
    ExchangeFailed,
    Internal,
}

impl CallbackFailure {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidState => "invalid_state",
            Self::Expired => "expired_state",
            Self::ExchangeFailed => "exchange_failed",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackError {
    pub provider_key: Option<String>,
    pub failure: CallbackFailure,
}

pub struct ProviderEngine<F: ApiFetcher, K: Clock = SystemClock> {
    catalog: &'static [SourceProviderDescriptor],
    fetcher: Arc<F>,
    clock: K,
    margin: Duration,
    allow_http: bool,
    cache: TokenCache,
    refresh_lock: Mutex<()>,
}

impl<F: ApiFetcher> ProviderEngine<F> {
    pub fn new(fetcher: Arc<F>) -> Self {
        Self::with_clock(fetcher, SystemClock)
    }
}

fn secret_of<'a>(
    secrets: &'a BTreeMap<String, Secret>,
    name: &'static str,
) -> Result<&'a Secret, AuthFailure> {
    secrets
        .get(name)
        .ok_or(AuthFailure::MissingCredential(name))
}

fn setting<'a>(
    configuration: &'a ProviderConfiguration,
    name: &'static str,
) -> Result<&'a str, AuthFailure> {
    configuration
        .settings
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(AuthFailure::MissingCredential(name))
}

fn endpoint(
    configuration: &ProviderConfiguration,
    key: &str,
    default: &str,
) -> Result<Url, AuthFailure> {
    let raw = configuration
        .settings
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(default);
    Url::parse(raw).map_err(|_| AuthFailure::InvalidEndpoint)
}

fn pair(name: &str, value: &str) -> (String, Secret) {
    (name.to_owned(), Secret::new(value))
}

fn expiry_of(expires_in: u64) -> Option<DateTime<Utc>> {
    i64::try_from(expires_in)
        .ok()
        .and_then(ChronoDuration::try_seconds)
        .map(|ttl| Utc::now() + ttl)
}

impl<F: ApiFetcher, K: Clock> ProviderEngine<F, K> {
    pub fn with_clock(fetcher: Arc<F>, clock: K) -> Self {
        Self {
            catalog: catalog(),
            fetcher,
            clock,
            margin: DEFAULT_MARGIN,
            allow_http: false,
            cache: TokenCache::default(),
            refresh_lock: Mutex::new(()),
        }
    }

    pub fn with_catalog(mut self, catalog: &'static [SourceProviderDescriptor]) -> Self {
        self.catalog = catalog;
        self
    }

    fn lookup(&self, key: &str) -> Option<&'static SourceProviderDescriptor> {
        self.catalog.iter().find(|descriptor| descriptor.key == key)
    }

    pub fn with_allow_http(mut self, allow_http: bool) -> Self {
        self.allow_http = allow_http;
        self
    }

    pub fn allows_http(&self) -> bool {
        self.allow_http
    }

    pub fn with_margin(mut self, margin: Duration) -> Self {
        self.margin = margin;
        self
    }

    pub fn forget(&self, key: &str) {
        self.cache.forget(key);
    }

    async fn post_form(
        &self,
        url: Url,
        source: SourceName,
        form: Vec<(String, Secret)>,
    ) -> Result<FetchResponse, AuthFailure> {
        let request = ApiRequest {
            url,
            source,
            bearer: None,
            headers: Vec::new(),
            form: Some(form),
        };
        self.fetcher
            .call(&request)
            .await
            .map_err(|error: FetchError| AuthFailure::Unreachable(fetch_reason(&error)))
    }

    async fn token_request(
        &self,
        configuration: &ProviderConfiguration,
        url: Url,
        form: Vec<(String, Secret)>,
    ) -> Result<TokenResponse, AuthFailure> {
        let response = self
            .post_form(url, fetch_source(&configuration.key), form)
            .await?;
        parse_token(&response)
    }

    async fn client_credentials_token<C: SecretCipher>(
        &self,
        configuration: &ProviderConfiguration,
        cipher: &C,
        token_url: &str,
        scopes: &[&str],
    ) -> Result<Secret, AuthFailure> {
        let now = self.clock.now();
        if let Some(token) = self.cache.get(&configuration.key, now, self.margin) {
            return Ok(token);
        }
        let generation = self.cache.generation(&configuration.key);
        let secrets = open_secrets(configuration, cipher).map_err(|_| AuthFailure::Cipher)?;
        let mut form = vec![
            pair("grant_type", "client_credentials"),
            pair("client_id", setting(configuration, "client_id")?),
            pair(
                "client_secret",
                secret_of(&secrets, "client_secret")?.expose(),
            ),
        ];
        if !scopes.is_empty() {
            form.push(pair("scope", &scopes.join(" ")));
        }
        let url = endpoint(configuration, "token_url", token_url)?;
        let token = self.token_request(configuration, url, form).await?;
        self.cache.put(
            &configuration.key,
            generation,
            token.access_token.clone(),
            now + Duration::from_secs(token.expires_in),
        );
        Ok(token.access_token)
    }

    pub async fn credential<C: SecretCipher>(
        &self,
        usecase: &LodestoneUseCase,
        cipher: &C,
        configuration: &ProviderConfiguration,
    ) -> Result<Credential, AuthFailure> {
        let descriptor = self
            .lookup(&configuration.key)
            .ok_or(AuthFailure::InvalidEndpoint)?;
        match descriptor.auth {
            AuthSpec::None => Ok(Credential::None),
            AuthSpec::ApiKey { header, prefix } => {
                let secrets =
                    open_secrets(configuration, cipher).map_err(|_| AuthFailure::Cipher)?;
                let key = secret_of(&secrets, "api_key")?;
                Ok(Credential::Header {
                    name: header.to_owned(),
                    value: Secret::new(format!("{prefix}{}", key.expose())),
                })
            }
            AuthSpec::OAuth2ClientCredentials { token_url, scopes } => self
                .client_credentials_token(configuration, cipher, token_url, scopes)
                .await
                .map(Credential::Bearer),
            AuthSpec::OAuth2AuthorizationCode { .. } => self
                .authorization_code_token(usecase, cipher, configuration)
                .await
                .map(Credential::Bearer),
        }
    }

    pub async fn credential_for<C: SecretCipher>(
        &self,
        usecase: &LodestoneUseCase,
        cipher: &C,
        key: &str,
    ) -> Result<Credential, EngineError> {
        let configuration = usecase
            .get_provider_configuration(key)
            .await?
            .ok_or(EngineError::NotConfigured)?;
        Ok(self.credential(usecase, cipher, &configuration).await?)
    }

    async fn authorization_code_token<C: SecretCipher>(
        &self,
        usecase: &LodestoneUseCase,
        cipher: &C,
        configuration: &ProviderConfiguration,
    ) -> Result<Secret, AuthFailure> {
        let account = configuration
            .connected_account
            .as_ref()
            .ok_or(AuthFailure::NotConnected)?;
        let margin = ChronoDuration::from_std(self.margin).unwrap_or_default();
        let fresh = account
            .expires_at
            .is_none_or(|expires_at| Utc::now() + margin < expires_at);
        if fresh {
            let secrets = open_secrets(configuration, cipher).map_err(|_| AuthFailure::Cipher)?;
            return secret_of(&secrets, TOKEN_ACCESS)
                .cloned()
                .map_err(|_| AuthFailure::NotConnected);
        }
        let refreshed = self.refresh(usecase, cipher, &configuration.key).await?;
        let secrets = open_secrets(&refreshed, cipher).map_err(|_| AuthFailure::Cipher)?;
        secret_of(&secrets, TOKEN_ACCESS)
            .cloned()
            .map_err(|_| AuthFailure::NotConnected)
    }

    pub async fn refresh<C: SecretCipher>(
        &self,
        usecase: &LodestoneUseCase,
        cipher: &C,
        key: &str,
    ) -> Result<ProviderConfiguration, AuthFailure> {
        let _guard = self.refresh_lock.lock().await;
        let configuration = usecase
            .get_provider_configuration(key)
            .await
            .map_err(|_| AuthFailure::InvalidResponse)?
            .ok_or(AuthFailure::NotConnected)?;
        let account = configuration
            .connected_account
            .clone()
            .ok_or(AuthFailure::NotConnected)?;
        let margin = ChronoDuration::from_std(self.margin).unwrap_or_default();
        if account
            .expires_at
            .is_none_or(|expires_at| Utc::now() + margin < expires_at)
        {
            return Ok(configuration);
        }
        let descriptor = self.lookup(key).ok_or(AuthFailure::InvalidEndpoint)?;
        let AuthSpec::OAuth2AuthorizationCode { token_url, .. } = descriptor.auth else {
            return Err(AuthFailure::Rejected(400));
        };
        let secrets = open_secrets(&configuration, cipher).map_err(|_| AuthFailure::Cipher)?;
        let refresh_token =
            secret_of(&secrets, TOKEN_REFRESH).map_err(|_| AuthFailure::NotConnected)?;
        let mut form = vec![
            pair("grant_type", "refresh_token"),
            pair("refresh_token", refresh_token.expose()),
            pair("client_id", setting(&configuration, "client_id")?),
        ];
        if let Some(secret) = secrets.get("client_secret") {
            form.push(pair("client_secret", secret.expose()));
        }
        let url = endpoint(&configuration, "token_url", token_url)?;
        let token = match self.token_request(&configuration, url, form).await {
            Ok(token) => token,
            Err(AuthFailure::Rejected(status)) => {
                if let Err(error) = usecase
                    .record_provider_status(
                        key,
                        ProviderStatus::Failing,
                        Some(REVOKED_MESSAGE.to_owned()),
                        Utc::now(),
                    )
                    .await
                {
                    warn!(provider = key, %error, "could not record the failed token refresh");
                }
                return Err(AuthFailure::Rejected(status));
            }
            Err(error) => return Err(error),
        };
        let renewed = ConnectedAccount {
            expires_at: expiry_of(token.expires_in),
            scopes: if token.scopes.is_empty() {
                account.scopes.clone()
            } else {
                token.scopes.clone()
            },
            ..account
        };
        usecase
            .store_provider_tokens(
                key,
                TokenUpdate {
                    access: token.access_token,
                    refresh: token.refresh_token,
                },
                renewed,
                cipher,
            )
            .await
            .map_err(|_| AuthFailure::InvalidResponse)
    }

    pub async fn test<C: SecretCipher>(
        &self,
        usecase: &LodestoneUseCase,
        cipher: &C,
        key: &str,
    ) -> Result<ProviderConfiguration, EngineError> {
        let descriptor = self.lookup(key).ok_or(EngineError::UnknownProvider)?;
        let configuration = usecase
            .get_provider_configuration(key)
            .await?
            .ok_or(EngineError::NotConfigured)?;
        let now = Utc::now();
        let needs_account = matches!(descriptor.auth, AuthSpec::OAuth2AuthorizationCode { .. });
        if needs_account && configuration.connected_account.is_none() {
            return Ok(usecase
                .record_provider_status(
                    key,
                    ProviderStatus::Untested,
                    Some(NOT_CONNECTED_MESSAGE.to_owned()),
                    now,
                )
                .await?);
        }
        let credential = match self.credential(usecase, cipher, &configuration).await {
            Ok(credential) => credential,
            Err(failure) => {
                return Ok(usecase
                    .record_provider_status(
                        key,
                        ProviderStatus::Failing,
                        Some(failure.to_string()),
                        Utc::now(),
                    )
                    .await?);
            }
        };
        let (status, message) = match Probe::for_key(key, self.fetcher.as_ref()) {
            Some(probe) => {
                let outcome = probe
                    .probe(&ProbeContext {
                        settings: &configuration.settings,
                        credential: &credential,
                    })
                    .await;
                let status = if outcome.ok {
                    ProviderStatus::Ready
                } else {
                    ProviderStatus::Failing
                };
                (status, outcome.message)
            }
            None => (ProviderStatus::Failing, NO_PROBE_MESSAGE.to_owned()),
        };
        Ok(usecase
            .record_provider_status(key, status, Some(message), Utc::now())
            .await?)
    }

    pub async fn start_oauth<C: SecretCipher>(
        &self,
        usecase: &LodestoneUseCase,
        cipher: &C,
        key: &str,
        user: UserId,
        redirect_uri: &str,
    ) -> Result<Url, EngineError> {
        let descriptor = self.lookup(key).ok_or(EngineError::UnknownProvider)?;
        let AuthSpec::OAuth2AuthorizationCode {
            authorize_url,
            scopes,
            pkce: use_pkce,
            ..
        } = descriptor.auth
        else {
            return Err(EngineError::WrongAuthMode);
        };
        let configuration = usecase
            .get_provider_configuration(key)
            .await?
            .ok_or(EngineError::NotConfigured)?;
        let client_id = setting(&configuration, "client_id")?;
        let state = pkce::random_token().ok_or(EngineError::Auth(AuthFailure::InvalidResponse))?;
        let verifier =
            pkce::random_token().ok_or(EngineError::Auth(AuthFailure::InvalidResponse))?;
        let mut url = endpoint(&configuration, "authorize_url", authorize_url)?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("response_type", "code")
                .append_pair("client_id", client_id)
                .append_pair("redirect_uri", redirect_uri)
                .append_pair("state", &state);
            if !scopes.is_empty() {
                query.append_pair("scope", &scopes.join(" "));
            }
            if use_pkce {
                query
                    .append_pair("code_challenge", &pkce::challenge(&verifier))
                    .append_pair("code_challenge_method", "S256");
            }
        }
        usecase
            .begin_oauth_state(
                NewOAuthState {
                    state,
                    provider_key: key.to_owned(),
                    user_id: user,
                    code_verifier: Secret::new(verifier),
                    redirect_uri: redirect_uri.to_owned(),
                },
                cipher,
            )
            .await?;
        Ok(url)
    }

    pub async fn abandon_oauth<C: SecretCipher>(
        &self,
        usecase: &LodestoneUseCase,
        cipher: &C,
        state: &str,
    ) -> Option<String> {
        usecase
            .consume_oauth_state(state, cipher)
            .await
            .ok()
            .map(|consumed| consumed.provider_key)
    }

    pub async fn complete_oauth<C: SecretCipher>(
        &self,
        usecase: &LodestoneUseCase,
        cipher: &C,
        state: &str,
        code: &str,
    ) -> Result<String, CallbackError> {
        let consumed = usecase
            .consume_oauth_state(state, cipher)
            .await
            .map_err(|error| CallbackError {
                provider_key: None,
                failure: match error {
                    OAuthStateError::Unknown => CallbackFailure::InvalidState,
                    OAuthStateError::Expired => CallbackFailure::Expired,
                    OAuthStateError::Cipher | OAuthStateError::Core(_) => CallbackFailure::Internal,
                },
            })?;
        let key = consumed.provider_key.clone();
        let fail = |failure| CallbackError {
            provider_key: Some(key.clone()),
            failure,
        };
        self.exchange(usecase, cipher, &consumed, code)
            .await
            .map_err(|error| {
                warn!(provider = %key, error = %error, "oauth code exchange failed");
                fail(match error {
                    EngineError::Auth(_) => CallbackFailure::ExchangeFailed,
                    _ => CallbackFailure::Internal,
                })
            })?;
        if let Err(error) = self.test(usecase, cipher, &key).await {
            warn!(provider = %key, %error, "connection test after oauth callback failed");
        }
        Ok(key)
    }

    async fn exchange<C: SecretCipher>(
        &self,
        usecase: &LodestoneUseCase,
        cipher: &C,
        consumed: &ConsumedOAuthState,
        code: &str,
    ) -> Result<(), EngineError> {
        let key = consumed.provider_key.as_str();
        let descriptor = self.lookup(key).ok_or(EngineError::UnknownProvider)?;
        let AuthSpec::OAuth2AuthorizationCode {
            token_url,
            userinfo_url,
            scopes,
            pkce: use_pkce,
            ..
        } = descriptor.auth
        else {
            return Err(EngineError::WrongAuthMode);
        };
        let configuration = usecase
            .get_provider_configuration(key)
            .await?
            .ok_or(EngineError::NotConfigured)?;
        let secrets = open_secrets(&configuration, cipher).map_err(|_| AuthFailure::Cipher)?;
        let mut form = vec![
            pair("grant_type", "authorization_code"),
            pair("code", code),
            pair("redirect_uri", &consumed.redirect_uri),
            pair("client_id", setting(&configuration, "client_id")?),
        ];
        if let Some(secret) = secrets.get("client_secret") {
            form.push(pair("client_secret", secret.expose()));
        }
        if use_pkce {
            form.push(pair("code_verifier", consumed.code_verifier.expose()));
        }
        let url = endpoint(&configuration, "token_url", token_url)?;
        let token = self.token_request(&configuration, url, form).await?;
        let (display_name, account_id) = match userinfo_url {
            Some(userinfo) => self
                .userinfo(&configuration, userinfo, &token.access_token)
                .await
                .unwrap_or_default(),
            None => Default::default(),
        };
        let account = ConnectedAccount {
            display_name: display_name.unwrap_or_else(|| "Compte connecté".to_owned()),
            account_id,
            scopes: if token.scopes.is_empty() {
                scopes.iter().map(|scope| (*scope).to_owned()).collect()
            } else {
                token.scopes.clone()
            },
            expires_at: expiry_of(token.expires_in),
            connected_by: Some(consumed.user_id.to_string()),
        };
        usecase
            .store_provider_tokens(
                key,
                TokenUpdate {
                    access: token.access_token,
                    refresh: token.refresh_token,
                },
                account,
                cipher,
            )
            .await?;
        Ok(())
    }

    async fn userinfo(
        &self,
        configuration: &ProviderConfiguration,
        url: &str,
        access: &Secret,
    ) -> Option<(Option<String>, Option<String>)> {
        let request = ApiRequest {
            url: endpoint(configuration, "userinfo_url", url).ok()?,
            source: fetch_source(&configuration.key),
            bearer: Some(access.clone()),
            headers: Vec::new(),
            form: None,
        };
        let response = self.fetcher.call(&request).await.ok()?;
        if !(200..300).contains(&response.status) {
            return None;
        }
        let body: Value = from_slice(&response.body).ok()?;
        let text = |names: &[&str]| {
            names.iter().find_map(|name| match body.get(*name) {
                Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
                Some(Value::Number(value)) => Some(value.to_string()),
                _ => None,
            })
        };
        Some((
            text(&["name", "display_name", "login", "email", "username"]),
            text(&["id", "sub"]),
        ))
    }

    pub async fn disconnect<C: SecretCipher>(
        &self,
        usecase: &LodestoneUseCase,
        cipher: &C,
        key: &str,
    ) -> Result<ProviderConfiguration, EngineError> {
        let descriptor = self.lookup(key).ok_or(EngineError::UnknownProvider)?;
        let AuthSpec::OAuth2AuthorizationCode { revoke_url, .. } = descriptor.auth else {
            return Err(EngineError::WrongAuthMode);
        };
        let configuration = usecase
            .get_provider_configuration(key)
            .await?
            .ok_or(EngineError::NotConfigured)?;
        if let Some(revoke) = revoke_url {
            self.revoke(&configuration, cipher, revoke).await;
        }
        self.cache.forget(key);
        Ok(usecase
            .clear_provider_tokens(key, Some(DISCONNECTED_MESSAGE.to_owned()))
            .await?)
    }

    async fn revoke<C: SecretCipher>(
        &self,
        configuration: &ProviderConfiguration,
        cipher: &C,
        default: &str,
    ) {
        let Ok(secrets) = open_secrets(configuration, cipher) else {
            return;
        };
        let (hint, token) = match (secrets.get(TOKEN_REFRESH), secrets.get(TOKEN_ACCESS)) {
            (Some(refresh), _) => ("refresh_token", refresh),
            (None, Some(access)) => ("access_token", access),
            (None, None) => return,
        };
        let Ok(client_id) = setting(configuration, "client_id") else {
            return;
        };
        let Ok(url) = endpoint(configuration, "revoke_url", default) else {
            return;
        };
        let mut form = vec![
            pair("token", token.expose()),
            pair("token_type_hint", hint),
            pair("client_id", client_id),
        ];
        if let Some(secret) = secrets.get("client_secret") {
            form.push(pair("client_secret", secret.expose()));
        }
        if let Err(error) = self
            .post_form(url, fetch_source(&configuration.key), form)
            .await
        {
            warn!(provider = %configuration.key, %error, "token revocation failed");
        }
    }
}
