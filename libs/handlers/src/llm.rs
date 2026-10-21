use std::time::{Duration, Instant};

use axum::{
    Extension, Json,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse, Response as HttpResponse},
};
use common::{CoreError, Secret};
use http::{HeaderValue, StatusCode, header::LOCATION};
use llm::any::AnyProvider;
use lodestone_core::{
    LlmError, LlmProvider, LlmProviderRecord, LlmRequest, ProviderDefinition, ProviderId,
    ProviderKind, ReasoningEffort, SecretCipher, Tier, TokenPricing,
    llm::{CreateLlmProvider, KeyChange, UpdateLlmProvider},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use url::Url;
use utoipa::ToSchema;

use crate::{
    auth::AuthenticatedUser,
    errors::{ApiError, ErrorBody, ErrorDetail},
    response::{DataEnvelope, Response},
    state::AppState,
};

const DEFAULT_MAX_RETRIES: u8 = 2;
const MAX_RETRIES: u64 = 10;
const DEFAULT_TIMEOUT_SECONDS: u64 = 30;
const MAX_TIMEOUT_SECONDS: u64 = 600;
const MAX_NAME_CHARS: usize = 100;
const TEST_TIMEOUT: Duration = Duration::from_secs(20);
const TEST_MAX_TOKENS: u32 = 64;
const REDACTED: &str = "[redacted]";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
#[schema(as = ReasoningEffort)]
pub enum ReasoningEffortDto {
    None,
    Low,
    Medium,
    High,
}

impl From<ReasoningEffort> for ReasoningEffortDto {
    fn from(effort: ReasoningEffort) -> Self {
        match effort {
            ReasoningEffort::None => Self::None,
            ReasoningEffort::Low => Self::Low,
            ReasoningEffort::Medium => Self::Medium,
            ReasoningEffort::High => Self::High,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct PricingDto {
    pub input_cents_per_mtok: u32,
    pub output_cents_per_mtok: u32,
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct ProviderDto {
    pub id: String,
    pub name: String,
    /// One of `anthropic`, `mistral`, `openai_compatible`.
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub enabled: bool,
    /// Whether an API key is stored. The key itself is never returned.
    pub has_api_key: bool,
    pub max_retries: u8,
    pub timeout_seconds: u64,
    pub pricing: PricingDto,
    /// Tiers (`triage`, `qualification`) this provider is assigned to.
    pub tiers: Vec<String>,
    /// Sent to OpenAI-compatible providers only when set; `null` when not sent.
    pub reasoning_effort: Option<ReasoningEffortDto>,
    #[schema(format = DateTime)]
    pub created_at: String,
    #[schema(format = DateTime)]
    pub updated_at: String,
}

impl ProviderDto {
    fn new(record: &LlmProviderRecord, assignments: &[(Tier, ProviderId)]) -> Self {
        Self {
            id: record.id.to_string(),
            name: record.name.clone(),
            kind: record.kind.as_str().to_owned(),
            base_url: record.base_url.as_str().to_owned(),
            model: record.model.clone(),
            enabled: record.enabled,
            has_api_key: record.has_api_key(),
            max_retries: record.max_retries,
            timeout_seconds: record.timeout.as_secs(),
            pricing: PricingDto {
                input_cents_per_mtok: record.pricing.input_cents_per_mtok,
                output_cents_per_mtok: record.pricing.output_cents_per_mtok,
            },
            tiers: assignments
                .iter()
                .filter(|(_, provider)| *provider == record.id)
                .map(|(tier, _)| tier.as_str().to_owned())
                .collect(),
            reasoning_effort: record.reasoning_effort.map(ReasoningEffortDto::from),
            created_at: record.created_at.to_rfc3339(),
            updated_at: record.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct TierDto {
    /// `triage` or `qualification`.
    pub tier: String,
    pub provider_id: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct TestErrorDto {
    /// One of `authentication`, `unreachable`, `timeout`, `invalid_response`, `rate_limited`, `reasoning_exhausted`, `other`.
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct TestResultDto {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<TestErrorDto>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PricingRequest {
    pub input_cents_per_mtok: Option<u32>,
    pub output_cents_per_mtok: Option<u32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProviderRequest {
    pub name: String,
    /// One of `anthropic`, `mistral`, `openai_compatible`.
    pub kind: String,
    /// http(s) URL of the provider API.
    pub base_url: String,
    pub model: String,
    /// Write-only. Required for `anthropic` and `mistral`.
    pub api_key: Option<String>,
    /// 0 to 10, defaults to 2.
    pub max_retries: Option<u8>,
    /// Defaults to zero.
    pub pricing: Option<PricingRequest>,
    /// 1 to 600, defaults to 30.
    pub timeout_seconds: Option<u64>,
    /// `none`, `low`, `medium` or `high`. Sent to OpenAI-compatible providers only when set.
    pub reasoning_effort: Option<ReasoningEffortDto>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateProviderRequest {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    /// Absent keeps the stored key, `null` removes it, a string replaces it. Never returned.
    #[schema(value_type = Option<String>, nullable)]
    pub api_key: Option<Option<String>>,
    pub max_retries: Option<u8>,
    pub pricing: Option<PricingRequest>,
    pub timeout_seconds: Option<u64>,
    /// Absent keeps the stored value, `null` stops sending it, a value sets it.
    #[schema(value_type = Option<ReasoningEffortDto>, nullable)]
    pub reasoning_effort: Option<Option<ReasoningEffortDto>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AssignTierRequest {
    /// Identifier (UUID) of the provider serving the tier.
    pub provider_id: String,
}

#[derive(Debug, PartialEq, Eq)]
enum KeyInput {
    Absent,
    Null,
    Value(String),
}

#[derive(Debug)]
struct ProviderInput {
    name: Option<String>,
    kind: Option<ProviderKind>,
    base_url: Option<Url>,
    model: Option<String>,
    api_key: KeyInput,
    max_retries: Option<u8>,
    input_cents: Option<u32>,
    output_cents: Option<u32>,
    timeout: Option<Duration>,
    reasoning_effort: Option<Option<ReasoningEffort>>,
}

struct Parser {
    problems: Vec<ErrorDetail>,
}

impl Parser {
    fn problem(&mut self, path: &str, message: &str) {
        self.problems.push(ErrorDetail {
            path: path.to_owned(),
            message: message.to_owned(),
        });
    }

    fn text(&mut self, map: &mut Map<String, Value>, field: &str, max: usize) -> Option<String> {
        match map.remove(field)? {
            Value::String(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    self.problem(field, "must not be empty");
                    None
                } else if trimmed.chars().count() > max {
                    self.problem(field, &format!("must be at most {max} characters"));
                    None
                } else {
                    Some(trimmed.to_owned())
                }
            }
            _ => {
                self.problem(field, "must be a string");
                None
            }
        }
    }

    fn kind(&mut self, map: &mut Map<String, Value>) -> Option<ProviderKind> {
        match map.remove("kind")? {
            Value::String(value) => {
                let kind = ProviderKind::parse(&value);
                if kind.is_none() {
                    self.problem(
                        "kind",
                        "must be one of anthropic, mistral, openai_compatible",
                    );
                }
                kind
            }
            _ => {
                self.problem("kind", "must be a string");
                None
            }
        }
    }

    fn reasoning_effort(
        &mut self,
        map: &mut Map<String, Value>,
    ) -> Option<Option<ReasoningEffort>> {
        match map.remove("reasoning_effort")? {
            Value::Null => Some(None),
            Value::String(value) => {
                let effort = ReasoningEffort::parse(&value);
                if effort.is_none() {
                    self.problem("reasoning_effort", "must be one of none, low, medium, high");
                }
                effort.map(Some)
            }
            _ => {
                self.problem("reasoning_effort", "must be a string or null");
                None
            }
        }
    }

    fn base_url(&mut self, map: &mut Map<String, Value>) -> Option<Url> {
        match map.remove("base_url")? {
            Value::String(value) => match Url::parse(value.trim()) {
                Ok(url) if matches!(url.scheme(), "http" | "https") && url.host().is_some() => {
                    Some(url)
                }
                _ => {
                    self.problem("base_url", "must be an http or https URL");
                    None
                }
            },
            _ => {
                self.problem("base_url", "must be a string");
                None
            }
        }
    }

    fn integer(&mut self, path: &str, value: Value, min: u64, max: u64) -> Option<u64> {
        match value.as_u64() {
            Some(number) if (min..=max).contains(&number) => Some(number),
            Some(_) => {
                self.problem(path, &format!("must be between {min} and {max}"));
                None
            }
            None => {
                self.problem(path, "must be a non-negative integer");
                None
            }
        }
    }

    fn pricing(&mut self, map: &mut Map<String, Value>) -> (Option<u32>, Option<u32>) {
        let Some(value) = map.remove("pricing") else {
            return (None, None);
        };
        let Value::Object(mut pricing) = value else {
            self.problem("pricing", "must be an object");
            return (None, None);
        };
        let mut read = |parser: &mut Self, field: &str| {
            pricing.remove(field).and_then(|value| {
                parser
                    .integer(&format!("pricing.{field}"), value, 0, i32::MAX as u64)
                    .and_then(|number| u32::try_from(number).ok())
            })
        };
        let input = read(self, "input_cents_per_mtok");
        let output = read(self, "output_cents_per_mtok");
        for field in pricing.keys() {
            self.problem(&format!("pricing.{field}"), "unknown field");
        }
        (input, output)
    }
}

fn parse_input(body: &[u8], required: bool) -> Result<ProviderInput, ApiError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| {
        ApiError::Rejected(vec![ErrorDetail {
            path: "$".to_owned(),
            message: "malformed JSON".to_owned(),
        }])
    })?;
    let Value::Object(mut map) = value else {
        return Err(ApiError::Rejected(vec![ErrorDetail {
            path: "$".to_owned(),
            message: "must be a JSON object".to_owned(),
        }]));
    };
    let mut parser = Parser { problems: vec![] };

    let name = parser.text(&mut map, "name", MAX_NAME_CHARS);
    let kind = parser.kind(&mut map);
    let base_url = parser.base_url(&mut map);
    let model = parser.text(&mut map, "model", 200);
    let api_key = match map.remove("api_key") {
        None => KeyInput::Absent,
        Some(Value::Null) => KeyInput::Null,
        Some(Value::String(key)) if !key.trim().is_empty() => KeyInput::Value(key),
        Some(Value::String(_)) => {
            parser.problem("api_key", "must not be empty");
            KeyInput::Absent
        }
        Some(_) => {
            parser.problem("api_key", "must be a string or null");
            KeyInput::Absent
        }
    };
    let max_retries = map
        .remove("max_retries")
        .and_then(|value| parser.integer("max_retries", value, 0, MAX_RETRIES))
        .and_then(|number| u8::try_from(number).ok());
    let (input_cents, output_cents) = parser.pricing(&mut map);
    let timeout = map
        .remove("timeout_seconds")
        .and_then(|value| parser.integer("timeout_seconds", value, 1, MAX_TIMEOUT_SECONDS))
        .map(Duration::from_secs);
    let reasoning_effort = parser.reasoning_effort(&mut map);

    if required {
        for (field, present) in [
            ("name", name.is_some()),
            ("kind", kind.is_some()),
            ("base_url", base_url.is_some()),
            ("model", model.is_some()),
        ] {
            let reported = parser.problems.iter().any(|problem| problem.path == field);
            if !present && !reported {
                parser.problem(field, "is required");
            }
        }
    }
    for field in map.keys() {
        parser.problem(field, "unknown field");
    }

    if parser.problems.is_empty() {
        Ok(ProviderInput {
            name,
            kind,
            base_url,
            model,
            api_key,
            max_retries,
            input_cents,
            output_cents,
            timeout,
            reasoning_effort,
        })
    } else {
        Err(ApiError::Rejected(parser.problems))
    }
}

fn key_secret(input: KeyInput) -> Option<Secret> {
    match input {
        KeyInput::Value(key) => Some(Secret::new(key)),
        KeyInput::Absent | KeyInput::Null => None,
    }
}

fn provider_id(raw: &str) -> Result<ProviderId, ApiError> {
    raw.parse()
        .map_err(|_| ApiError::BadRequest("provider_id: must be a UUID".to_owned()))
}

fn tier_from(raw: &str) -> Result<Tier, ApiError> {
    Tier::parse(raw)
        .ok_or_else(|| ApiError::BadRequest("tier: must be `triage` or `qualification`".to_owned()))
}

fn actor(user: Option<Extension<AuthenticatedUser>>) -> Result<(), ApiError> {
    match user {
        Some(_) => Ok(()),
        None => Err(ApiError::Forbidden),
    }
}

fn write_conflict(error: CoreError) -> ApiError {
    match error {
        CoreError::Conflict(_) => {
            ApiError::Conflict("a provider with this name already exists".to_owned())
        }
        other => other.into(),
    }
}

fn created(location: String, body: ProviderDto) -> HttpResponse {
    match HeaderValue::from_str(&location) {
        Ok(value) => (
            StatusCode::CREATED,
            [(LOCATION, value)],
            Json(DataEnvelope::new(body)),
        )
            .into_response(),
        Err(_) => ApiError::Internal.into_response(),
    }
}

async fn view(state: &AppState, record: &LlmProviderRecord) -> Result<ProviderDto, ApiError> {
    let assignments = state.usecase.list_llm_tiers().await?;
    Ok(ProviderDto::new(record, &assignments))
}

#[utoipa::path(
    get,
    path = "/api/v1/llm/providers",
    tag = "llm",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "LLM providers; API keys are never returned, only whether one is set", body = DataEnvelope<Vec<ProviderDto>>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
    )
)]
pub async fn list_providers(
    State(state): State<AppState>,
) -> Result<Response<Vec<ProviderDto>>, ApiError> {
    let records = state.usecase.list_llm_providers().await?;
    let assignments = state.usecase.list_llm_tiers().await?;
    Ok(Response::OK(
        records
            .iter()
            .map(|record| ProviderDto::new(record, &assignments))
            .collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/llm/providers/{provider_id}",
    tag = "llm",
    security(("bearer_auth" = [])),
    params(("provider_id" = String, Path, description = "Provider identifier (UUID)")),
    responses(
        (status = 200, description = "The provider; the API key is never returned", body = DataEnvelope<ProviderDto>),
        (status = 400, description = "Malformed provider identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 404, description = "Unknown provider"),
    )
)]
pub async fn get_provider(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Response<ProviderDto>, ApiError> {
    let record = state.usecase.get_llm_provider(provider_id(&raw)?).await?;
    Ok(Response::OK(view(&state, &record).await?))
}

#[utoipa::path(
    post,
    path = "/api/v1/llm/providers",
    tag = "llm",
    security(("bearer_auth" = [])),
    request_body = CreateProviderRequest,
    responses(
        (status = 201, description = "The provider, enabled and unassigned; Location points at it", body = DataEnvelope<ProviderDto>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 409, description = "A provider with this name already exists"),
        (status = 422, description = "The payload is invalid; `details` lists the faulty field paths", body = ErrorBody),
    )
)]
pub async fn create_provider(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    body: Bytes,
) -> Result<HttpResponse, ApiError> {
    actor(user)?;
    let input = parse_input(&body, true)?;
    let (Some(name), Some(kind), Some(base_url), Some(model)) =
        (input.name, input.kind, input.base_url, input.model)
    else {
        return Err(ApiError::Internal);
    };
    let record = state
        .usecase
        .create_llm_provider(
            CreateLlmProvider {
                name,
                kind,
                base_url,
                model,
                timeout: input
                    .timeout
                    .unwrap_or(Duration::from_secs(DEFAULT_TIMEOUT_SECONDS)),
                max_retries: input.max_retries.unwrap_or(DEFAULT_MAX_RETRIES),
                pricing: TokenPricing {
                    input_cents_per_mtok: input.input_cents.unwrap_or(0),
                    output_cents_per_mtok: input.output_cents.unwrap_or(0),
                },
                api_key: key_secret(input.api_key),
                reasoning_effort: input.reasoning_effort.flatten(),
            },
            state.cipher.as_ref(),
        )
        .await
        .map_err(write_conflict)?;
    let location = format!("{}/llm/providers/{}", crate::api::API_PREFIX, record.id);
    Ok(created(location, view(&state, &record).await?))
}

#[utoipa::path(
    patch,
    path = "/api/v1/llm/providers/{provider_id}",
    tag = "llm",
    security(("bearer_auth" = [])),
    params(("provider_id" = String, Path, description = "Provider identifier (UUID)")),
    request_body = UpdateProviderRequest,
    responses(
        (status = 200, description = "The updated provider; the API key is never returned", body = DataEnvelope<ProviderDto>),
        (status = 400, description = "Malformed provider identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown provider"),
        (status = 409, description = "A provider with this name already exists"),
        (status = 422, description = "The payload is invalid; `details` lists the faulty field paths", body = ErrorBody),
    )
)]
pub async fn update_provider(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
    body: Bytes,
) -> Result<Response<ProviderDto>, ApiError> {
    actor(user)?;
    let id = provider_id(&raw)?;
    let input = parse_input(&body, false)?;
    let current = state.usecase.get_llm_provider(id).await?;
    let api_key = match input.api_key {
        KeyInput::Absent => KeyChange::Keep,
        KeyInput::Null => KeyChange::Remove,
        KeyInput::Value(key) => KeyChange::Replace(Secret::new(key)),
    };
    let record = state
        .usecase
        .update_llm_provider(
            UpdateLlmProvider {
                id,
                name: input.name.unwrap_or(current.name),
                kind: input.kind.unwrap_or(current.kind),
                base_url: input.base_url.unwrap_or(current.base_url),
                model: input.model.unwrap_or(current.model),
                timeout: input.timeout.unwrap_or(current.timeout),
                max_retries: input.max_retries.unwrap_or(current.max_retries),
                pricing: TokenPricing {
                    input_cents_per_mtok: input
                        .input_cents
                        .unwrap_or(current.pricing.input_cents_per_mtok),
                    output_cents_per_mtok: input
                        .output_cents
                        .unwrap_or(current.pricing.output_cents_per_mtok),
                },
                api_key,
                reasoning_effort: input.reasoning_effort.unwrap_or(current.reasoning_effort),
            },
            state.cipher.as_ref(),
        )
        .await
        .map_err(write_conflict)?;
    Ok(Response::OK(view(&state, &record).await?))
}

async fn set_enabled(
    state: &AppState,
    user: Option<Extension<AuthenticatedUser>>,
    raw: &str,
    enabled: bool,
) -> Result<Response<ProviderDto>, ApiError> {
    actor(user)?;
    let record = state
        .usecase
        .set_llm_provider_enabled(provider_id(raw)?, enabled)
        .await?;
    Ok(Response::OK(view(state, &record).await?))
}

#[utoipa::path(
    post,
    path = "/api/v1/llm/providers/{provider_id}/enable",
    tag = "llm",
    security(("bearer_auth" = [])),
    params(("provider_id" = String, Path, description = "Provider identifier (UUID)")),
    responses(
        (status = 200, description = "The provider, enabled", body = DataEnvelope<ProviderDto>),
        (status = 400, description = "Malformed provider identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown provider"),
    )
)]
pub async fn enable_provider(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
) -> Result<Response<ProviderDto>, ApiError> {
    set_enabled(&state, user, &raw, true).await
}

#[utoipa::path(
    post,
    path = "/api/v1/llm/providers/{provider_id}/disable",
    tag = "llm",
    security(("bearer_auth" = [])),
    params(("provider_id" = String, Path, description = "Provider identifier (UUID)")),
    responses(
        (status = 200, description = "The provider, disabled", body = DataEnvelope<ProviderDto>),
        (status = 400, description = "Malformed provider identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown provider"),
    )
)]
pub async fn disable_provider(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
) -> Result<Response<ProviderDto>, ApiError> {
    set_enabled(&state, user, &raw, false).await
}

#[utoipa::path(
    delete,
    path = "/api/v1/llm/providers/{provider_id}",
    tag = "llm",
    security(("bearer_auth" = [])),
    params(("provider_id" = String, Path, description = "Provider identifier (UUID)")),
    responses(
        (status = 204, description = "The provider was deleted"),
        (status = 400, description = "Malformed provider identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown provider"),
        (status = 409, description = "The provider is assigned to a tier", body = ErrorBody),
    )
)]
pub async fn delete_provider(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
) -> Result<Response<()>, ApiError> {
    actor(user)?;
    state
        .usecase
        .delete_llm_provider(provider_id(&raw)?)
        .await?;
    Ok(Response::NoContent)
}

#[utoipa::path(
    get,
    path = "/api/v1/llm/tiers",
    tag = "llm",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Each tier with its assigned provider, or null", body = DataEnvelope<Vec<TierDto>>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
    )
)]
pub async fn list_tiers(State(state): State<AppState>) -> Result<Response<Vec<TierDto>>, ApiError> {
    let assignments = state.usecase.list_llm_tiers().await?;
    Ok(Response::OK(
        [Tier::Triage, Tier::Qualification]
            .into_iter()
            .map(|tier| tier_dto(tier, &assignments))
            .collect(),
    ))
}

fn tier_dto(tier: Tier, assignments: &[(Tier, ProviderId)]) -> TierDto {
    TierDto {
        tier: tier.as_str().to_owned(),
        provider_id: assignments
            .iter()
            .find(|(assigned, _)| *assigned == tier)
            .map(|(_, provider)| provider.to_string()),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/llm/tiers/{tier}",
    tag = "llm",
    security(("bearer_auth" = [])),
    params(("tier" = String, Path, description = "`triage` or `qualification`")),
    request_body = AssignTierRequest,
    responses(
        (status = 200, description = "The tier with its new provider", body = DataEnvelope<TierDto>),
        (status = 400, description = "Unknown tier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 422, description = "The provider identifier is malformed or unknown", body = ErrorBody),
    )
)]
pub async fn assign_tier(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw_tier): Path<String>,
    body: Bytes,
) -> Result<Response<TierDto>, ApiError> {
    actor(user)?;
    let tier = tier_from(&raw_tier)?;
    let reject = |message: &str| {
        ApiError::Rejected(vec![ErrorDetail {
            path: "provider_id".to_owned(),
            message: message.to_owned(),
        }])
    };
    let request: Value = serde_json::from_slice(&body).map_err(|_| {
        ApiError::Rejected(vec![ErrorDetail {
            path: "$".to_owned(),
            message: "malformed JSON".to_owned(),
        }])
    })?;
    let provider = match request.get("provider_id") {
        None => return Err(reject("is required")),
        Some(Value::String(raw)) => raw
            .parse::<ProviderId>()
            .map_err(|_| reject("must be a UUID"))?,
        Some(_) => return Err(reject("must be a string")),
    };
    state
        .usecase
        .assign_llm_tier(tier, provider)
        .await
        .map_err(|error| match error {
            CoreError::NotFound => reject("unknown provider"),
            other => other.into(),
        })?;
    Ok(Response::OK(TierDto {
        tier: tier.as_str().to_owned(),
        provider_id: Some(provider.to_string()),
    }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/llm/tiers/{tier}",
    tag = "llm",
    security(("bearer_auth" = [])),
    params(("tier" = String, Path, description = "`triage` or `qualification`")),
    responses(
        (status = 204, description = "The tier is now unassigned"),
        (status = 400, description = "Unknown tier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "The tier has no provider assigned"),
    )
)]
pub async fn unassign_tier(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw_tier): Path<String>,
) -> Result<Response<()>, ApiError> {
    actor(user)?;
    state
        .usecase
        .unassign_llm_tier(tier_from(&raw_tier)?)
        .await?;
    Ok(Response::NoContent)
}

fn failure(kind: &str, message: &str, key: Option<&Secret>) -> TestResultDto {
    TestResultDto {
        ok: false,
        model: None,
        latency_ms: None,
        error: Some(TestErrorDto {
            kind: kind.to_owned(),
            message: scrub(message, key),
        }),
    }
}

fn scrub(text: &str, key: Option<&Secret>) -> String {
    match key {
        Some(secret) if !secret.expose().is_empty() => text.replace(secret.expose(), REDACTED),
        _ => text.to_owned(),
    }
}

fn classify(error: &LlmError, key: Option<&Secret>) -> TestResultDto {
    match error {
        LlmError::Timeout => failure("timeout", "The provider did not answer in time.", key),
        LlmError::RateLimited { .. } => failure(
            "rate_limited",
            "The provider rate limited the request.",
            key,
        ),
        LlmError::Status { status: 401 | 403 } => failure(
            "authentication",
            "The provider rejected the credentials.",
            key,
        ),
        LlmError::Status { status } => failure(
            "other",
            &format!("The provider answered with status {status}."),
            key,
        ),
        LlmError::Network(_) => failure("unreachable", "The endpoint could not be reached.", key),
        LlmError::ReasoningExhausted => failure(
            "reasoning_exhausted",
            "The model used all its tokens to reason; lower its reasoning effort.",
            key,
        ),
        LlmError::InvalidOutput(_) | LlmError::Truncated | LlmError::Refused => failure(
            "invalid_response",
            "The provider answered with an unusable response.",
            key,
        ),
        LlmError::Configuration(_) => {
            failure("other", "The provider configuration is not usable.", key)
        }
    }
}

async fn run_test(record: &LlmProviderRecord, cipher: &impl SecretCipher) -> TestResultDto {
    let api_key = match record
        .api_key
        .as_ref()
        .map(|sealed| cipher.open(sealed, record.id.0.as_bytes()))
        .transpose()
    {
        Ok(key) => key,
        Err(_) => {
            return failure(
                "other",
                "The stored API key could not be decrypted; set it again.",
                None,
            );
        }
    };
    let definition = ProviderDefinition {
        kind: record.kind,
        base_url: record.base_url.clone(),
        model: record.model.clone(),
        api_key,
        timeout: record.timeout.min(TEST_TIMEOUT),
        reasoning_effort: record.reasoning_effort,
    };
    let key = definition.api_key.as_ref();
    let provider = match AnyProvider::from_definition(&definition) {
        Ok(provider) => provider,
        Err(error) => return classify(&error, key),
    };
    let schema = json!({
        "type": "object",
        "properties": { "ok": { "type": "boolean" } },
        "required": ["ok"],
        "additionalProperties": false,
    });
    let request = LlmRequest {
        system: "You are a connectivity check. Answer with the requested JSON only.",
        instruction: "Return {\"ok\": true}.",
        documents: &[],
        schema_name: "connectivity_check",
        schema: &schema,
        max_tokens: TEST_MAX_TOKENS,
    };
    let started = Instant::now();
    match provider.complete(&request).await {
        Ok(response) if response.output.get("ok").is_some_and(Value::is_boolean) => TestResultDto {
            ok: true,
            model: Some(scrub(&response.model, key)),
            latency_ms: Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)),
            error: None,
        },
        Ok(_) => failure(
            "invalid_response",
            "The provider answered with an unusable response.",
            key,
        ),
        Err(error) => classify(&error, key),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/llm/providers/{provider_id}/test",
    tag = "llm",
    security(("bearer_auth" = [])),
    params(("provider_id" = String, Path, description = "Provider identifier (UUID)")),
    responses(
        (status = 200, description = "Outcome of one minimal structured request, without retry, cache or budget charge; failures are reported in the body and never echo the key", body = DataEnvelope<TestResultDto>),
        (status = 400, description = "Malformed provider identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown provider"),
    )
)]
pub async fn test_provider(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
) -> Result<Response<TestResultDto>, ApiError> {
    actor(user)?;
    let record = state.usecase.get_llm_provider(provider_id(&raw)?).await?;
    Ok(Response::OK(run_test(&record, state.cipher.as_ref()).await))
}
