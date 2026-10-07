use std::{fs, num::NonZeroU32, path::PathBuf, time::Duration};

use common::Secret;
use llm::{anthropic::AnthropicProvider, openai::OpenAiCompatibleProvider};
use lodestone_core::{
    AggregatorOutput, Facts, GenerationParams, LlmProvider, ProviderDefinition, ProviderKind,
    TokenPricing, UntrustedDocument, ValidatedProfile, Verdict, aggregation_instruction,
    aggregation_prompt, check_evidence,
    llm::gateway::{CallSettings, Prompt, RunBudget, call_structured},
};
use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core")
}

fn definition_from_env() -> Option<ProviderDefinition> {
    if std::env::var("LODESTONE_LLM_LIVE").ok().as_deref() != Some("1") {
        return None;
    }
    let kind = ProviderKind::parse(
        &std::env::var("LODESTONE_LLM_KIND").unwrap_or_else(|_| "anthropic".into()),
    )
    .expect("LODESTONE_LLM_KIND is anthropic, mistral or openai_compatible");
    let base_url = std::env::var("LODESTONE_LLM_BASE_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com".into());
    let model = std::env::var("LODESTONE_LLM_MODEL").unwrap_or_else(|_| "claude-opus-5-5".into());
    Some(ProviderDefinition {
        kind,
        base_url: base_url.parse().unwrap(),
        model,
        api_key: std::env::var("LODESTONE_LLM_API_KEY").ok().map(Secret::new),
        timeout: Duration::from_secs(180),
        reasoning_effort: None,
        generation: GenerationParams::default(),
    })
}

async fn run_case<P: LlmProvider>(provider: &P) -> Result<(), String> {
    let case: Value = serde_json::from_str(
        &fs::read_to_string(
            root().join("tests/fixtures/prompts/qualification_legacy_keycloak.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let raw =
        fs::read_to_string(root().join("../../docs/examples/profiles/iam-ferriskey.json")).unwrap();
    let profile = ValidatedProfile::parse(serde_json::from_str(&raw).unwrap()).unwrap();
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
    let schema = AggregatorOutput::schema().unwrap();
    let built = aggregation_prompt(
        profile.definition(),
        &profile.definition().aggregator,
        &schema,
    );
    let instruction = aggregation_instruction(
        Facts {
            confidence: Some(88.0),
            verdict: Some(Verdict::Reliable),
            score: Some(88),
        },
        docs.len(),
    );
    let prompt = Prompt {
        system: &built.system,
        instruction: &instruction,
        documents: &untrusted,
        max_tokens: 4096,
        temperature: None,
    };
    let settings = CallSettings {
        max_retries: 2,
        pricing: TokenPricing {
            input_cents_per_mtok: 0,
            output_cents_per_mtok: 0,
        },
        base_delay: Duration::from_secs(2),
    };
    let mut budget = RunBudget::new(NonZeroU32::new(100).unwrap());
    let structured =
        call_structured::<AggregatorOutput, _>(provider, prompt, settings, &mut budget)
            .await
            .map_err(|error| error.to_string())?;
    let check = check_evidence(&structured.value.evidence, &untrusted);
    println!(
        "evidence={} verbatim={} rejected={}",
        structured.value.evidence.len(),
        check.verified.len(),
        check.rejected
    );
    Ok(())
}

#[tokio::test]
async fn the_aggregator_follows_the_contract_on_a_real_provider() {
    let Some(definition) = definition_from_env() else {
        println!("skipped: set LODESTONE_LLM_LIVE=1");
        return;
    };
    let result = match definition.kind {
        ProviderKind::Anthropic => {
            run_case(&AnthropicProvider::new(definition.clone()).unwrap()).await
        }
        _ => run_case(&OpenAiCompatibleProvider::new(&definition).unwrap()).await,
    };
    result.unwrap();
}
