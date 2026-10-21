use std::{fs, num::NonZeroU32, path::PathBuf, time::Duration};

use llm::openai::OpenAiCompatibleProvider;
use lodestone_core::{
    ProviderDefinition, ProviderKind, QUALIFICATION_V1, QualificationOutput, ReasoningEffort,
    TokenPricing, UntrustedDocument, ValidatedProfile,
    llm::gateway::{CallSettings, Prompt, RunBudget, call_structured},
};
use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core")
}

#[tokio::test]
async fn qualification_summary_follows_the_profile_language() {
    if std::env::var("LODESTONE_LANG_LIVE").ok().as_deref() != Some("1") {
        println!(
            "skipped: set LODESTONE_LANG_LIVE=1 (LODESTONE_LLM_BASE_URL, LODESTONE_LLM_MODEL)"
        );
        return;
    }
    let definition = ProviderDefinition {
        kind: ProviderKind::OpenAiCompatible,
        base_url: std::env::var("LODESTONE_LLM_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11434".into())
            .parse()
            .unwrap(),
        model: std::env::var("LODESTONE_LLM_MODEL").unwrap_or_else(|_| "gemma4:e2b".into()),
        api_key: None,
        timeout: Duration::from_secs(300),
        reasoning_effort: Some(ReasoningEffort::None),
    };
    let provider = OpenAiCompatibleProvider::new(&definition).unwrap();
    let case: Value = serde_json::from_str(
        &fs::read_to_string(
            root().join("tests/fixtures/prompts/qualification_legacy_keycloak.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let docs: Vec<(String, String)> = case["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["source"].as_str().unwrap().to_owned(),
                d["content"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let untrusted: Vec<UntrustedDocument<'_>> = docs
        .iter()
        .map(|(source, content)| UntrustedDocument { source, content })
        .collect();
    let raw =
        fs::read_to_string(root().join("../../docs/examples/profiles/iam-ferriskey.json")).unwrap();
    for language in ["fr", "en"] {
        let mut profile: Value = serde_json::from_str(&raw).unwrap();
        profile["llm"]["language"] = serde_json::json!(language);
        let profile = ValidatedProfile::parse(profile).unwrap();
        let system = QUALIFICATION_V1.system(profile.definition());
        let prompt = Prompt {
            system: &system,
            instruction: QUALIFICATION_V1.instruction(),
            documents: &untrusted,
            max_tokens: 4096,
        };
        let settings = CallSettings {
            max_retries: 2,
            pricing: TokenPricing {
                input_cents_per_mtok: 0,
                output_cents_per_mtok: 0,
            },
            base_delay: Duration::from_secs(2),
        };
        let mut budget = RunBudget::new(NonZeroU32::new(10).unwrap());
        let output =
            call_structured::<QualificationOutput, _>(&provider, prompt, settings, &mut budget)
                .await
                .unwrap()
                .value;
        println!("[{language}] summary: {}", output.summary);
        println!("[{language}] approach_angle: {}", output.approach_angle);
        println!("[{language}] subject: {}", output.draft_message.subject);
    }
}
