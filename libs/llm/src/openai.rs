use std::time::Duration;

use common::Secret;
use lodestone_core::{
    GenerationParams, LlmError, LlmProvider, LlmRequest, LlmResponse, ProviderDefinition,
    ProviderKind, ReasoningEffort, TokenUsage, render_user_content,
};
use reqwest::{Client, StatusCode, header};
use serde::{Deserialize, Serialize};
use tracing::{Instrument, field};

const COMPLETIONS_PATH: &str = "/v1/chat/completions";

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleProvider {
    client: Client,
    endpoint: String,
    model: String,
    api_key: Option<Secret>,
    kind: ProviderKind,
    reasoning_effort: Option<ReasoningEffort>,
    generation: GenerationParams,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: [Message<'a>; 2],
    response_format: ResponseFormat<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<ReasoningEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f64>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct ResponseFormat<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    json_schema: JsonSchema<'a>,
}

#[derive(Serialize)]
struct JsonSchema<'a> {
    name: &'a str,
    schema: &'a serde_json::Value,
    strict: bool,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    message: Option<ChoiceMessage>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    refusal: Option<String>,
    #[serde(default, alias = "reasoning_content")]
    reasoning: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

impl OpenAiCompatibleProvider {
    pub fn new(definition: &ProviderDefinition) -> Result<Self, LlmError> {
        match definition.kind {
            ProviderKind::Mistral if definition.api_key.is_none() => {
                return Err(LlmError::Configuration(
                    "mistral requires an api key".to_owned(),
                ));
            }
            ProviderKind::Mistral | ProviderKind::OpenAiCompatible => {}
            ProviderKind::Anthropic => {
                return Err(LlmError::Configuration(
                    "unsupported provider kind for the openai adapter".to_owned(),
                ));
            }
        }
        let client = Client::builder()
            .timeout(definition.timeout)
            .build()
            .map_err(|_| LlmError::Configuration("http client could not be built".to_owned()))?;
        Ok(Self {
            client,
            endpoint: format!(
                "{}{COMPLETIONS_PATH}",
                definition.base_url.as_str().trim_end_matches('/')
            ),
            model: definition.model.clone(),
            api_key: definition.api_key.clone(),
            kind: definition.kind,
            reasoning_effort: definition.reasoning_effort,
            generation: definition.generation,
        })
    }

    async fn send(&self, request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        let user = render_user_content(request);
        let body = ChatRequest {
            model: &self.model,
            max_tokens: self.generation.max_tokens_or(request.max_tokens),
            messages: [
                Message {
                    role: "system",
                    content: request.system,
                },
                Message {
                    role: "user",
                    content: &user,
                },
            ],
            response_format: ResponseFormat {
                kind: "json_schema",
                json_schema: JsonSchema {
                    name: request.schema_name,
                    schema: request.schema,
                    strict: true,
                },
            },
            reasoning_effort: self.reasoning_effort,
            temperature: request.temperature.or(self.generation.temperature),
            frequency_penalty: self.generation.frequency_penalty_for_openai(),
            presence_penalty: self.generation.presence_penalty,
        };
        let mut builder = self.client.post(&self.endpoint).json(&body);
        if let Some(key) = &self.api_key {
            let mut value = header::HeaderValue::try_from(format!("Bearer {}", key.expose()))
                .map_err(|_| LlmError::Configuration("api key is not a valid header".to_owned()))?;
            value.set_sensitive(true);
            builder = builder.header(header::AUTHORIZATION, value);
        }
        let response = builder.send().await.map_err(transport_error)?;
        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            return Err(LlmError::RateLimited { retry_after });
        }
        if !status.is_success() {
            return Err(LlmError::Status {
                status: status.as_u16(),
            });
        }
        let parsed: ChatResponse = response.json().await.map_err(|error| {
            if error.is_timeout() {
                LlmError::Timeout
            } else if error.is_decode() {
                LlmError::InvalidOutput("response is not a chat completion".to_owned())
            } else {
                transport_error(error)
            }
        })?;
        interpret(parsed)
    }
}

fn transport_error(error: reqwest::Error) -> LlmError {
    if error.is_timeout() {
        LlmError::Timeout
    } else if error.is_connect() {
        LlmError::Network("connection failed".to_owned())
    } else {
        LlmError::Network("request failed".to_owned())
    }
}

fn exhausted_by_reasoning(message: Option<&ChoiceMessage>) -> bool {
    message.is_some_and(|message| {
        message.content.as_deref().is_none_or(str::is_empty)
            && message
                .reasoning
                .as_deref()
                .is_some_and(|text| !text.is_empty())
    })
}

fn interpret(response: ChatResponse) -> Result<LlmResponse, LlmError> {
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| LlmError::InvalidOutput("response has no choice".to_owned()))?;
    match choice.finish_reason.as_deref() {
        Some("length") => {
            return Err(if exhausted_by_reasoning(choice.message.as_ref()) {
                LlmError::ReasoningExhausted
            } else {
                LlmError::Truncated
            });
        }
        Some("content_filter") => return Err(LlmError::Refused),
        _ => {}
    }
    let message = choice
        .message
        .ok_or_else(|| LlmError::InvalidOutput("choice has no message".to_owned()))?;
    if message
        .refusal
        .as_deref()
        .is_some_and(|text| !text.is_empty())
    {
        return Err(LlmError::Refused);
    }
    let content = message
        .content
        .ok_or_else(|| LlmError::InvalidOutput("message has no content".to_owned()))?;
    let output = serde_json::from_str(&content)
        .map_err(|_| LlmError::InvalidOutput("content is not valid json".to_owned()))?;
    let usage = response
        .usage
        .map_or_else(TokenUsage::default, |usage| TokenUsage {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
        });
    Ok(LlmResponse {
        output,
        model: response.model,
        usage,
    })
}

impl LlmProvider for OpenAiCompatibleProvider {
    async fn complete(&self, request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        let span = tracing::info_span!(
            "llm.complete",
            provider = self.kind.as_str(),
            model = %self.model,
            input_tokens = field::Empty,
            output_tokens = field::Empty,
        );
        let recorder = span.clone();
        async move {
            let response = self.send(request).await?;
            recorder.record("input_tokens", response.usage.input_tokens);
            recorder.record("output_tokens", response.usage.output_tokens);
            Ok(response)
        }
        .instrument(span)
        .await
    }
}

#[cfg(test)]
mod tests;
