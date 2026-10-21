use std::{fs, num::NonZeroU32, path::PathBuf, time::Duration};

use llm::openai::OpenAiCompatibleProvider;
use lodestone_core::{
    CacheScope, CachedOutput, GenerationParams, LlmCache, ProfileId, ProviderDefinition,
    ProviderKind, ReasoningEffort, RevisionNumber, TokenPricing, UntrustedDocument, ValidOutput,
    ValidatedProfile, agent_prompt, agent_score, contract_schema,
    llm::gateway::{CallSettings, Prompt, RunBudget, SchemaCall, call_with_schema},
    validate_answers, vote, with_ungrounded, with_violations,
};
use serde_json::Value;

const LIVE_AGENTS: usize = 2;
const LIVE_SAMPLES: u32 = 3;
const LIVE_TEMPERATURE: f64 = 0.7;

struct NoCache;

impl LlmCache for NoCache {
    async fn get(&self, _key: &[u8]) -> Result<Option<CachedOutput>, common::CoreError> {
        Ok(None)
    }

    async fn put(
        &self,
        _key: &[u8],
        _scope: &CacheScope,
        _entry: &CachedOutput,
    ) -> Result<(), common::CoreError> {
        Ok(())
    }
}

fn core_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core")
}

#[tokio::test]
async fn the_pertinence_agent_answers_the_contract_live() {
    if std::env::var("LODESTONE_QUALIF_LIVE").ok().as_deref() != Some("1") {
        println!(
            "skipped: set LODESTONE_QUALIF_LIVE=1 (LODESTONE_LLM_BASE_URL, LODESTONE_LLM_MODEL)"
        );
        return;
    }
    let model = std::env::var("LODESTONE_LLM_MODEL").unwrap_or_else(|_| "gemma_laya".into());
    let definition = ProviderDefinition {
        kind: ProviderKind::OpenAiCompatible,
        base_url: std::env::var("LODESTONE_LLM_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11434".into())
            .parse()
            .unwrap(),
        model: model.clone(),
        api_key: None,
        timeout: Duration::from_secs(300),
        reasoning_effort: Some(ReasoningEffort::None),
        generation: GenerationParams::default(),
    };
    let provider = OpenAiCompatibleProvider::new(&definition).unwrap();
    let case: Value = serde_json::from_str(
        &fs::read_to_string(
            core_root().join("tests/fixtures/prompts/qualification_legacy_keycloak.json"),
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
    let documents: Vec<UntrustedDocument<'_>> = docs
        .iter()
        .map(|(source, content)| UntrustedDocument { source, content })
        .collect();
    let raw =
        fs::read_to_string(core_root().join("../../docs/examples/profiles/iam-ferriskey.json"))
            .unwrap();
    let profile = ValidatedProfile::parse(serde_json::from_str(&raw).unwrap()).unwrap();
    let settings = CallSettings {
        max_retries: 2,
        pricing: TokenPricing {
            input_cents_per_mtok: 0,
            output_cents_per_mtok: 0,
        },
        base_delay: Duration::from_secs(2),
    };
    let mut budget = RunBudget::new(NonZeroU32::new(10).unwrap());
    let mut checked = 0;
    let mut failed: Vec<String> = Vec::new();
    for agent in profile
        .definition()
        .qualification_agents
        .iter()
        .take(LIVE_AGENTS)
    {
        let schema = contract_schema(agent);
        let built = agent_prompt(profile.definition(), agent, &schema);
        let mut valid: Vec<ValidOutput> = Vec::new();
        for sample in 0..LIVE_SAMPLES {
            let scope = CacheScope {
                profile_id: "00000000-0000-0000-0000-000000000000"
                    .parse::<ProfileId>()
                    .unwrap(),
                revision: RevisionNumber::FIRST,
                prompt: format!("qualification@{}", built.hash),
                domain: lodestone_core::NormalizedDomain::from_host("acme-keycloak.com").unwrap(),
                agent: Some(agent.id.clone()),
                sample,
                fresh: false,
            };
            let mut instruction = built.instruction.clone();
            let mut attempt = 0;
            let output = loop {
                attempt += 1;
                let call = SchemaCall {
                    scope: &scope,
                    prompt: Prompt {
                        system: &built.system,
                        instruction: &instruction,
                        documents: &documents,
                        max_tokens: 2048,
                        temperature: Some(LIVE_TEMPERATURE),
                    },
                    schema: &schema,
                };
                let structured = call_with_schema(
                    &provider,
                    &NoCache,
                    call,
                    |output| validate_answers(agent, output, &documents).is_ok(),
                    settings,
                    &mut budget,
                )
                .await
                .unwrap();
                println!(
                    "agent {} sample {sample} model: {} (requested {model}), attempt {attempt}",
                    agent.id.as_str(),
                    structured.model
                );
                println!(
                    "raw output: {}",
                    serde_json::to_string(&structured.value).unwrap()
                );
                let output = match validate_answers(agent, &structured.value, &documents) {
                    Ok(output) => output,
                    Err(violations) => {
                        println!("contract violated on attempt {attempt}: {violations}");
                        if attempt == 1 {
                            instruction = with_violations(&built.instruction, &violations);
                            continue;
                        }
                        break None;
                    }
                };
                let ungrounded = output.answers.ungrounded();
                if attempt == 1 && !ungrounded.is_empty() {
                    println!("ungrounded on first attempt: {ungrounded:?}");
                    instruction = with_ungrounded(&built.instruction, &ungrounded);
                    continue;
                }
                break Some(output);
            };
            match output {
                Some(output) => {
                    for answer in &output.answers.0 {
                        println!(
                            "  sample {sample} {}: choice={} grounded={} confidence={:.2}",
                            answer.question_id, answer.choice, answer.grounded, answer.confidence
                        );
                    }
                    valid.push(output);
                }
                None => println!("  sample {sample} dropped"),
            }
        }
        let Some(output) = vote(agent, &valid) else {
            failed.push(agent.id.as_str().to_owned());
            continue;
        };
        println!(
            "agent: {} ({}) voted over {}/{LIVE_SAMPLES} samples",
            agent.name,
            agent.description,
            valid.len()
        );
        println!("summary: {}", output.summary);
        assert!(!output.summary.is_empty());
        assert!(output.summary.chars().count() <= 600);
        for answer in &output.answers.0 {
            let votes: Vec<&str> = valid
                .iter()
                .filter_map(|sample| {
                    sample
                        .answers
                        .0
                        .iter()
                        .find(|own| own.question_id == answer.question_id)
                        .map(|own| own.choice.as_str())
                })
                .collect();
            println!(
                "axis {}: votes={votes:?} final={} agreement={:.2} grounded={} source={:?} evidence={:?}",
                answer.question_id,
                answer.choice,
                answer.agreement.unwrap_or_default(),
                answer.grounded,
                answer.evidence_source,
                answer.evidence
            );
            assert!(answer.agreement.is_some());
            assert!(answer.evidence.chars().count() <= 300);
        }
        println!(
            "voted answers: {}",
            serde_json::to_string_pretty(&output.answers.to_json()).unwrap()
        );
        println!(
            "agent score: {:.4}",
            agent_score(agent, &output.answers, 0.05)
        );
        checked += 1;
    }
    assert!(checked > 0);
    assert!(
        failed.is_empty(),
        "agents without any valid sample: {failed:?}"
    );
}
