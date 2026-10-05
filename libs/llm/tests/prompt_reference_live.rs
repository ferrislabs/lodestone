use std::{fs, num::NonZeroU32, path::PathBuf, time::Duration};

use common::Secret;
use llm::{anthropic::AnthropicProvider, openai::OpenAiCompatibleProvider};
use lodestone_core::{
    LlmProvider, PromptTemplate, ProviderDefinition, ProviderKind, QUALIFICATION_V1,
    QualificationOutput, TRIAGE_V1, TokenPricing, TriageVerdict, UntrustedDocument,
    ValidatedProfile,
    llm::gateway::{CallSettings, Prompt, RunBudget, call_structured},
};
use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core")
}

fn cases() -> Vec<Value> {
    let mut entries: Vec<_> = fs::read_dir(root().join("tests/fixtures/prompts"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    entries
        .iter()
        .map(|path| serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap())
        .collect()
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
    })
}

async fn run_case<P: LlmProvider>(provider: &P, case: &Value) -> Result<(), String> {
    let name = case["name"].as_str().unwrap();
    let raw =
        fs::read_to_string(root().join("../../docs/examples/profiles/iam-ferriskey.json")).unwrap();
    let mut profile: Value = serde_json::from_str(&raw).unwrap();
    if let Some(criteria) = case.get("profile_relevance_criteria") {
        profile["llm"]["relevance_criteria"] = criteria.clone();
    }
    let definition = ValidatedProfile::parse(profile)
        .unwrap()
        .definition()
        .clone();
    let template: PromptTemplate = if case["template"] == "triage" {
        TRIAGE_V1
    } else {
        QUALIFICATION_V1
    };
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
    let system = template.system(&definition);
    let prompt = Prompt {
        system: &system,
        instruction: template.instruction(),
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
    let mut budget = RunBudget::new(NonZeroU32::new(100).unwrap());
    if template.id == "triage" {
        let verdict = call_structured::<TriageVerdict, _>(provider, prompt, settings, &mut budget)
            .await
            .map_err(|e| format!("{name}: {e}"))?
            .value;
        let accepted = verdict.is_supported_relevance(&untrusted);
        println!(
            "{name}: model_relevant={} accepted_relevant={accepted} evidence={} email={:?}",
            verdict.relevant,
            verdict.evidence.len(),
            verdict.contact_email
        );
        if Some(accepted) != case["expect"]["relevant"].as_bool() {
            return Err(format!("{name}: unexpected accepted verdict {accepted}"));
        }
        if verdict
            .contact_email
            .as_deref()
            .is_some_and(|email| email.contains("evil"))
        {
            return Err(format!("{name}: injected address returned"));
        }
    } else {
        let output =
            call_structured::<QualificationOutput, _>(provider, prompt, settings, &mut budget)
                .await
                .map_err(|e| format!("{name}: {e}"))?
                .value;
        let verbatim = output
            .evidence
            .iter()
            .filter(|q| {
                docs.get(q.document_index as usize)
                    .is_some_and(|d| d.1.contains(&q.quote))
            })
            .count();
        println!(
            "{name}: evidence={} verbatim={verbatim}",
            output.evidence.len()
        );
    }
    Ok(())
}

#[tokio::test]
async fn reference_cases_against_a_real_provider() {
    let Some(definition) = definition_from_env() else {
        println!(
            "skipped: set LODESTONE_LLM_LIVE=1 (and LODESTONE_LLM_KIND, LODESTONE_LLM_BASE_URL, LODESTONE_LLM_MODEL, LODESTONE_LLM_API_KEY)"
        );
        return;
    };
    let mut failures = Vec::new();
    for case in cases() {
        let result = match definition.kind {
            ProviderKind::Anthropic => {
                run_case(&AnthropicProvider::new(definition.clone()).unwrap(), &case).await
            }
            ProviderKind::Mistral | ProviderKind::OpenAiCompatible => {
                run_case(&OpenAiCompatibleProvider::new(&definition).unwrap(), &case).await
            }
        };
        if let Err(failure) = result {
            failures.push(failure);
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
