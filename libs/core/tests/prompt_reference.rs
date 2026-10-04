use std::{fs, num::NonZeroU32, path::PathBuf, time::Duration};

use lodestone_core::{
    LlmError, LlmProvider, LlmRequest, LlmResponse, PromptTemplate, QUALIFICATION_V1,
    QualificationOutput, TRIAGE_V1, TokenPricing, TokenUsage, TriageVerdict, UntrustedDocument,
    ValidatedProfile,
    definition::ProfileDefinition,
    llm::gateway::{CallSettings, Prompt, RunBudget, call_structured},
    render_user_content,
};
use serde_json::Value;

struct Replay {
    output: Value,
}

impl LlmProvider for Replay {
    async fn complete(&self, _request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        Ok(LlmResponse {
            output: self.output.clone(),
            model: "recorded".to_owned(),
            usage: TokenUsage::default(),
        })
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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

fn iam_profile(case: &Value) -> ProfileDefinition {
    let raw =
        fs::read_to_string(root().join("../../docs/examples/profiles/iam-ferriskey.json")).unwrap();
    let mut value: Value = serde_json::from_str(&raw).unwrap();
    if let Some(criteria) = case.get("profile_relevance_criteria") {
        value["llm"]["relevance_criteria"] = criteria.clone();
    }
    ValidatedProfile::parse(value).unwrap().definition().clone()
}

fn template(case: &Value) -> PromptTemplate {
    match case["template"].as_str().unwrap() {
        "triage" => TRIAGE_V1,
        "qualification" => QUALIFICATION_V1,
        other => panic!("unknown template {other}"),
    }
}

fn documents(case: &Value) -> Vec<(String, String)> {
    case["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["source"].as_str().unwrap().to_owned(),
                d["content"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn settings() -> CallSettings {
    CallSettings {
        max_retries: 0,
        pricing: TokenPricing {
            input_cents_per_mtok: 0,
            output_cents_per_mtok: 0,
        },
        base_delay: Duration::ZERO,
    }
}

fn assert_quotes_are_verbatim(
    evidence: &[lodestone_core::EvidenceQuote],
    docs: &[(String, String)],
    name: &str,
) {
    for quote in evidence {
        let document = &docs[quote.document_index as usize].1;
        assert!(
            document.contains(&quote.quote),
            "{name}: quote not verbatim"
        );
    }
}

#[test]
fn every_case_keeps_instructions_out_of_data() {
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        let profile = iam_profile(&case);
        let template = template(&case);
        let system = template.system(&profile);
        assert_eq!(system.matches("\n<market_context>\n").count(), 1, "{name}");
        assert_eq!(system.matches("</market_context>").count(), 1, "{name}");

        let docs = documents(&case);
        let untrusted: Vec<UntrustedDocument<'_>> = docs
            .iter()
            .map(|(source, content)| UntrustedDocument { source, content })
            .collect();
        let schema = serde_json::json!({});
        let user = render_user_content(&LlmRequest {
            system: &system,
            instruction: template.instruction(),
            documents: &untrusted,
            schema_name: "output",
            schema: &schema,
            max_tokens: 1024,
        });
        assert_eq!(user.matches("</document>\n").count(), docs.len(), "{name}");
        let last_block_end = user.rfind("</document>\n").unwrap() + "</document>\n".len();
        assert_eq!(&user[last_block_end..], template.instruction(), "{name}");
    }
}

#[tokio::test]
async fn recorded_outputs_have_the_expected_shape_and_verdict() {
    for case in cases() {
        let name = case["name"].as_str().unwrap().to_owned();
        let provider = Replay {
            output: case["recorded_output"].clone(),
        };
        let docs = documents(&case);
        let untrusted: Vec<UntrustedDocument<'_>> = docs
            .iter()
            .map(|(source, content)| UntrustedDocument { source, content })
            .collect();
        let profile = iam_profile(&case);
        let template = template(&case);
        let system = template.system(&profile);
        let prompt = Prompt {
            system: &system,
            instruction: template.instruction(),
            documents: &untrusted,
            max_tokens: 2048,
        };
        let mut budget = RunBudget::new(NonZeroU32::new(1).unwrap());
        match template.id {
            "triage" => {
                let verdict =
                    call_structured::<TriageVerdict, _>(&provider, prompt, settings(), &mut budget)
                        .await
                        .unwrap()
                        .value;
                assert_eq!(
                    Some(verdict.is_supported_relevance(&untrusted)),
                    case["expect"]["relevant"].as_bool(),
                    "{name}"
                );
                assert_quotes_are_verbatim(&verdict.evidence, &docs, &name);
                if let Some(email) = verdict.contact_email {
                    assert!(!email.contains("evil"), "{name}");
                }
            }
            _ => {
                let output = call_structured::<QualificationOutput, _>(
                    &provider,
                    prompt,
                    settings(),
                    &mut budget,
                )
                .await
                .unwrap()
                .value;
                assert!(!output.evidence.is_empty(), "{name}");
                assert_quotes_are_verbatim(&output.evidence, &docs, &name);
            }
        }
    }
}
