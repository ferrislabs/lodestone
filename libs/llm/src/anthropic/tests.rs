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
    GenerationParams, LlmError, LlmProvider, LlmRequest, ProviderDefinition, ProviderKind,
    UntrustedDocument, render_user_content,
};
use serde_json::{Value, json};
use tracing::instrument::WithSubscriber;
use tracing::{
    Event, Metadata, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};

use super::AnthropicProvider;

const KEY: &str = "sk-ant-super-secret-key";

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<String>>);

struct Writer<'a>(&'a mut String);

impl Visit for Writer<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self.0, "{}={value:?} ", field.name());
    }
}

struct CaptureSubscriber {
    capture: Capture,
    next: AtomicU64,
}

impl CaptureSubscriber {
    fn push(&self, text: &str) {
        if let Ok(mut buffer) = self.capture.0.lock() {
            buffer.push_str(text);
            buffer.push('\n');
        }
    }
}

impl Subscriber for CaptureSubscriber {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, attributes: &Attributes<'_>) -> Id {
        let mut text = format!("span {} ", attributes.metadata().name());
        attributes.record(&mut Writer(&mut text));
        self.push(&text);
        Id::from_u64(self.next.fetch_add(1, Ordering::Relaxed))
    }

    fn record(&self, _: &Id, values: &Record<'_>) {
        let mut text = String::from("record ");
        values.record(&mut Writer(&mut text));
        self.push(&text);
    }

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut text = String::from("event ");
        event.record(&mut Writer(&mut text));
        self.push(&text);
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

fn definition(server: &MockServer, timeout: Duration) -> ProviderDefinition {
    ProviderDefinition {
        kind: ProviderKind::Anthropic,
        base_url: server.base_url().parse().unwrap(),
        model: "claude-test".to_owned(),
        api_key: Some(Secret::new(KEY)),
        timeout,
        reasoning_effort: None,
        generation: GenerationParams::default(),
    }
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {"verdict": {"type": "string"}},
        "required": ["verdict"],
        "additionalProperties": false
    })
}

fn documents() -> [UntrustedDocument<'static>; 1] {
    [UntrustedDocument {
        source: "https://a.example/x",
        content: "hello",
    }]
}

fn request<'a>(documents: &'a [UntrustedDocument<'a>], schema: &'a Value) -> LlmRequest<'a> {
    LlmRequest {
        system: "You triage leads.",
        instruction: "Classify.",
        documents,
        schema_name: "verdict",
        schema,
        max_tokens: 256,
        temperature: None,
    }
}

fn ok_body(text: &str, stop_reason: &str) -> Value {
    json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-test-resolved",
        "content": [
            {"type": "thinking", "thinking": "hmm", "signature": "s"},
            {"type": "text", "text": text}
        ],
        "stop_reason": stop_reason,
        "usage": {"input_tokens": 11, "output_tokens": 7, "cache_read_input_tokens": 0}
    })
}

async fn run(
    server: &MockServer,
    timeout: Duration,
) -> (Result<lodestone_core::LlmResponse, LlmError>, String) {
    let provider = AnthropicProvider::new(definition(server, timeout)).unwrap();
    let schema = schema();
    let documents = documents();
    let capture = Capture::default();
    let subscriber = CaptureSubscriber {
        capture: capture.clone(),
        next: AtomicU64::new(1),
    };
    let result = async { provider.complete(&request(&documents, &schema)).await }
        .with_subscriber(subscriber)
        .await;
    let logs = capture.0.lock().unwrap().clone();
    (result, logs)
}

#[tokio::test]
async fn request_has_the_exact_expected_shape() {
    let server = MockServer::start();
    let schema = schema();
    let documents = documents();
    let rendered = render_user_content(&request(&documents, &schema));
    let expected = json!({
        "model": "claude-test",
        "max_tokens": 256,
        "system": "You triage leads.",
        "messages": [{"role": "user", "content": rendered}],
        "output_config": {"format": {"type": "json_schema", "schema": schema}}
    });
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/messages")
            .header("x-api-key", KEY)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json_body(expected.clone());
        then.status(200)
            .json_body(ok_body("{\"verdict\":\"hot\"}", "end_turn"));
    });
    let (result, _) = run(&server, Duration::from_secs(5)).await;
    mock.assert();
    assert!(result.is_ok());
    for key in ["tools", "tool_choice", "thinking", "temperature"] {
        assert!(expected.get(key).is_none());
    }
}

#[tokio::test]
async fn temperature_and_max_tokens_are_sent_and_penalties_are_not() {
    let server = MockServer::start();
    let mut definition = definition(&server, Duration::from_secs(5));
    definition.generation = GenerationParams {
        max_tokens: Some(700),
        temperature: Some(0.3),
        frequency_penalty: Some(0.5),
        presence_penalty: Some(0.5),
        repeat_penalty: Some(1.3),
    };
    let provider = AnthropicProvider::new(definition).unwrap();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/v1/messages").matches(|request| {
            request.body.as_ref().is_some_and(|body| {
                let value: Value = serde_json::from_slice(body).unwrap();
                value["max_tokens"] == json!(700)
                    && value["temperature"] == json!(0.3)
                    && value.get("frequency_penalty").is_none()
                    && value.get("presence_penalty").is_none()
                    && value.get("repeat_penalty").is_none()
            })
        });
        then.status(200)
            .json_body(ok_body("{\"verdict\":\"hot\"}", "end_turn"));
    });
    let schema = schema();
    let documents = documents();
    provider
        .complete(&request(&documents, &schema))
        .await
        .unwrap();
    mock.assert();
}

#[tokio::test]
async fn success_maps_output_usage_and_model() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        then.status(200)
            .json_body(ok_body("{\"verdict\":\"hot\"}", "end_turn"));
    });
    let (result, logs) = run(&server, Duration::from_secs(5)).await;
    let response = result.unwrap();
    assert_eq!(response.output, json!({"verdict": "hot"}));
    assert_eq!(response.model, "claude-test-resolved");
    assert_eq!(response.usage.input_tokens, 11);
    assert_eq!(response.usage.output_tokens, 7);
    assert!(logs.contains("claude-test"));
    assert!(logs.contains("input_tokens=11"));
    assert!(logs.contains("output_tokens=7"));
    assert!(!logs.contains(KEY));
    assert!(!logs.contains("hello"));
}

#[tokio::test]
async fn errors_are_mapped_and_never_leak_the_key() {
    let cases = vec![
        (200, ok_body("{}", "refusal"), vec![], LlmError::Refused),
        (
            200,
            ok_body("{\"verdict\":", "max_tokens"),
            vec![],
            LlmError::Truncated,
        ),
        (
            429,
            json!({"error": KEY}),
            vec![("retry-after", "7")],
            LlmError::RateLimited {
                retry_after: Some(Duration::from_secs(7)),
            },
        ),
        (
            529,
            json!({"error": KEY}),
            vec![],
            LlmError::RateLimited { retry_after: None },
        ),
        (
            500,
            json!({"error": KEY}),
            vec![],
            LlmError::Status { status: 500 },
        ),
    ];
    for (status, body, headers, expected) in cases {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/messages");
            let mut then = then.status(status).json_body(body.clone());
            for (name, value) in &headers {
                then = then.header(*name, *value);
            }
        });
        let (result, logs) = run(&server, Duration::from_secs(5)).await;
        let error = result.unwrap_err();
        assert_eq!(error, expected);
        assert!(!format!("{error}").contains(KEY));
        assert!(!format!("{error:?}").contains(KEY));
        assert!(!logs.contains(KEY));
    }
}

#[tokio::test]
async fn invalid_json_text_is_invalid_output() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        then.status(200).json_body(ok_body("not json", "end_turn"));
    });
    let (result, logs) = run(&server, Duration::from_secs(5)).await;
    let error = result.unwrap_err();
    assert!(matches!(error, LlmError::InvalidOutput(_)));
    assert!(!format!("{error}").contains(KEY));
    assert!(!format!("{error:?}").contains(KEY));
    assert!(!logs.contains(KEY));
}

#[tokio::test]
async fn slow_server_times_out() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        then.status(200)
            .delay(Duration::from_millis(1500))
            .json_body(ok_body("{}", "end_turn"));
    });
    let (result, logs) = run(&server, Duration::from_millis(200)).await;
    let error = result.unwrap_err();
    assert_eq!(error, LlmError::Timeout);
    assert!(!format!("{error:?}").contains(KEY));
    assert!(!logs.contains(KEY));
}

#[tokio::test]
async fn unreachable_server_is_a_network_error_without_the_key() {
    let server = MockServer::start();
    let mut definition = definition(&server, Duration::from_secs(2));
    definition.base_url = "http://127.0.0.1:1".parse().unwrap();
    let provider = AnthropicProvider::new(definition).unwrap();
    let schema = schema();
    let documents = documents();
    let error = provider
        .complete(&request(&documents, &schema))
        .await
        .unwrap_err();
    assert!(matches!(error, LlmError::Network(_)));
    assert!(!format!("{error:?}").contains(KEY));
}

#[test]
fn constructor_rejects_missing_key_and_wrong_kind() {
    let server = MockServer::start();
    let mut missing = definition(&server, Duration::from_secs(1));
    missing.api_key = None;
    assert!(matches!(
        AnthropicProvider::new(missing),
        Err(LlmError::Configuration(_))
    ));
    let mut wrong = definition(&server, Duration::from_secs(1));
    wrong.kind = ProviderKind::Mistral;
    assert!(matches!(
        AnthropicProvider::new(wrong),
        Err(LlmError::Configuration(_))
    ));
}
