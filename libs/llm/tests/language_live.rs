use std::{fs, num::NonZeroU32, path::PathBuf, time::Duration};

use llm::openai::OpenAiCompatibleProvider;
use lodestone_core::{
    AggregatorOutput, CacheScope, CachedOutput, Facts, LeadId, LlmCache, ProfileId,
    ProviderDefinition, ProviderKind, QualificationResult, ReasoningEffort, ResultId, ResultStatus,
    RevisionNumber, TokenPricing, UntrustedDocument, ValidatedProfile, Verdict,
    aggregation_instruction, aggregation_prompt, check_evidence,
    definition::AgentId,
    llm::gateway::{CallSettings, Prompt, RunBudget, SchemaCall, call_with_schema},
    render_agent,
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

fn fake_result(
    agent: &str,
    question: &str,
    choice: &str,
    probabilities: Value,
    score: f64,
) -> QualificationResult {
    QualificationResult {
        id: NIL.parse::<ResultId>().unwrap(),
        lead_id: NIL.parse::<LeadId>().unwrap(),
        profile_id: NIL.parse::<ProfileId>().unwrap(),
        profile_revision: RevisionNumber::FIRST,
        agent_id: AgentId::new(agent),
        provider_id: None,
        model: "gemma_laya".to_owned(),
        prompt_hash: String::new(),
        status: ResultStatus::Valid,
        summary: Some("Résumé de l'agent.".to_owned()),
        answers: json!({question: {
            "type": "choice",
            "choice": choice,
            "confidence": 0.92,
            "probabilities": probabilities
        }}),
        agent_score: Some(score),
        error: None,
        created_at: std::time::SystemTime::now().into(),
    }
}

#[tokio::test]
async fn the_aggregator_summarises_an_iam_lead_in_french_live() {
    if std::env::var("LODESTONE_AGGREGATOR_LIVE").ok().as_deref() != Some("1") {
        println!(
            "skipped: set LODESTONE_AGGREGATOR_LIVE=1 (LODESTONE_LLM_BASE_URL, LODESTONE_LLM_MODEL)"
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
    let agents = &profile.definition().qualification_agents;
    let relevance = &agents[0];
    let maturity = &agents[1];
    let results = [
        fake_result(
            relevance.id.as_str(),
            &relevance.questions[0].id,
            "forte",
            json!({"forte": 0.92, "moyenne": 0.06, "faible": 0.02}),
            0.95,
        ),
        fake_result(
            maturity.id.as_str(),
            &maturity.questions[0].id,
            "legacy_auto_heberge",
            json!({"legacy_auto_heberge": 0.9, "saas": 0.05, "inconnu": 0.05}),
            0.93,
        ),
    ];
    let overview = "lead: example-retail.fr\nfirst seen: 2026-10-06T08:00:00Z".to_owned();
    let blocks = [
        render_agent(relevance, Some(&results[0])),
        render_agent(maturity, Some(&results[1])),
    ];
    let mut documents: Vec<UntrustedDocument<'_>> = signals
        .iter()
        .map(|(source, content)| UntrustedDocument { source, content })
        .collect();
    documents.push(UntrustedDocument {
        source: "lead",
        content: &overview,
    });
    documents.push(UntrustedDocument {
        source: "qualification:pertinence",
        content: &blocks[0],
    });
    documents.push(UntrustedDocument {
        source: "qualification:maturite-technique",
        content: &blocks[1],
    });
    let schema = AggregatorOutput::schema().unwrap();
    let built = aggregation_prompt(
        profile.definition(),
        &profile.definition().aggregator,
        &schema,
    );
    let instruction = aggregation_instruction(
        Facts {
            confidence: Some(94.0),
            verdict: Some(Verdict::Reliable),
        },
        signals.len(),
    );
    let scope = CacheScope {
        profile_id: NIL.parse::<ProfileId>().unwrap(),
        revision: RevisionNumber::FIRST,
        prompt: built.tag.clone(),
        domain: lodestone_core::NormalizedDomain::from_host("example-retail.fr").unwrap(),
        agent: None,
    };
    let call = SchemaCall {
        scope: &scope,
        prompt: Prompt {
            system: &built.system,
            instruction: &instruction,
            documents: &documents,
            max_tokens: 4096,
        },
        schema: &schema,
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
    let structured = call_with_schema(
        &provider,
        &NoCache,
        call,
        |value| AggregatorOutput::parse(value).is_ok(),
        settings,
        &mut budget,
    )
    .await
    .unwrap();
    println!("model: {}", structured.model);
    let output = AggregatorOutput::parse(&structured.value)
        .unwrap_or_else(|problem| panic!("contract violated: {problem}"));
    println!("summary: {}", output.summary);
    println!("verdict_narrative: {}", output.verdict_narrative);
    println!("approach_angle: {}", output.approach_angle);
    println!("draft subject: {}", output.draft_message.subject);
    println!("draft body: {}", output.draft_message.body);
    let check = check_evidence(&output.evidence, &documents[..signals.len()]);
    println!(
        "evidence: {} quotes, {} verbatim, {} rejected",
        output.evidence.len(),
        check.verified.len(),
        check.rejected
    );
}
