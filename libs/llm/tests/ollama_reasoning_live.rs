use std::time::Duration;

use llm::openai::OpenAiCompatibleProvider;
use lodestone_core::{LlmProvider, LlmRequest, ProviderDefinition, ProviderKind, ReasoningEffort};
use serde_json::json;
use url::Url;

#[tokio::test]
async fn local_ollama_answers_with_reasoning_disabled() {
    if std::env::var("LODESTONE_OLLAMA_LIVE").as_deref() != Ok("1") {
        return;
    }
    let provider = OpenAiCompatibleProvider::new(&ProviderDefinition {
        kind: ProviderKind::OpenAiCompatible,
        base_url: Url::parse("http://localhost:11434").unwrap(),
        model: "gemma4:e2b".to_owned(),
        api_key: None,
        timeout: Duration::from_secs(60),
        reasoning_effort: Some(ReasoningEffort::None),
    })
    .unwrap();
    let schema = json!({
        "type": "object",
        "properties": { "ok": { "type": "boolean" } },
        "required": ["ok"],
        "additionalProperties": false,
    });
    let response = provider
        .complete(&LlmRequest {
            system: "You are a connectivity check. Answer with the requested JSON only.",
            instruction: "Return {\"ok\": true}.",
            documents: &[],
            schema_name: "connectivity_check",
            schema: &schema,
            max_tokens: 64,
        })
        .await
        .unwrap();
    println!("live output: {} model: {}", response.output, response.model);
    assert_eq!(response.output, json!({"ok": true}));
}
