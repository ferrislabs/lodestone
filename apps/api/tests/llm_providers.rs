mod support;

use std::time::Duration;

use axum::Router;
use http::{Method, StatusCode, header};
use httpmock::{Method::POST, MockServer};
use lodestone_core::Tier;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{Harness, send, send_raw};
use utoipa::OpenApi;

const KEY: &str = "sk-live-0123456789-abcdef";
const NIL: &str = "00000000-0000-0000-0000-000000000000";

struct Api {
    harness: Harness,
    bearer: String,
}

impl Api {
    async fn new(pool: PgPool) -> Self {
        let harness = Harness::new(pool).await;
        let bearer = format!("Bearer {}", harness.valid_user_token());
        Self { harness, bearer }
    }

    fn app(&self) -> Router {
        self.harness.public()
    }

    async fn raw(
        &self,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value, String) {
        send(self.app(), method, uri, Some(&self.bearer), body).await
    }

    async fn call(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let (status, json, _) = self.raw(method, uri, body).await;
        (status, json)
    }

    async fn create(&self, body: Value) -> String {
        let (status, body) = self
            .call(Method::POST, "/api/v1/llm/providers", Some(body))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["data"]["id"].as_str().expect("id").to_owned()
    }

    async fn test(&self, id: &str) -> (StatusCode, Value, String) {
        self.raw(
            Method::POST,
            &format!("/api/v1/llm/providers/{id}/test"),
            None,
        )
        .await
    }
}

fn provider(name: &str, base_url: &str) -> Value {
    json!({
        "name": name,
        "kind": "openai_compatible",
        "base_url": base_url,
        "model": "model-a",
        "api_key": KEY,
    })
}

fn paths(details: &Value) -> Vec<String> {
    details["details"]
        .as_array()
        .expect("details")
        .iter()
        .map(|detail| detail["path"].as_str().expect("path").to_owned())
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_with_a_key_returns_201_location_and_never_the_key(pool: PgPool) {
    let api = Api::new(pool).await;

    let (status, headers, raw) = send_raw(
        api.app(),
        Method::POST,
        "/api/v1/llm/providers",
        Some(&api.bearer),
        Some(
            json!({
                "name": "main",
                "kind": "mistral",
                "base_url": "https://api.mistral.ai",
                "model": "small",
                "api_key": KEY,
                "max_retries": 3,
                "pricing": { "input_cents_per_mtok": 10, "output_cents_per_mtok": 30 },
                "timeout_seconds": 45,
            })
            .to_string(),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{raw}");
    assert!(!raw.contains(KEY));
    let body: Value = serde_json::from_str(&raw).expect("json");
    let data = &body["data"];
    let id = data["id"].as_str().expect("id");
    assert_eq!(
        headers[header::LOCATION],
        format!("/api/v1/llm/providers/{id}").as_str()
    );
    assert_eq!(data["name"], "main");
    assert_eq!(data["kind"], "mistral");
    assert_eq!(data["has_api_key"], true);
    assert_eq!(data["enabled"], true);
    assert_eq!(data["max_retries"], 3);
    assert_eq!(data["timeout_seconds"], 45);
    assert_eq!(
        data["pricing"],
        json!({ "input_cents_per_mtok": 10, "output_cents_per_mtok": 30 })
    );
    assert_eq!(data["tiers"], json!([]));
    assert!(data.get("api_key").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_without_a_key_for_a_keyless_kind(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api
        .create(json!({
            "name": "local",
            "kind": "openai_compatible",
            "base_url": "http://localhost:11434",
            "model": "llama",
        }))
        .await;

    let (status, body) = api
        .call(Method::GET, &format!("/api/v1/llm/providers/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["has_api_key"], false);
    assert_eq!(body["data"]["max_retries"], 2);
    assert_eq!(body["data"]["timeout_seconds"], 30);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_payloads_are_rejected_with_field_paths(pool: PgPool) {
    let api = Api::new(pool).await;
    let cases = [
        (
            json!({ "name": "a", "kind": "gpt", "base_url": "https://x.example", "model": "m" }),
            "kind",
        ),
        (
            json!({ "name": "a", "kind": "mistral", "base_url": "ftp://x.example", "model": "m", "api_key": "k" }),
            "base_url",
        ),
        (
            json!({ "name": "a", "kind": "mistral", "base_url": "not a url", "model": "m", "api_key": "k" }),
            "base_url",
        ),
        (
            json!({ "name": "a", "kind": "mistral", "base_url": "https://x.example", "model": "  ", "api_key": "k" }),
            "model",
        ),
        (
            json!({ "kind": "mistral", "base_url": "https://x.example", "model": "m", "api_key": "k" }),
            "name",
        ),
        (
            json!({ "name": "a", "kind": "mistral", "base_url": "https://x.example", "model": "m" }),
            "api_key",
        ),
        (
            json!({ "name": "a", "kind": "mistral", "base_url": "https://x.example", "model": "m", "api_key": "" }),
            "api_key",
        ),
        (
            json!({ "name": "a", "kind": "mistral", "base_url": "https://x.example", "model": "m", "api_key": "k", "max_retries": 99 }),
            "max_retries",
        ),
        (
            json!({ "name": "a", "kind": "mistral", "base_url": "https://x.example", "model": "m", "api_key": "k", "timeout_seconds": 0 }),
            "timeout_seconds",
        ),
        (
            json!({ "name": "a", "kind": "mistral", "base_url": "https://x.example", "model": "m", "api_key": "k", "pricing": { "input_cents_per_mtok": -1 } }),
            "pricing.input_cents_per_mtok",
        ),
        (
            json!({ "name": "a", "kind": "mistral", "base_url": "https://x.example", "model": "m", "api_key": "k", "colour": "red" }),
            "colour",
        ),
    ];

    for (body, path) in cases {
        let (status, json, raw) = api
            .raw(Method::POST, "/api/v1/llm/providers", Some(body))
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{raw}");
        assert!(paths(&json).contains(&path.to_owned()), "{path} in {raw}");
    }

    let (status, _) = api.call(Method::GET, "/api/v1/llm/providers", None).await;
    assert_eq!(status, StatusCode::OK);
    let (_, list) = api.call(Method::GET, "/api/v1/llm/providers", None).await;
    assert_eq!(list["data"], json!([]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn malformed_json_is_rejected_and_duplicate_names_conflict(pool: PgPool) {
    let api = Api::new(pool).await;
    let (status, _, raw) = send_raw(
        api.app(),
        Method::POST,
        "/api/v1/llm/providers",
        Some(&api.bearer),
        Some("{".to_owned()),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{raw}");

    api.create(provider("dup", "https://x.example")).await;
    let (status, _) = api
        .call(
            Method::POST,
            "/api/v1/llm/providers",
            Some(provider("dup", "https://y.example")),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn reads_never_contain_the_key(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create(provider("a", "https://x.example")).await;
    api.create(json!({
        "name": "b",
        "kind": "openai_compatible",
        "base_url": "https://y.example",
        "model": "m",
    }))
    .await;

    let (status, list, raw_list) = api.raw(Method::GET, "/api/v1/llm/providers", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!raw_list.contains(KEY));
    assert!(!raw_list.contains("\"api_key\""));
    let items = list["data"].as_array().expect("list");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["has_api_key"], true);
    assert_eq!(items[1]["has_api_key"], false);

    let (_, _, raw_one) = api
        .raw(Method::GET, &format!("/api/v1/llm/providers/{id}"), None)
        .await;
    assert!(!raw_one.contains(KEY));
}

#[sqlx::test(migrations = "../../migrations")]
async fn patch_keeps_replaces_and_removes_the_key(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create(provider("a", "https://x.example")).await;
    let uri = format!("/api/v1/llm/providers/{id}");
    api.call(
        Method::PUT,
        "/api/v1/llm/tiers/triage",
        Some(json!({ "provider_id": id })),
    )
    .await;
    let resolve = || async {
        api.harness
            .state
            .usecase
            .resolve_llm_tier(Tier::Triage, api.harness.state.cipher.as_ref())
            .await
            .expect("resolve")
            .definition
            .api_key
            .map(|key| key.expose().to_owned())
    };

    let (status, body) = api
        .call(Method::PATCH, &uri, Some(json!({ "name": "renamed", "model": "model-b", "pricing": { "output_cents_per_mtok": 9 } })))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["name"], "renamed");
    assert_eq!(body["data"]["model"], "model-b");
    assert_eq!(body["data"]["has_api_key"], true);
    assert_eq!(body["data"]["pricing"]["output_cents_per_mtok"], 9);
    assert_eq!(body["data"]["tiers"], json!(["triage"]));
    assert_eq!(resolve().await.as_deref(), Some(KEY));

    let (status, body, raw) = api
        .raw(
            Method::PATCH,
            &uri,
            Some(json!({ "api_key": "sk-replaced-key" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert!(!raw.contains("sk-replaced-key"));
    assert_eq!(body["data"]["has_api_key"], true);
    assert_eq!(resolve().await.as_deref(), Some("sk-replaced-key"));

    let (status, body) = api
        .call(Method::PATCH, &uri, Some(json!({ "api_key": null })))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["has_api_key"], false);
    assert_eq!(resolve().await, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn patch_validates_and_keyless_mistral_is_refused(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create(provider("a", "https://x.example")).await;
    let uri = format!("/api/v1/llm/providers/{id}");

    let (status, body) = api
        .call(
            Method::PATCH,
            &uri,
            Some(json!({ "base_url": "gopher://x" })),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(paths(&body), vec!["base_url"]);

    let (status, body) = api
        .call(
            Method::PATCH,
            &uri,
            Some(json!({ "kind": "mistral", "api_key": null })),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(paths(&body), vec!["api_key"]);

    let (status, _) = api
        .call(
            Method::PATCH,
            &format!("/api/v1/llm/providers/{NIL}"),
            Some(json!({ "name": "x" })),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn enable_and_disable_toggle_the_provider(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create(provider("a", "https://x.example")).await;

    let (status, body) = api
        .call(
            Method::POST,
            &format!("/api/v1/llm/providers/{id}/disable"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["enabled"], false);
    let (_, body) = api
        .call(Method::GET, &format!("/api/v1/llm/providers/{id}"), None)
        .await;
    assert_eq!(body["data"]["enabled"], false);
    let (_, body) = api
        .call(
            Method::POST,
            &format!("/api/v1/llm/providers/{id}/enable"),
            None,
        )
        .await;
    assert_eq!(body["data"]["enabled"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn tiers_can_be_assigned_listed_and_unassigned(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create(provider("a", "https://x.example")).await;

    let (_, body) = api.call(Method::GET, "/api/v1/llm/tiers", None).await;
    assert_eq!(
        body["data"],
        json!([
            { "tier": "triage", "provider_id": null },
            { "tier": "qualification", "provider_id": null },
        ])
    );

    let (status, body) = api
        .call(
            Method::PUT,
            "/api/v1/llm/tiers/qualification",
            Some(json!({ "provider_id": id })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"],
        json!({ "tier": "qualification", "provider_id": id })
    );

    let (_, body) = api.call(Method::GET, "/api/v1/llm/tiers", None).await;
    assert_eq!(body["data"][1]["provider_id"], id.as_str());
    assert_eq!(body["data"][0]["provider_id"], Value::Null);
    let (_, body) = api
        .call(Method::GET, &format!("/api/v1/llm/providers/{id}"), None)
        .await;
    assert_eq!(body["data"]["tiers"], json!(["qualification"]));

    let (status, _) = api
        .call(Method::DELETE, "/api/v1/llm/tiers/qualification", None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = api
        .call(Method::DELETE, "/api/v1/llm/tiers/qualification", None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn tier_assignment_validates_its_inputs(pool: PgPool) {
    let api = Api::new(pool).await;

    let (status, _) = api
        .call(
            Method::PUT,
            "/api/v1/llm/tiers/other",
            Some(json!({ "provider_id": NIL })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = api
        .call(Method::DELETE, "/api/v1/llm/tiers/other", None)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    for body in [
        json!({}),
        json!({ "provider_id": "nope" }),
        json!({ "provider_id": NIL }),
    ] {
        let (status, json) = api
            .call(Method::PUT, "/api/v1/llm/tiers/triage", Some(body))
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(paths(&json), vec!["provider_id"]);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn delete_removes_an_unassigned_provider_and_refuses_an_assigned_one(pool: PgPool) {
    let api = Api::new(pool).await;
    let free = api.create(provider("free", "https://x.example")).await;
    let used = api.create(provider("used", "https://y.example")).await;
    api.call(
        Method::PUT,
        "/api/v1/llm/tiers/triage",
        Some(json!({ "provider_id": used })),
    )
    .await;

    let (status, _, raw) = api
        .raw(
            Method::DELETE,
            &format!("/api/v1/llm/providers/{free}"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(raw.is_empty());
    let (status, _) = api
        .call(Method::GET, &format!("/api/v1/llm/providers/{free}"), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, body) = api
        .call(
            Method::DELETE,
            &format!("/api/v1/llm/providers/{used}"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["message"]
            .as_str()
            .expect("message")
            .contains("triage"),
        "{body}"
    );
    let (status, _) = api
        .call(Method::GET, &format!("/api/v1/llm/providers/{used}"), None)
        .await;
    assert_eq!(status, StatusCode::OK);

    api.call(Method::DELETE, "/api/v1/llm/tiers/triage", None)
        .await;
    let (status, _) = api
        .call(
            Method::DELETE,
            &format!("/api/v1/llm/providers/{used}"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_and_malformed_identifiers(pool: PgPool) {
    let api = Api::new(pool).await;
    let missing = format!("/api/v1/llm/providers/{NIL}");

    for (method, uri) in [
        (Method::GET, missing.clone()),
        (Method::DELETE, missing.clone()),
        (Method::POST, format!("{missing}/enable")),
        (Method::POST, format!("{missing}/disable")),
        (Method::POST, format!("{missing}/test")),
    ] {
        let (status, _) = api.call(method.clone(), &uri, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}");
    }
    let (status, _) = api
        .call(Method::GET, "/api/v1/llm/providers/not-a-uuid", None)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

fn every_route() -> Vec<(Method, String)> {
    let provider = format!("/api/v1/llm/providers/{NIL}");
    vec![
        (Method::GET, "/api/v1/llm/providers".to_owned()),
        (Method::POST, "/api/v1/llm/providers".to_owned()),
        (Method::GET, provider.clone()),
        (Method::PATCH, provider.clone()),
        (Method::DELETE, provider.clone()),
        (Method::POST, format!("{provider}/enable")),
        (Method::POST, format!("{provider}/disable")),
        (Method::POST, format!("{provider}/test")),
        (Method::GET, "/api/v1/llm/tiers".to_owned()),
        (Method::PUT, "/api/v1/llm/tiers/triage".to_owned()),
        (Method::DELETE, "/api/v1/llm/tiers/triage".to_owned()),
    ]
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_route_requires_authentication(pool: PgPool) {
    let api = Api::new(pool).await;
    for (method, uri) in every_route() {
        let (status, _, _) = send(api.app(), method.clone(), &uri, None, Some(json!({}))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn service_accounts_can_read_but_not_mutate(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create(provider("a", "https://x.example")).await;
    let client = format!("Bearer {}", api.harness.valid_client_token());
    let missing = format!("/api/v1/llm/providers/{id}");

    for uri in [
        "/api/v1/llm/providers",
        missing.as_str(),
        "/api/v1/llm/tiers",
    ] {
        let (status, _, raw) = send(api.app(), Method::GET, uri, Some(&client), None).await;
        assert_eq!(status, StatusCode::OK, "{uri} {raw}");
        assert!(!raw.contains(KEY));
    }

    let mutations = [
        (Method::POST, "/api/v1/llm/providers".to_owned()),
        (Method::PATCH, missing.clone()),
        (Method::DELETE, missing.clone()),
        (Method::POST, format!("{missing}/enable")),
        (Method::POST, format!("{missing}/disable")),
        (Method::POST, format!("{missing}/test")),
        (Method::PUT, "/api/v1/llm/tiers/triage".to_owned()),
        (Method::DELETE, "/api/v1/llm/tiers/triage".to_owned()),
    ];
    for (method, uri) in mutations {
        let (status, _, _) = send(
            api.app(),
            method.clone(),
            &uri,
            Some(&client),
            Some(json!({ "provider_id": id })),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
    }
}

const ANTHROPIC_OK: fn() -> Value = || {
    json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-resolved",
        "content": [{ "type": "text", "text": "{\"ok\": true}" }],
        "stop_reason": "end_turn",
        "usage": { "input_tokens": 5, "output_tokens": 3 },
    })
};

#[sqlx::test(migrations = "../../migrations")]
async fn test_endpoint_succeeds_against_an_anthropic_shaped_upstream(pool: PgPool) {
    let api = Api::new(pool).await;
    let upstream = MockServer::start_async().await;
    let mock = upstream
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/messages")
                .header("x-api-key", KEY);
            then.status(200).json_body(ANTHROPIC_OK());
        })
        .await;
    let id = api
        .create(json!({
            "name": "claude", "kind": "anthropic", "base_url": upstream.base_url(),
            "model": "claude-x", "api_key": KEY,
        }))
        .await;

    let (status, body, raw) = api.test(&id).await;

    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_eq!(body["data"]["ok"], true);
    assert_eq!(body["data"]["model"], "claude-resolved");
    assert!(body["data"]["latency_ms"].is_u64());
    assert!(!raw.contains(KEY));
    mock.assert_async().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_endpoint_succeeds_against_an_openai_shaped_upstream_even_when_disabled(pool: PgPool) {
    let api = Api::new(pool).await;
    let upstream = MockServer::start_async().await;
    let mock = upstream
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .header("authorization", format!("Bearer {KEY}"));
            then.status(200).json_body(json!({
                "model": "served-model",
                "choices": [{ "message": { "role": "assistant", "content": "{\"ok\": true}" }, "finish_reason": "stop" }],
                "usage": { "prompt_tokens": 4, "completion_tokens": 2 },
            }));
        })
        .await;
    let id = api.create(provider("oa", &upstream.base_url())).await;
    api.call(
        Method::POST,
        &format!("/api/v1/llm/providers/{id}/disable"),
        None,
    )
    .await;

    let (status, body, raw) = api.test(&id).await;

    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_eq!(body["data"]["ok"], true);
    assert_eq!(body["data"]["model"], "served-model");
    assert!(!raw.contains(KEY));
    mock.assert_async().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_endpoint_reports_a_rejected_key_without_echoing_it(pool: PgPool) {
    let api = Api::new(pool).await;
    let upstream = MockServer::start_async().await;
    upstream
        .mock_async(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(401).json_body(
                json!({ "error": { "message": format!("Incorrect API key provided: {KEY}") } }),
            );
        })
        .await;
    let id = api.create(provider("oa", &upstream.base_url())).await;

    let (status, body, raw) = api.test(&id).await;

    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_eq!(body["data"]["ok"], false);
    assert_eq!(body["data"]["error"]["kind"], "authentication");
    assert!(
        !body["data"]["error"]["message"]
            .as_str()
            .expect("message")
            .is_empty()
    );
    assert!(!raw.contains(KEY));
    assert_eq!(
        body["data"]["error"]["message"],
        "The provider rejected the credentials: Incorrect API key provided: [redacted]"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_endpoint_reports_an_unreachable_endpoint(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create(provider("down", "http://127.0.0.1:1")).await;

    let (status, body, raw) = api.test(&id).await;

    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_eq!(body["data"]["ok"], false);
    assert_eq!(body["data"]["error"]["kind"], "unreachable");
    assert!(!raw.contains(KEY));
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_endpoint_reports_a_timeout(pool: PgPool) {
    let api = Api::new(pool).await;
    let upstream = MockServer::start_async().await;
    upstream
        .mock_async(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(200)
                .delay(Duration::from_secs(4))
                .json_body(json!({}));
        })
        .await;
    let mut body = provider("slow", &upstream.base_url());
    body["timeout_seconds"] = json!(1);
    let id = api.create(body).await;

    let (status, body, raw) = api.test(&id).await;

    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_eq!(body["data"]["ok"], false);
    assert_eq!(body["data"]["error"]["kind"], "timeout");
    assert!(!raw.contains(KEY));
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_endpoint_classifies_rate_limits_bad_bodies_and_other_statuses(pool: PgPool) {
    let api = Api::new(pool).await;
    let cases = [
        (429, json!({ "error": KEY }), "rate_limited"),
        (
            200,
            json!({ "model": "m", "choices": [] }),
            "invalid_response",
        ),
        (500, json!({ "error": KEY }), "other"),
    ];
    for (index, (status_code, body, kind)) in cases.into_iter().enumerate() {
        let upstream = MockServer::start_async().await;
        upstream
            .mock_async(|when, then| {
                when.method(POST).path("/v1/chat/completions");
                then.status(status_code).json_body(body);
            })
            .await;
        let id = api
            .create(provider(&format!("p{index}"), &upstream.base_url()))
            .await;

        let (status, body, raw) = api.test(&id).await;

        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(body["data"]["error"]["kind"], kind, "{raw}");
        assert!(!raw.contains(KEY), "{raw}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_endpoint_reports_reasoning_exhaustion_without_the_reasoning_text(pool: PgPool) {
    let api = Api::new(pool).await;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../libs/llm/src/openai/fixtures/ollama_reasoning_exhausted.json"
    ))
    .expect("fixture");
    let reasoning = fixture["choices"][0]["message"]["reasoning"]
        .as_str()
        .expect("reasoning")
        .to_owned();
    let upstream = MockServer::start_async().await;
    upstream
        .mock_async(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(200).json_body(fixture.clone());
        })
        .await;
    let id = api.create(provider("think", &upstream.base_url())).await;

    let (status, body, raw) = api.test(&id).await;

    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_eq!(body["data"]["ok"], false);
    assert_eq!(body["data"]["error"]["kind"], "reasoning_exhausted");
    assert!(!raw.contains(&reasoning[..40]), "{raw}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_endpoint_sends_the_configured_reasoning_effort(pool: PgPool) {
    let api = Api::new(pool).await;
    let upstream = MockServer::start_async().await;
    let mock = upstream
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .json_body_partial(r#"{"reasoning_effort": "none"}"#);
            then.status(200).json_body(json!({
                "model": "m",
                "choices": [{ "message": { "role": "assistant", "content": "{\"ok\": true}" }, "finish_reason": "stop" }],
            }));
        })
        .await;
    let mut body = provider("think", &upstream.base_url());
    body["reasoning_effort"] = json!("none");
    let id = api.create(body).await;

    let (_, body, raw) = api.test(&id).await;

    assert_eq!(body["data"]["ok"], true, "{raw}");
    mock.assert_async().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn reasoning_effort_is_created_read_patched_and_cleared(pool: PgPool) {
    let api = Api::new(pool).await;
    let (_, body) = api
        .call(
            Method::POST,
            "/api/v1/llm/providers",
            Some(provider("plain", "https://x.example")),
        )
        .await;
    assert_eq!(body["data"]["reasoning_effort"], Value::Null);

    let mut with = provider("with", "https://y.example");
    with["reasoning_effort"] = json!("low");
    let id = api.create(with).await;
    let uri = format!("/api/v1/llm/providers/{id}");
    let (_, body) = api.call(Method::GET, &uri, None).await;
    assert_eq!(body["data"]["reasoning_effort"], "low");

    let (status, body) = api
        .call(Method::PATCH, &uri, Some(json!({ "name": "renamed" })))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["reasoning_effort"], "low");

    let (_, body) = api
        .call(
            Method::PATCH,
            &uri,
            Some(json!({ "reasoning_effort": "high" })),
        )
        .await;
    assert_eq!(body["data"]["reasoning_effort"], "high");

    let (_, body) = api
        .call(
            Method::PATCH,
            &uri,
            Some(json!({ "reasoning_effort": null })),
        )
        .await;
    assert_eq!(body["data"]["reasoning_effort"], Value::Null);
    let (_, list) = api.call(Method::GET, "/api/v1/llm/providers", None).await;
    assert!(
        list["data"]
            .as_array()
            .expect("list")
            .iter()
            .any(|item| item["name"] == "renamed" && item["reasoning_effort"].is_null())
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_reasoning_effort_is_rejected_with_its_path(pool: PgPool) {
    let api = Api::new(pool).await;
    for value in [json!("max"), json!(3)] {
        let mut body = provider("bad", "https://x.example");
        body["reasoning_effort"] = value;
        let (status, details) = api
            .call(Method::POST, "/api/v1/llm/providers", Some(body))
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{details}");
        assert_eq!(paths(&details), vec!["reasoning_effort"]);
    }
    let id = api.create(provider("ok", "https://x.example")).await;
    let (status, details) = api
        .call(
            Method::PATCH,
            &format!("/api/v1/llm/providers/{id}"),
            Some(json!({ "reasoning_effort": "max" })),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(paths(&details), vec!["reasoning_effort"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn generation_params_are_created_read_patched_and_cleared(pool: PgPool) {
    let api = Api::new(pool).await;
    let mut with = provider("tuned", "https://x.example");
    with["max_tokens"] = json!(900);
    with["temperature"] = json!(0.2);
    with["frequency_penalty"] = json!(0.4);
    with["presence_penalty"] = json!(-0.5);
    with["repeat_penalty"] = json!(1.3);
    let id = api.create(with).await;
    let uri = format!("/api/v1/llm/providers/{id}");
    let (_, body) = api.call(Method::GET, &uri, None).await;
    assert_eq!(body["data"]["max_tokens"], 900);
    assert_eq!(body["data"]["temperature"], 0.2);
    assert_eq!(body["data"]["frequency_penalty"], 0.4);
    assert_eq!(body["data"]["presence_penalty"], -0.5);
    assert_eq!(body["data"]["repeat_penalty"], 1.3);

    let (status, body) = api
        .call(Method::PATCH, &uri, Some(json!({ "name": "renamed" })))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["max_tokens"], 900);

    let (_, body) = api
        .call(
            Method::PATCH,
            &uri,
            Some(json!({ "max_tokens": null, "temperature": 0, "repeat_penalty": null })),
        )
        .await;
    assert_eq!(body["data"]["max_tokens"], Value::Null);
    assert_eq!(body["data"]["temperature"], 0.0);
    assert_eq!(body["data"]["repeat_penalty"], Value::Null);
    assert_eq!(body["data"]["frequency_penalty"], 0.4);

    let (_, plain) = api
        .call(
            Method::POST,
            "/api/v1/llm/providers",
            Some(provider("plain", "https://z.example")),
        )
        .await;
    for field in [
        "max_tokens",
        "temperature",
        "frequency_penalty",
        "presence_penalty",
        "repeat_penalty",
    ] {
        assert_eq!(plain["data"][field], Value::Null, "{field}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_generation_params_are_rejected_with_their_paths(pool: PgPool) {
    let api = Api::new(pool).await;
    let mut body = provider("bad", "https://x.example");
    body["max_tokens"] = json!(0);
    body["temperature"] = json!(2.5);
    body["frequency_penalty"] = json!(-3);
    body["presence_penalty"] = json!("high");
    body["repeat_penalty"] = json!(0.5);
    let (status, details) = api
        .call(Method::POST, "/api/v1/llm/providers", Some(body))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{details}");
    let mut found = paths(&details);
    found.sort_unstable();
    assert_eq!(
        found,
        vec![
            "frequency_penalty",
            "max_tokens",
            "presence_penalty",
            "repeat_penalty",
            "temperature"
        ]
    );
    let id = api.create(provider("ok", "https://x.example")).await;
    let (status, details) = api
        .call(
            Method::PATCH,
            &format!("/api/v1/llm/providers/{id}"),
            Some(json!({ "temperature": 3 })),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(paths(&details), vec!["temperature"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_endpoint_sends_the_configured_generation_params(pool: PgPool) {
    let api = Api::new(pool).await;
    let upstream = MockServer::start_async().await;
    let mock = upstream
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .json_body_partial(r#"{"temperature": 0.2, "presence_penalty": 0.5}"#);
            then.status(200).json_body(json!({
                "model": "m",
                "choices": [{ "message": { "role": "assistant", "content": "{\"ok\": true}" }, "finish_reason": "stop" }],
            }));
        })
        .await;
    let mut body = provider("tuned", &upstream.base_url());
    body["temperature"] = json!(0.2);
    body["presence_penalty"] = json!(0.5);
    let id = api.create(body).await;

    let (_, body, raw) = api.test(&id).await;

    assert_eq!(body["data"]["ok"], true, "{raw}");
    mock.assert_async().await;
}

#[test]
fn openapi_documents_every_llm_path_with_bearer_auth() {
    let spec = serde_json::to_value(api::openapi::ApiDoc::openapi()).expect("spec");
    let expected = [
        ("/api/v1/llm/providers", vec!["get", "post"]),
        (
            "/api/v1/llm/providers/{provider_id}",
            vec!["get", "patch", "delete"],
        ),
        ("/api/v1/llm/providers/{provider_id}/enable", vec!["post"]),
        ("/api/v1/llm/providers/{provider_id}/disable", vec!["post"]),
        ("/api/v1/llm/providers/{provider_id}/test", vec!["post"]),
        ("/api/v1/llm/tiers", vec!["get"]),
        ("/api/v1/llm/tiers/{tier}", vec!["put", "delete"]),
    ];
    for (path, methods) in expected {
        for method in methods {
            let secured = spec["paths"][path][method]["security"]
                .as_array()
                .is_some_and(|reqs| reqs.iter().any(|req| req.get("bearer_auth").is_some()));
            assert!(secured, "{method} {path}");
        }
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_endpoint_explains_a_model_that_rejects_the_reasoning_effort(pool: PgPool) {
    let api = Api::new(pool).await;
    let upstream = MockServer::start_async().await;
    let mock = upstream
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .json_body_partial(r#"{"reasoning_effort": "medium"}"#);
            then.status(400)
                .json_body(json!({ "message": "reasoning_effort is not enabled for this model" }));
        })
        .await;
    let mut body = provider("ministral", &upstream.base_url());
    body["reasoning_effort"] = json!("medium");
    let id = api.create(body).await;

    let (status, body, raw) = api.test(&id).await;

    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_eq!(body["data"]["ok"], false);
    assert_eq!(body["data"]["error"]["kind"], "reasoning_unsupported");
    assert_eq!(
        body["data"]["error"]["message"],
        "Ce modèle n'accepte pas le réglage de raisonnement : choisissez « Par défaut (non envoyé) »."
    );
    mock.assert_hits(1);
    assert!(!raw.contains(KEY), "{raw}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn max_concurrency_defaults_by_host_and_kind_and_is_read_and_patched(pool: PgPool) {
    let api = Api::new(pool).await;
    let cases = [
        (
            "ollama",
            "openai_compatible",
            "http://localhost:11434/v1",
            2,
        ),
        (
            "loopback",
            "openai_compatible",
            "http://127.0.0.1:11434/v1",
            2,
        ),
        ("ipv6", "openai_compatible", "http://[::1]:11434/v1", 2),
        ("remote", "openai_compatible", "https://llm.example/v1", 4),
        ("mistral", "mistral", "http://localhost:8080", 4),
    ];
    for (name, kind, base_url, expected) in cases {
        let id = api
            .create(json!({
                "name": name,
                "kind": kind,
                "base_url": base_url,
                "model": "model-a",
                "api_key": KEY,
            }))
            .await;
        let (status, body) = api
            .call(Method::GET, &format!("/api/v1/llm/providers/{id}"), None)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["max_concurrency"], expected, "{name}");
        assert_eq!(body["data"]["in_flight"], 0, "{name}");
    }
    let mut chosen = provider("chosen", "http://localhost:11434/v1");
    chosen["max_concurrency"] = json!(6);
    let id = api.create(chosen).await;
    let (_, body) = api
        .call(Method::GET, &format!("/api/v1/llm/providers/{id}"), None)
        .await;
    assert_eq!(body["data"]["max_concurrency"], 6);
    let (status, body) = api
        .call(
            Method::PATCH,
            &format!("/api/v1/llm/providers/{id}"),
            Some(json!({ "max_concurrency": 32 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["max_concurrency"], 32);
    let (status, body) = api
        .call(
            Method::PATCH,
            &format!("/api/v1/llm/providers/{id}"),
            Some(json!({ "name": "renamed" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["max_concurrency"], 32);
    let (_, listed) = api.call(Method::GET, "/api/v1/llm/providers", None).await;
    let listed = listed["data"]
        .as_array()
        .expect("list")
        .iter()
        .find(|entry| entry["id"] == id.as_str())
        .cloned()
        .expect("listed");
    assert_eq!(listed["max_concurrency"], 32);
    assert_eq!(listed["in_flight"], 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn max_concurrency_outside_one_to_thirty_two_is_rejected_with_its_path(pool: PgPool) {
    let api = Api::new(pool).await;
    for value in [json!(0), json!(33), json!(-1), json!("4"), json!(2.5)] {
        let mut body = provider("bad", "https://x.example");
        body["max_concurrency"] = value.clone();
        let (status, details) = api
            .call(Method::POST, "/api/v1/llm/providers", Some(body))
            .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{value}: {details}"
        );
        assert_eq!(paths(&details), vec!["max_concurrency"]);
    }
    let id = api.create(provider("ok", "https://x.example")).await;
    let (status, details) = api
        .call(
            Method::PATCH,
            &format!("/api/v1/llm/providers/{id}"),
            Some(json!({ "max_concurrency": 0 })),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(paths(&details), vec!["max_concurrency"]);
}
