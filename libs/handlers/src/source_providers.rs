use std::collections::BTreeMap;

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    response::Redirect,
};
use common::Secret;
use lodestone_core::{
    AuthSpec, ConnectedAccount, FieldSpec, ProviderConfiguration, ProviderStatus,
    SourceProviderDescriptor, UserId, catalog, mask_secret, provider_descriptor,
    source_providers::{
        ProviderConfigError, SaveProviderConfiguration, SecretChange, open_secrets,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use source_providers::EngineError;
use tracing::warn;
use url::Url;
use utoipa::ToSchema;

use crate::{
    auth::AuthenticatedUser,
    errors::{ApiError, ErrorDetail},
    response::{DataEnvelope, Response},
    state::AppState,
};

const CALLBACK_PATH: &str = "/api/v1/source-providers/oauth/callback";

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct FieldDto {
    pub key: String,
    pub label: String,
    /// One of `text`, `url`, `secret`, `select`, `number`, `bool`.
    pub kind: String,
    pub required: bool,
    pub default: Option<String>,
    pub help: String,
    pub options: Vec<String>,
}

impl From<&FieldSpec> for FieldDto {
    fn from(field: &FieldSpec) -> Self {
        Self {
            key: field.key.to_owned(),
            label: field.label.to_owned(),
            kind: field.kind.as_str().to_owned(),
            required: field.required,
            default: field.default.map(str::to_owned),
            help: field.help.to_owned(),
            options: field
                .options
                .iter()
                .map(|option| (*option).to_owned())
                .collect(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct FiltersDto {
    pub remote: bool,
    pub countries: bool,
    pub contract: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct CapabilitiesDto {
    pub keyword_search: bool,
    pub filters: FiltersDto,
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct ConnectedAccountDto {
    pub display_name: String,
    pub account_id: Option<String>,
    pub scopes: Vec<String>,
    #[schema(format = DateTime)]
    pub expires_at: Option<String>,
}

impl From<&ConnectedAccount> for ConnectedAccountDto {
    fn from(account: &ConnectedAccount) -> Self {
        Self {
            display_name: account.display_name.clone(),
            account_id: account.account_id.clone(),
            scopes: account.scopes.clone(),
            expires_at: account.expires_at.map(|at| at.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct ProviderSummaryDto {
    pub key: String,
    pub name: String,
    pub description: String,
    /// One of `public_procurement`, `jobs_and_missions`, `technical_signals`, `companies`.
    pub category: String,
    /// One of `none`, `api_key`, `oauth2_client_credentials`, `oauth2_authorization_code`.
    pub auth_kind: String,
    pub capabilities: CapabilitiesDto,
    /// One of `not_configured`, `untested`, `ready`, `failing`. Only `ready` providers are usable.
    pub status: String,
    #[schema(format = DateTime)]
    pub last_test_at: Option<String>,
    pub connected_account: Option<ConnectedAccountDto>,
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct SecretStateDto {
    /// Key of the secret field.
    pub key: String,
    pub is_set: bool,
    /// Masked value: four dots followed by the last four characters when the secret is long enough.
    pub hint: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct ProviderDetailDto {
    pub summary: ProviderSummaryDto,
    pub fields: Vec<FieldDto>,
    #[schema(value_type = Object)]
    pub settings: Map<String, Value>,
    /// One entry per secret field. Secret values are never returned.
    pub secrets: Vec<SecretStateDto>,
    pub scopes: Vec<String>,
    pub last_test_message: Option<String>,
    pub terms_note: Option<String>,
    /// Redirect URI to register at the provider, for the authorization code mode only.
    pub oauth_redirect_uri: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveProviderRequest {
    #[serde(default)]
    #[schema(value_type = Object)]
    pub settings: Map<String, Value>,
    /// Secret values by field key. Absent keeps the stored secret, `null` removes it, a string replaces it.
    #[serde(default)]
    #[schema(value_type = Object)]
    pub secrets: Map<String, Value>,
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct AuthorizeUrlDto {
    pub authorize_url: String,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

fn actor(user: Option<Extension<AuthenticatedUser>>) -> Result<UserId, ApiError> {
    match user {
        Some(Extension(AuthenticatedUser(id))) => Ok(id),
        None => Err(ApiError::Forbidden),
    }
}

fn find(key: &str) -> Result<&'static SourceProviderDescriptor, ApiError> {
    provider_descriptor(key).ok_or(ApiError::NotFound)
}

fn redirect_uri(state: &AppState) -> Result<String, ApiError> {
    let base = Url::parse(&state.args.server.public_url).map_err(|_| ApiError::Internal)?;
    Ok(format!(
        "{}{CALLBACK_PATH}",
        base.as_str().trim_end_matches('/')
    ))
}

fn summary(
    descriptor: &SourceProviderDescriptor,
    configuration: Option<&ProviderConfiguration>,
) -> ProviderSummaryDto {
    ProviderSummaryDto {
        key: descriptor.key.to_owned(),
        name: descriptor.name.to_owned(),
        description: descriptor.description.to_owned(),
        category: descriptor.category.as_str().to_owned(),
        auth_kind: descriptor.auth.kind().as_str().to_owned(),
        capabilities: CapabilitiesDto {
            keyword_search: descriptor.capabilities.keyword_search,
            filters: FiltersDto {
                remote: descriptor.capabilities.filters.remote,
                countries: descriptor.capabilities.filters.countries,
                contract: descriptor.capabilities.filters.contract,
            },
        },
        status: configuration
            .map_or(ProviderStatus::NotConfigured, |configuration| {
                configuration.status
            })
            .as_str()
            .to_owned(),
        last_test_at: configuration
            .and_then(|configuration| configuration.last_test_at)
            .map(|at| at.to_rfc3339()),
        connected_account: configuration
            .and_then(|configuration| configuration.connected_account.as_ref())
            .map(ConnectedAccountDto::from),
    }
}

fn detail(
    state: &AppState,
    descriptor: &SourceProviderDescriptor,
    configuration: Option<&ProviderConfiguration>,
) -> Result<ProviderDetailDto, ApiError> {
    let opened = match configuration {
        Some(configuration) => open_secrets(configuration, state.cipher.as_ref()).ok(),
        None => None,
    };
    let secrets = descriptor
        .secret_fields()
        .map(|field| {
            let is_set =
                configuration.is_some_and(|configuration| configuration.has_secret(field.key));
            let hint = is_set.then(|| {
                opened
                    .as_ref()
                    .and_then(|secrets| secrets.get(field.key))
                    .map_or_else(|| mask_secret(""), |secret| mask_secret(secret.expose()))
            });
            SecretStateDto {
                key: field.key.to_owned(),
                is_set,
                hint,
            }
        })
        .collect();
    let (scopes, oauth_redirect_uri) = match descriptor.auth {
        AuthSpec::OAuth2AuthorizationCode { scopes, .. } => {
            (scopes.to_vec(), Some(redirect_uri(state)?))
        }
        AuthSpec::OAuth2ClientCredentials { scopes, .. } => (scopes.to_vec(), None),
        AuthSpec::None | AuthSpec::ApiKey { .. } => (Vec::new(), None),
    };
    Ok(ProviderDetailDto {
        summary: summary(descriptor, configuration),
        fields: descriptor.fields.iter().map(FieldDto::from).collect(),
        settings: configuration
            .map(|configuration| configuration.settings.clone())
            .unwrap_or_default(),
        secrets,
        scopes: scopes.into_iter().map(str::to_owned).collect(),
        last_test_message: configuration
            .and_then(|configuration| configuration.last_test_message.clone()),
        terms_note: descriptor.terms_note.map(str::to_owned),
        oauth_redirect_uri,
    })
}

fn engine_error(error: EngineError) -> ApiError {
    match error {
        EngineError::UnknownProvider => ApiError::NotFound,
        EngineError::NotConfigured => {
            ApiError::Conflict("the provider is not configured yet".to_owned())
        }
        EngineError::WrongAuthMode => {
            ApiError::Conflict("the provider does not use this authentication mode".to_owned())
        }
        EngineError::Auth(failure) => {
            warn!(error = %failure, "source provider authentication failed");
            ApiError::ExternalService("provider authentication failed".to_owned())
        }
        EngineError::Core(error) => error.into(),
    }
}

fn parse_secrets(input: Map<String, Value>) -> Result<BTreeMap<String, SecretChange>, ApiError> {
    let mut problems = Vec::new();
    let mut secrets = BTreeMap::new();
    for (name, value) in input {
        match value {
            Value::Null => {
                secrets.insert(name, SecretChange::Remove);
            }
            Value::String(text) if !text.trim().is_empty() => {
                secrets.insert(name, SecretChange::Replace(Secret::new(text)));
            }
            Value::String(_) => problems.push(ErrorDetail {
                path: format!("secrets.{name}"),
                message: "must not be empty".to_owned(),
            }),
            _ => problems.push(ErrorDetail {
                path: format!("secrets.{name}"),
                message: "must be a string or null".to_owned(),
            }),
        }
    }
    if problems.is_empty() {
        Ok(secrets)
    } else {
        Err(ApiError::Rejected(problems))
    }
}

fn config_error(error: ProviderConfigError) -> ApiError {
    match error {
        ProviderConfigError::UnknownProvider => ApiError::NotFound,
        ProviderConfigError::Invalid(problems) => ApiError::Rejected(
            problems
                .into_iter()
                .map(|problem| ErrorDetail {
                    path: problem.field,
                    message: problem.message,
                })
                .collect(),
        ),
        ProviderConfigError::Cipher => ApiError::Internal,
        ProviderConfigError::Core(error) => error.into(),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/source-providers",
    tag = "source-providers",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "The provider catalog with each provider's status", body = DataEnvelope<Vec<ProviderSummaryDto>>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
    )
)]
pub async fn list_source_providers(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
) -> Result<Response<Vec<ProviderSummaryDto>>, ApiError> {
    actor(user)?;
    let configurations = state.usecase.list_provider_configurations().await?;
    Ok(Response::OK(
        catalog()
            .iter()
            .map(|descriptor| {
                let configuration = configurations
                    .iter()
                    .find(|configuration| configuration.key == descriptor.key);
                summary(descriptor, configuration)
            })
            .collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/source-providers/{key}",
    tag = "source-providers",
    security(("bearer_auth" = [])),
    params(("key" = String, Path, description = "Provider key")),
    responses(
        (status = 200, description = "The provider descriptor and its configuration; secrets are masked", body = DataEnvelope<ProviderDetailDto>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown provider"),
    )
)]
pub async fn get_source_provider(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(key): Path<String>,
) -> Result<Response<ProviderDetailDto>, ApiError> {
    actor(user)?;
    let descriptor = find(&key)?;
    let configuration = state
        .usecase
        .get_provider_configuration(descriptor.key)
        .await?;
    Ok(Response::OK(detail(
        &state,
        descriptor,
        configuration.as_ref(),
    )?))
}

#[utoipa::path(
    put,
    path = "/api/v1/source-providers/{key}",
    tag = "source-providers",
    security(("bearer_auth" = [])),
    params(("key" = String, Path, description = "Provider key")),
    request_body = SaveProviderRequest,
    responses(
        (status = 200, description = "The configuration was saved and the connection tested; the status tells whether the provider is usable", body = DataEnvelope<ProviderDetailDto>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown provider"),
        (status = 422, description = "Settings or secrets rejected by the provider's field specifications"),
    )
)]
pub async fn put_source_provider(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(key): Path<String>,
    Json(body): Json<SaveProviderRequest>,
) -> Result<Response<ProviderDetailDto>, ApiError> {
    actor(user)?;
    let descriptor = find(&key)?;
    let secrets = parse_secrets(body.secrets)?;
    state
        .usecase
        .save_provider_configuration(
            descriptor,
            SaveProviderConfiguration {
                settings: body.settings,
                secrets,
            },
            state.providers.allows_http(),
            state.cipher.as_ref(),
        )
        .await
        .map_err(config_error)?;
    state.providers.forget(descriptor.key);
    let tested = state
        .providers
        .test(&state.usecase, state.cipher.as_ref(), descriptor.key)
        .await
        .map_err(engine_error)?;
    Ok(Response::OK(detail(&state, descriptor, Some(&tested))?))
}

#[utoipa::path(
    post,
    path = "/api/v1/source-providers/{key}/test",
    tag = "source-providers",
    security(("bearer_auth" = [])),
    params(("key" = String, Path, description = "Provider key")),
    responses(
        (status = 200, description = "The connection was tested; failures are reported in the status and message", body = DataEnvelope<ProviderDetailDto>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown provider"),
        (status = 409, description = "The provider is not configured yet"),
    )
)]
pub async fn test_source_provider(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(key): Path<String>,
) -> Result<Response<ProviderDetailDto>, ApiError> {
    actor(user)?;
    let descriptor = find(&key)?;
    let tested = state
        .providers
        .test(&state.usecase, state.cipher.as_ref(), descriptor.key)
        .await
        .map_err(engine_error)?;
    Ok(Response::OK(detail(&state, descriptor, Some(&tested))?))
}

#[utoipa::path(
    post,
    path = "/api/v1/source-providers/{key}/oauth/start",
    tag = "source-providers",
    security(("bearer_auth" = [])),
    params(("key" = String, Path, description = "Provider key")),
    responses(
        (status = 200, description = "Where to send the operator to grant access; the state is valid for ten minutes and bound to the caller", body = DataEnvelope<AuthorizeUrlDto>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown provider"),
        (status = 409, description = "The provider is not configured or does not use the authorization code mode"),
    )
)]
pub async fn start_source_provider_oauth(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(key): Path<String>,
) -> Result<Response<AuthorizeUrlDto>, ApiError> {
    let user = actor(user)?;
    let descriptor = find(&key)?;
    let url = state
        .providers
        .start_oauth(
            &state.usecase,
            state.cipher.as_ref(),
            descriptor.key,
            user,
            &redirect_uri(&state)?,
        )
        .await
        .map_err(engine_error)?;
    Ok(Response::OK(AuthorizeUrlDto {
        authorize_url: url.to_string(),
    }))
}

fn landing(state: &AppState, key: Option<&str>, outcome: &str) -> Redirect {
    let base = state.args.server.webapp_url.trim_end_matches('/');
    let target = match key {
        Some(key) => format!("{base}/sources/providers/{key}?{outcome}"),
        None => format!("{base}/sources?{outcome}"),
    };
    Redirect::to(&target)
}

#[utoipa::path(
    get,
    path = "/api/v1/source-providers/oauth/callback",
    tag = "source-providers",
    params(
        ("code" = Option<String>, Query, description = "Authorization code"),
        ("state" = Option<String>, Query, description = "State issued by the start endpoint"),
        ("error" = Option<String>, Query, description = "Error reported by the provider"),
    ),
    responses(
        (status = 303, description = "Redirects the browser to the provider page of the web UI with `connected=1` or `error=<code>`. The request is not authenticated: the one-time state, bound to the user who started the flow, authorises it"),
    )
)]
pub async fn source_provider_oauth_callback(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> Redirect {
    let cipher = state.cipher.as_ref();
    let Some(token) = query.state.as_deref() else {
        return landing(&state, None, "error=invalid_request");
    };
    if query.error.is_some() {
        let key = state
            .providers
            .abandon_oauth(&state.usecase, cipher, token)
            .await;
        return landing(&state, key.as_deref(), "error=denied");
    }
    let Some(code) = query.code.as_deref() else {
        return landing(&state, None, "error=invalid_request");
    };
    match state
        .providers
        .complete_oauth(&state.usecase, cipher, token, code)
        .await
    {
        Ok(key) => landing(&state, Some(&key), "connected=1"),
        Err(error) => landing(
            &state,
            error.provider_key.as_deref(),
            &format!("error={}", error.failure.code()),
        ),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/source-providers/{key}/disconnect",
    tag = "source-providers",
    security(("bearer_auth" = [])),
    params(("key" = String, Path, description = "Provider key")),
    responses(
        (status = 200, description = "The token was revoked when the provider offers a revocation endpoint, then the connected account and its tokens were cleared", body = DataEnvelope<ProviderDetailDto>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown provider"),
        (status = 409, description = "The provider is not configured or does not use the authorization code mode"),
    )
)]
pub async fn disconnect_source_provider(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(key): Path<String>,
) -> Result<Response<ProviderDetailDto>, ApiError> {
    actor(user)?;
    let descriptor = find(&key)?;
    let cleared = state
        .providers
        .disconnect(&state.usecase, state.cipher.as_ref(), descriptor.key)
        .await
        .map_err(engine_error)?;
    Ok(Response::OK(detail(&state, descriptor, Some(&cleared))?))
}

#[utoipa::path(
    delete,
    path = "/api/v1/source-providers/{key}",
    tag = "source-providers",
    security(("bearer_auth" = [])),
    params(("key" = String, Path, description = "Provider key")),
    responses(
        (status = 204, description = "The configuration, its secrets and its tokens were cleared"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown provider or nothing configured"),
    )
)]
pub async fn delete_source_provider(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(key): Path<String>,
) -> Result<Response<()>, ApiError> {
    actor(user)?;
    let descriptor = find(&key)?;
    state
        .usecase
        .clear_provider_configuration(descriptor.key)
        .await?;
    state.providers.forget(descriptor.key);
    Ok(Response::NoContent)
}
