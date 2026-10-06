use std::time::Duration;

use lodestone_core::{
    GenerationParams, LlmError, LlmProvider, LlmRequest, LlmResponse, ProviderDefinition,
    ProviderKind, TokenUsage, render_user_content,
};
use reqwest::{Client, StatusCode, header::RETRY_AFTER};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{Instrument, Span, field, info_span};
use url::Url;

const API_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    client: Client,
    endpoint: Url,
    model: String,
    api_key: common::Secret,
    generation: GenerationParams,
}

#[derive(Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: [WireMessage<'a>; 1],
    output_config: OutputConfig<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct OutputConfig<'a> {
    format: OutputFormat<'a>,
}

#[derive(Serialize)]
struct OutputFormat<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    schema: &'a Value,
}

#[derive(Deserialize)]
struct WireResponse {
    model: String,
    #[serde(default)]
    content: Vec<WireBlock>,
    stop_reason: Option<String>,
    usage: WireUsage,
}

#[derive(Deserialize)]
struct WireBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct WireUsage {
    input_tokens: u64,
    output_tokens: u64,
}

impl AnthropicProvider {
    pub fn new(definition: ProviderDefinition) -> Result<Self, LlmError> {
        if definition.kind != ProviderKind::Anthropic {
            return Err(LlmError::Configuration(
                "provider kind is not anthropic".to_owned(),
            ));
        }
        let api_key = definition
            .api_key
            .ok_or_else(|| LlmError::Configuration("anthropic api key is missing".to_owned()))?;
        let endpoint = definition
            .base_url
            .join("v1/messages")
            .map_err(|_| LlmError::Configuration("base url cannot be extended".to_owned()))?;
        let client = Client::builder()
            .timeout(definition.timeout)
            .build()
            .map_err(|_| LlmError::Configuration("http client cannot be built".to_owned()))?;
        Ok(Self {
            client,
            endpoint,
            model: definition.model,
            api_key,
            generation: definition.generation,
        })
    }

    async fn call(&self, request: &LlmRequest<'_>, span: &Span) -> Result<LlmResponse, LlmError> {
        let content = render_user_content(request);
        let body = WireRequest {
            model: &self.model,
            max_tokens: self.generation.max_tokens_or(request.max_tokens),
            system: request.system,
            messages: [WireMessage {
                role: "user",
                content: &content,
            }],
            output_config: OutputConfig {
                format: OutputFormat {
                    kind: "json_schema",
                    schema: request.schema,
                },
            },
            temperature: request.temperature.or(self.generation.temperature),
        };
        let response = self
            .client
            .post(self.endpoint.clone())
            .header("x-api-key", self.api_key.expose())
            .header("anthropic-version", API_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(transport_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(status_error(status, &response));
        }
        let bytes = response.bytes().await.map_err(transport_error)?;
        let wire: WireResponse = serde_json::from_slice(&bytes)
            .map_err(|error| LlmError::InvalidOutput(format!("malformed response: {error}")))?;
        match wire.stop_reason.as_deref() {
            Some("refusal") => return Err(LlmError::Refused),
            Some("max_tokens") => return Err(LlmError::Truncated),
            _ => {}
        }
        let text: String = wire
            .content
            .iter()
            .filter(|block| block.kind == "text")
            .filter_map(|block| block.text.as_deref())
            .collect();
        let output = serde_json::from_str(&text)
            .map_err(|error| LlmError::InvalidOutput(format!("output is not json: {error}")))?;
        span.record("input_tokens", wire.usage.input_tokens);
        span.record("output_tokens", wire.usage.output_tokens);
        Ok(LlmResponse {
            output,
            model: wire.model,
            usage: TokenUsage {
                input_tokens: wire.usage.input_tokens,
                output_tokens: wire.usage.output_tokens,
            },
        })
    }
}

fn status_error(status: StatusCode, response: &reqwest::Response) -> LlmError {
    match status.as_u16() {
        429 => LlmError::RateLimited {
            retry_after: response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok())
                .map(Duration::from_secs),
        },
        529 => LlmError::RateLimited { retry_after: None },
        code => LlmError::Status { status: code },
    }
}

fn transport_error(error: reqwest::Error) -> LlmError {
    if error.is_timeout() {
        return LlmError::Timeout;
    }
    let kind = if error.is_connect() {
        "connection failed"
    } else if error.is_body() || error.is_decode() {
        "response body failed"
    } else if error.is_request() {
        "request failed"
    } else {
        "transport failed"
    };
    LlmError::Network(kind.to_owned())
}

impl LlmProvider for AnthropicProvider {
    async fn complete(&self, request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        let span = info_span!(
            "llm.anthropic.complete",
            model = %self.model,
            input_tokens = field::Empty,
            output_tokens = field::Empty,
        );
        self.call(request, &span).instrument(span.clone()).await
    }
}

#[cfg(test)]
mod tests;
