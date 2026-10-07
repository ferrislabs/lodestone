use std::{fs, num::NonZeroU32, path::PathBuf, time::Duration};

use chrono::DateTime;
use llm::openai::OpenAiCompatibleProvider;
use lodestone_core::{
    AgentDecision, CacheScope, CachedOutput, Calibrations, EvidenceComponents, EvidenceStrength,
    GenerationParams, LeadId, LlmCache, ProfileId, ProviderDefinition, ProviderKind,
    QualificationResult, ReasoningEffort, ResultId, ResultStatus, RevisionNumber, RunProvenance,
    ScoringOutput, TokenPricing, UntrustedDocument, ValidatedProfile, combine,
    llm::gateway::{CallSettings, Prompt, RunBudget, SchemaCall, call_with_schema},
    render_indicators, render_report, render_report_evidence, report_source,
    scoring_contract_schema, scoring_prompt, scoring_violations, validate_scoring, with_ungrounded,
};
use serde_json::{Value, json};

const NIL: &str = "00000000-0000-0000-0000-000000000000";

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

fn mocked_result(
    agent: &lodestone_core::definition::QualificationAgent,
    quote: &str,
) -> QualificationResult {
    let mut answers = serde_json::Map::new();
    for question in &agent.questions {
        let chosen = &question.options[0];
        let probabilities: serde_json::Map<String, Value> = question
            .options
            .iter()
            .map(|option| {
                (
                    option.value.clone(),
                    json!(if option.value == chosen.value {
                        0.9
                    } else {
                        0.1 / 3.0
                    }),
                )
            })
            .collect();
        answers.insert(
            question.id.clone(),
            json!({
                "type": "choice",
                "choice": chosen.value,
                "evidence": quote,
                "grounded": true,
                "confidence": 0.9,
                "agreement": 1.0,
                "probabilities": probabilities
            }),
        );
    }
    QualificationResult {
        id: NIL.parse::<ResultId>().unwrap(),
        lead_id: NIL.parse::<LeadId>().unwrap(),
        profile_id: NIL.parse::<ProfileId>().unwrap(),
        profile_revision: RevisionNumber::FIRST,
        agent_id: agent.id.clone(),
        provenance: RunProvenance::legacy("mocked".to_owned(), String::new()),
        status: ResultStatus::Valid,
        summary: Some(format!("Mocked report of {}.", agent.name)),
        answers: Value::Object(answers),
        agent_score: Some(0.9),
        error: None,
        escalated: false,
        raw_samples: json!([]),
        created_at: DateTime::from_timestamp(0, 0).unwrap(),
    }
}

#[tokio::test]
async fn the_scoring_agent_answers_the_imposed_contract_live() {
    if std::env::var("LODESTONE_SCORING_LIVE").ok().as_deref() != Some("1") {
        println!(
            "skipped: set LODESTONE_SCORING_LIVE=1 (LODESTONE_LLM_BASE_URL, LODESTONE_LLM_MODEL)"
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
    let signals: Vec<(String, String)> = case["documents"]
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
    let raw =
        fs::read_to_string(core_root().join("../../docs/examples/profiles/iam-ferriskey.json"))
            .unwrap();
    let profile = ValidatedProfile::parse(serde_json::from_str(&raw).unwrap()).unwrap();
    let definition = profile.definition();
    let quote = signals[1].1.clone();
    let mocked: Vec<(String, QualificationResult)> = definition
        .qualification_agents
        .iter()
        .take(2)
        .map(|agent| (report_source(agent), mocked_result(agent, &quote)))
        .collect();
    let reports: Vec<(String, String)> = definition
        .qualification_agents
        .iter()
        .take(2)
        .zip(&mocked)
        .map(|(agent, (source, result))| (source.clone(), render_report(agent, Some(result), &[])))
        .collect();
    let evidence_texts: Vec<(String, String)> = definition
        .qualification_agents
        .iter()
        .take(2)
        .zip(&mocked)
        .map(|(agent, (source, result))| {
            (source.clone(), render_report_evidence(agent, Some(result)))
        })
        .collect();
    let evidence = EvidenceStrength {
        score: 0.62,
        components: EvidenceComponents {
            volume: 0.55,
            diversity: 1.0,
            environment: 1.0,
            strength: 0.3,
        },
        signal_count: 2,
        signal_score: 14.0,
        only_non_production: false,
        independent_signals: 2,
        decayed_signals: 1.9,
        source_count: 2,
        kind_count: 2,
    };
    let indicators = render_indicators(Some(0.9), &evidence, Some(68.0));
    let overview = "lead: example-retail.fr\nFacts computed by the system from stored data:\n- hosts carrying signals: 1 (sso.example-retail.fr (production))\n- production hosts: 1 of 1\n- signals: 2 from 2 distinct source(s)".to_owned();
    let mut owned: Vec<(String, String)> = vec![("lead".to_owned(), overview)];
    owned.extend(signals);
    let grounding_count = owned.len();
    owned.extend(reports);
    owned.push(("indicators".to_owned(), indicators));
    let documents: Vec<UntrustedDocument<'_>> = owned
        .iter()
        .map(|(source, content)| UntrustedDocument { source, content })
        .collect();
    let mut grounding: Vec<UntrustedDocument<'_>> = documents[..grounding_count].to_vec();
    grounding.extend(
        evidence_texts
            .iter()
            .map(|(source, content)| UntrustedDocument { source, content }),
    );
    let agent = &definition.scoring_agents[0];
    let schema = scoring_contract_schema(agent);
    let built = scoring_prompt(definition, agent, &schema);
    let settings = CallSettings {
        max_retries: 2,
        pricing: TokenPricing {
            input_cents_per_mtok: 0,
            output_cents_per_mtok: 0,
        },
        base_delay: Duration::from_secs(2),
    };
    let mut budget = RunBudget::new(NonZeroU32::new(10).unwrap());
    let scope = CacheScope {
        profile_id: NIL.parse::<ProfileId>().unwrap(),
        revision: RevisionNumber::FIRST,
        prompt: format!("scoring@{}", built.hash),
        domain: lodestone_core::NormalizedDomain::from_host("example-retail.fr").unwrap(),
        agent: Some(agent.id.clone()),
        sample: 0,
    };
    let mut instruction = built.instruction.clone();
    let mut attempt = 0;
    let mut output: Option<ScoringOutput> = None;
    while attempt < 2 {
        attempt += 1;
        let call = SchemaCall {
            scope: &scope,
            prompt: Prompt {
                system: &built.system,
                instruction: &instruction,
                documents: &documents,
                max_tokens: 2048,
                temperature: None,
            },
            schema: &schema,
        };
        let structured = call_with_schema(
            &provider,
            &NoCache,
            call,
            |value| validate_scoring(agent, value, &grounding).is_ok(),
            settings,
            &mut budget,
        )
        .await
        .unwrap();
        println!(
            "model: {} (requested {model}), attempt {attempt}",
            structured.model
        );
        println!(
            "raw output: {}",
            serde_json::to_string(&structured.value).unwrap()
        );
        match validate_scoring(agent, &structured.value, &grounding) {
            Ok(valid) => {
                let ungrounded = valid.criteria.ungrounded();
                if attempt == 1 && !ungrounded.is_empty() {
                    println!("ungrounded on first attempt: {ungrounded:?}");
                    instruction = with_ungrounded(&built.instruction, &ungrounded);
                    output = Some(valid);
                    continue;
                }
                output = Some(valid);
                break;
            }
            Err(violations) => {
                println!("contract violated on attempt {attempt}: {violations}");
                instruction = scoring_violations(&built.instruction, &violations);
            }
        }
    }
    let output = output.expect("the scoring agent answered the contract within one retry");
    println!(
        "validated scoring output: {}",
        serde_json::to_string_pretty(&output.to_json()).unwrap()
    );
    assert!(output.score <= 100);
    assert!((1..=5).contains(&output.reasons.len()));
    assert!(
        output
            .reasons
            .iter()
            .all(|reason| reason.chars().count() <= 200)
    );
    let combined = combine(
        &[AgentDecision {
            agent,
            score: output.score,
            probabilities: output.verdict.probabilities,
        }],
        &Calibrations::none(),
        Vec::new(),
    );
    println!("combination: {combined:#?}");
    assert_eq!(combined.score, Some(output.score));
    assert_eq!(combined.verdict, Some(output.verdict.choice));
}
