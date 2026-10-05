use std::{
    fmt::Write as _,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use common::Secret;
use httpmock::prelude::*;
use lodestone_core::{
    LlmError, LlmProvider, LlmRequest, ProviderDefinition, ProviderKind, ReasoningEffort,
    UntrustedDocument,
};
use serde_json::json;
use tracing::{
    Event, Metadata, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};
use url::Url;

use super::OpenAiCompatibleProvider;

const KEY: &str = "sk-very-secret-key";

fn definition(
    kind: ProviderKind,
    base: &str,
    key: Option<&str>,
    timeout: Duration,
) -> ProviderDefinition {
    ProviderDefinition {
        kind,
        base_url: Url::parse(base).unwrap(),
        model: "test-model".to_owned(),
        api_key: key.map(Secret::new),
        timeout,
        reasoning_effort: None,
    }
}

fn provider(
    kind: ProviderKind,
    server: &MockServer,
    key: Option<&str>,
) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(&definition(
        kind,
        &server.base_url(),
        key,
        Duration::from_secs(5),
    ))
    .unwrap()
}

fn schema() -> serde_json::Value {
    json!({"type": "object", "properties": {"ok": {"type": "boolean"}}, "required": ["ok"]})
}

static SERIAL: Mutex<()> = Mutex::new(());

#[allow(clippy::await_holding_lock)]
async fn run(provider: &OpenAiCompatibleProvider) -> Result<lodestone_core::LlmResponse, LlmError> {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    run_unlocked(provider).await
}

async fn run_unlocked(
    provider: &OpenAiCompatibleProvider,
) -> Result<lodestone_core::LlmResponse, LlmError> {
    let schema = schema();
    let documents = [UntrustedDocument {
        source: "https://a.example",
        content: "hello",
    }];
    provider
        .complete(&LlmRequest {
            system: "be strict",
            instruction: "classify",
            documents: &documents,
            schema_name: "verdict",
            schema: &schema,
            max_tokens: 256,
        })
        .await
}

fn expected_body() -> serde_json::Value {
    json!({
        "model": "test-model",
        "max_tokens": 256,
        "messages": [
            {"role": "system", "content": "be strict"},
            {"role": "user", "content": "<document index=\"0\" source=\"https://a.example\">\nhello\n</document>\nclassify"}
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": {"name": "verdict", "schema": schema(), "strict": true}
        }
    })
}

fn success_body() -> serde_json::Value {
    json!({
        "model": "served-model",
        "choices": [{
            "message": {"role": "assistant", "content": "{\"ok\": true}"},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 11, "completion_tokens": 7}
    })
}

fn choice_body(message: serde_json::Value, finish: &str) -> serde_json::Value {
    json!({"model": "m", "choices": [{"message": message, "finish_reason": finish}]})
}

#[tokio::test]
async fn mistral_request_has_exact_shape_and_bearer() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", format!("Bearer {KEY}"))
            .header("content-type", "application/json")
            .json_body(expected_body());
        then.status(200).json_body(success_body());
    });
    let response = run(&provider(ProviderKind::Mistral, &server, Some(KEY)))
        .await
        .unwrap();
    mock.assert();
    assert_eq!(response.output, json!({"ok": true}));
    assert_eq!(response.model, "served-model");
    assert_eq!(response.usage.input_tokens, 11);
    assert_eq!(response.usage.output_tokens, 7);
}

#[tokio::test]
async fn keyless_request_has_no_authorization_and_no_tools() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .matches(|request| {
                request.headers.as_ref().is_none_or(|headers| {
                    headers
                        .iter()
                        .all(|(name, _)| !name.eq_ignore_ascii_case("authorization"))
                })
            })
            .json_body(expected_body());
        then.status(200).json_body(success_body());
    });
    run(&provider(ProviderKind::OpenAiCompatible, &server, None))
        .await
        .unwrap();
    mock.assert();
    let body = expected_body();
    assert!(body.get("tools").is_none());
    assert!(body.get("tool_choice").is_none());
}

#[tokio::test]
async fn base_url_with_trailing_slash_and_prefix_is_joined() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/proxy/v1/chat/completions");
        then.status(200).json_body(success_body());
    });
    let base = format!("{}/proxy/", server.base_url());
    let provider = OpenAiCompatibleProvider::new(&definition(
        ProviderKind::OpenAiCompatible,
        &base,
        None,
        Duration::from_secs(5),
    ))
    .unwrap();
    run(&provider).await.unwrap();
    mock.assert();
}

#[tokio::test]
async fn missing_usage_yields_zeros() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST);
        then.status(200)
            .json_body(choice_body(json!({"content": "{\"ok\": false}"}), "stop"));
    });
    let response = run(&provider(ProviderKind::OpenAiCompatible, &server, None))
        .await
        .unwrap();
    assert_eq!(response.usage.input_tokens, 0);
    assert_eq!(response.usage.output_tokens, 0);
}

async fn error_for(body: serde_json::Value) -> LlmError {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST);
        then.status(200).json_body(body);
    });
    run(&provider(ProviderKind::OpenAiCompatible, &server, None))
        .await
        .unwrap_err()
}

#[tokio::test]
async fn finish_reason_length_is_truncated() {
    let error = error_for(choice_body(json!({"content": "{\"ok\":"}), "length")).await;
    assert_eq!(error, LlmError::Truncated);
}

#[tokio::test]
async fn finish_reason_content_filter_is_refused() {
    let error = error_for(choice_body(json!({"content": null}), "content_filter")).await;
    assert_eq!(error, LlmError::Refused);
}

#[tokio::test]
async fn refusal_field_is_refused() {
    let error = error_for(choice_body(
        json!({"content": null, "refusal": "I cannot help"}),
        "stop",
    ))
    .await;
    assert_eq!(error, LlmError::Refused);
}

#[tokio::test]
async fn invalid_json_content_is_invalid_output() {
    let error = error_for(choice_body(
        json!({"content": "not json secret-body"}),
        "stop",
    ))
    .await;
    assert!(matches!(error, LlmError::InvalidOutput(_)));
    assert!(!format!("{error}").contains("secret-body"));
}

#[tokio::test]
async fn empty_choices_is_invalid_output() {
    let error = error_for(json!({"model": "m", "choices": []})).await;
    assert!(matches!(error, LlmError::InvalidOutput(_)));
}

#[tokio::test]
async fn rate_limit_reads_retry_after_seconds() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST);
        then.status(429)
            .header("retry-after", "12")
            .body("slow down");
    });
    let error = run(&provider(ProviderKind::Mistral, &server, Some(KEY)))
        .await
        .unwrap_err();
    assert_eq!(
        error,
        LlmError::RateLimited {
            retry_after: Some(Duration::from_secs(12))
        }
    );
}

#[tokio::test]
async fn rate_limit_without_header_has_no_delay() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST);
        then.status(429);
    });
    let error = run(&provider(ProviderKind::Mistral, &server, Some(KEY)))
        .await
        .unwrap_err();
    assert_eq!(error, LlmError::RateLimited { retry_after: None });
}

#[tokio::test]
async fn server_error_is_status_without_body() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST);
        then.status(500).body("internal leak");
    });
    let error = run(&provider(ProviderKind::Mistral, &server, Some(KEY)))
        .await
        .unwrap_err();
    assert_eq!(error, LlmError::Status { status: 500 });
    assert!(!format!("{error}").contains("leak"));
}

#[tokio::test]
async fn slow_server_times_out() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST);
        then.status(200)
            .delay(Duration::from_millis(1500))
            .json_body(success_body());
    });
    let provider = OpenAiCompatibleProvider::new(&definition(
        ProviderKind::OpenAiCompatible,
        &server.base_url(),
        None,
        Duration::from_millis(200),
    ))
    .unwrap();
    assert_eq!(run(&provider).await.unwrap_err(), LlmError::Timeout);
}

#[tokio::test]
async fn unreachable_server_is_network_error() {
    let provider = OpenAiCompatibleProvider::new(&definition(
        ProviderKind::OpenAiCompatible,
        "http://127.0.0.1:1",
        Some(KEY),
        Duration::from_secs(2),
    ))
    .unwrap();
    let error = run(&provider).await.unwrap_err();
    assert!(matches!(error, LlmError::Network(_)));
    assert!(!format!("{error} {error:?}").contains(KEY));
}

#[test]
fn constructor_enforces_kind_and_key_rules() {
    let timeout = Duration::from_secs(1);
    let base = "http://localhost:1";
    assert!(matches!(
        OpenAiCompatibleProvider::new(&definition(
            ProviderKind::Anthropic,
            base,
            Some(KEY),
            timeout
        )),
        Err(LlmError::Configuration(_))
    ));
    assert!(matches!(
        OpenAiCompatibleProvider::new(&definition(ProviderKind::Mistral, base, None, timeout)),
        Err(LlmError::Configuration(_))
    ));
    assert!(
        OpenAiCompatibleProvider::new(&definition(ProviderKind::Mistral, base, Some(KEY), timeout))
            .is_ok()
    );
    assert!(
        OpenAiCompatibleProvider::new(&definition(
            ProviderKind::OpenAiCompatible,
            base,
            None,
            timeout
        ))
        .is_ok()
    );
}

#[test]
fn provider_debug_hides_the_key() {
    let provider = OpenAiCompatibleProvider::new(&definition(
        ProviderKind::Mistral,
        "http://localhost:1",
        Some(KEY),
        Duration::from_secs(1),
    ))
    .unwrap();
    assert!(!format!("{provider:?}").contains(KEY));
}

#[derive(Clone, Default)]
struct Capture {
    buffer: Arc<Mutex<String>>,
    next: Arc<AtomicU64>,
}

struct Writer<'a>(&'a mut String);

impl Visit for Writer<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self.0, " {}={:?}", field.name(), value);
    }
}

impl Capture {
    fn push(&self, name: &str, record: impl FnOnce(&mut Writer<'_>)) {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.push_str(name);
        record(&mut Writer(&mut buffer));
        buffer.push('\n');
    }
}

impl Subscriber for Capture {
    fn register_callsite(&self, _: &'static Metadata<'static>) -> tracing::subscriber::Interest {
        tracing::subscriber::Interest::always()
    }

    fn max_level_hint(&self) -> Option<tracing::metadata::LevelFilter> {
        Some(tracing::metadata::LevelFilter::TRACE)
    }

    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, attributes: &Attributes<'_>) -> Id {
        self.push(attributes.metadata().name(), |writer| {
            attributes.record(writer)
        });
        Id::from_u64(self.next.fetch_add(1, Ordering::SeqCst) + 1)
    }

    fn record(&self, _: &Id, values: &Record<'_>) {
        self.push("record", |writer| values.record(writer));
    }

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        self.push(event.metadata().name(), |writer| event.record(writer));
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn tracing_records_model_and_usage_but_never_secrets() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let capture = Capture::default();
    let _guard = tracing::subscriber::set_default(capture.clone());
    tracing::callsite::rebuild_interest_cache();
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST);
        then.status(200).json_body(success_body());
    });
    let ok = run_unlocked(&provider(ProviderKind::Mistral, &server, Some(KEY))).await;
    assert!(ok.is_ok());
    let failing = MockServer::start();
    failing.mock(|when, then| {
        when.method(POST);
        then.status(500).body("leaky-body");
    });
    assert!(
        run_unlocked(&provider(ProviderKind::Mistral, &failing, Some(KEY)))
            .await
            .is_err()
    );
    let captured = capture.buffer.lock().unwrap().clone();
    assert!(captured.contains("test-model"), "{captured}");
    assert!(captured.contains("input_tokens=11"));
    assert!(captured.contains("output_tokens=7"));
    assert!(!captured.contains(KEY));
    assert!(!captured.contains("hello"));
    assert!(!captured.contains("leaky-body"));
}

const OLLAMA_EXHAUSTED: &str = include_str!("fixtures/ollama_reasoning_exhausted.json");

fn provider_with_effort(server: &MockServer, effort: ReasoningEffort) -> OpenAiCompatibleProvider {
    let mut definition = definition(
        ProviderKind::OpenAiCompatible,
        &server.base_url(),
        None,
        Duration::from_secs(5),
    );
    definition.reasoning_effort = Some(effort);
    OpenAiCompatibleProvider::new(&definition).unwrap()
}

#[tokio::test]
async fn reasoning_effort_is_sent_when_set() {
    for effort in [
        ReasoningEffort::None,
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
    ] {
        let server = MockServer::start();
        let mut expected = expected_body();
        expected["reasoning_effort"] = json!(effort.as_str());
        let mock = server.mock(|when, then| {
            when.method(POST).json_body(expected);
            then.status(200).json_body(success_body());
        });
        run(&provider_with_effort(&server, effort)).await.unwrap();
        mock.assert();
    }
}

#[tokio::test]
async fn reasoning_effort_is_absent_when_unset() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .matches(|request| {
                request
                    .body
                    .as_ref()
                    .is_some_and(|body| !String::from_utf8_lossy(body).contains("reasoning_effort"))
            })
            .json_body(expected_body());
        then.status(200).json_body(success_body());
    });
    run(&provider(ProviderKind::OpenAiCompatible, &server, None))
        .await
        .unwrap();
    mock.assert();
}

#[tokio::test]
async fn empty_content_with_reasoning_and_length_is_reasoning_exhausted() {
    let body: serde_json::Value = serde_json::from_str(OLLAMA_EXHAUSTED).unwrap();
    assert_eq!(error_for(body).await, LlmError::ReasoningExhausted);
}

#[tokio::test]
async fn reasoning_content_alias_is_understood() {
    let error = error_for(choice_body(
        json!({"content": null, "reasoning_content": "thinking"}),
        "length",
    ))
    .await;
    assert_eq!(error, LlmError::ReasoningExhausted);
}

#[tokio::test]
async fn empty_content_without_reasoning_stays_truncated() {
    let error = error_for(choice_body(json!({"content": ""}), "length")).await;
    assert_eq!(error, LlmError::Truncated);
}

#[tokio::test]
async fn reasoning_alongside_a_normal_answer_is_ignored() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST);
        then.status(200).json_body(choice_body(
            json!({"content": "{\"ok\": true}", "reasoning": "short thought"}),
            "stop",
        ));
    });
    let response = run(&provider(ProviderKind::OpenAiCompatible, &server, None))
        .await
        .unwrap();
    assert_eq!(response.output, json!({"ok": true}));
}

fn fixture_reasoning() -> String {
    let body: serde_json::Value = serde_json::from_str(OLLAMA_EXHAUSTED).unwrap();
    body["choices"][0]["message"]["reasoning"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn reasoning_text_never_reaches_the_response_or_the_error() {
    let reasoning = fixture_reasoning();
    assert!(!reasoning.is_empty());
    let probe = &reasoning[..40];

    let exhausted: serde_json::Value = serde_json::from_str(OLLAMA_EXHAUSTED).unwrap();
    let error = error_for(exhausted).await;
    assert!(!format!("{error} {error:?}").contains(probe));

    let mut answered: serde_json::Value = serde_json::from_str(OLLAMA_EXHAUSTED).unwrap();
    answered["choices"][0]["message"]["content"] = json!("{\"ok\": true}");
    answered["choices"][0]["finish_reason"] = json!("stop");
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST);
        then.status(200).json_body(answered);
    });
    let response = run(&provider(ProviderKind::OpenAiCompatible, &server, None))
        .await
        .unwrap();
    assert!(!format!("{response:?}").contains(probe));
}
