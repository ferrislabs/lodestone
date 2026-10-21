use std::time::Duration;

use chrono::{DateTime, Utc};
use common::CoreError;
use url::Url;
use uuid::Uuid;

use crate::domain::llm::{
    GenerationParams, ProviderKind, ReasoningEffort,
    provider::{LlmProviderRecord, MaxConcurrency, ProviderId, SealedSecret, TokenPricing},
};

#[derive(Debug, Clone)]
pub struct LlmProviderRow {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub timeout_ms: i32,
    pub max_retries: i16,
    pub max_concurrency: i16,
    pub input_cents_per_mtok: i32,
    pub output_cents_per_mtok: i32,
    pub api_key_nonce: Option<Vec<u8>>,
    pub api_key_ciphertext: Option<Vec<u8>>,
    pub enabled: bool,
    pub reasoning_effort: Option<String>,
    pub max_tokens: Option<i32>,
    pub temperature: Option<f64>,
    pub frequency_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    pub repeat_penalty: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn pricing_value(value: i32) -> Result<u32, CoreError> {
    u32::try_from(value)
        .map_err(|_| CoreError::Internal("stored llm provider pricing is invalid".to_owned()))
}

impl TryFrom<LlmProviderRow> for LlmProviderRecord {
    type Error = CoreError;

    fn try_from(row: LlmProviderRow) -> Result<Self, Self::Error> {
        let kind = ProviderKind::parse(&row.kind)
            .ok_or_else(|| CoreError::Internal("stored llm provider kind is invalid".to_owned()))?;
        let base_url = Url::parse(&row.base_url)
            .map_err(|_| CoreError::Internal("stored llm provider url is invalid".to_owned()))?;
        let timeout_ms = u64::try_from(row.timeout_ms).map_err(|_| {
            CoreError::Internal("stored llm provider timeout is invalid".to_owned())
        })?;
        let max_retries = u8::try_from(row.max_retries).map_err(|_| {
            CoreError::Internal("stored llm provider retry count is invalid".to_owned())
        })?;
        let max_concurrency = u8::try_from(row.max_concurrency)
            .ok()
            .and_then(MaxConcurrency::new)
            .ok_or_else(|| {
                CoreError::Internal("stored llm provider concurrency is invalid".to_owned())
            })?;
        let api_key = match (row.api_key_nonce, row.api_key_ciphertext) {
            (Some(nonce), Some(ciphertext)) => Some(SealedSecret { nonce, ciphertext }),
            (None, None) => None,
            _ => {
                return Err(CoreError::Internal(
                    "stored llm provider key is inconsistent".to_owned(),
                ));
            }
        };
        let reasoning_effort = row
            .reasoning_effort
            .as_deref()
            .map(|raw| {
                ReasoningEffort::parse(raw).ok_or_else(|| {
                    CoreError::Internal(
                        "stored llm provider reasoning effort is invalid".to_owned(),
                    )
                })
            })
            .transpose()?;
        Ok(Self {
            id: ProviderId(row.id),
            name: row.name,
            kind,
            base_url,
            model: row.model,
            timeout: Duration::from_millis(timeout_ms),
            max_retries,
            max_concurrency,
            pricing: TokenPricing {
                input_cents_per_mtok: pricing_value(row.input_cents_per_mtok)?,
                output_cents_per_mtok: pricing_value(row.output_cents_per_mtok)?,
            },
            api_key,
            enabled: row.enabled,
            reasoning_effort,
            generation: GenerationParams {
                max_tokens: row.max_tokens.map(u32::try_from).transpose().map_err(|_| {
                    CoreError::Internal("stored llm provider max tokens is invalid".to_owned())
                })?,
                temperature: row.temperature,
                frequency_penalty: row.frequency_penalty,
                presence_penalty: row.presence_penalty,
                repeat_penalty: row.repeat_penalty,
            },
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

pub fn timeout_ms(timeout: Duration) -> Result<i32, CoreError> {
    i32::try_from(timeout.as_millis())
        .map_err(|_| CoreError::Internal("llm provider timeout out of range".to_owned()))
}

pub fn pricing_cents(value: u32) -> Result<i32, CoreError> {
    i32::try_from(value)
        .map_err(|_| CoreError::Internal("llm provider pricing out of range".to_owned()))
}
