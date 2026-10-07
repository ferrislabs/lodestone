use aws_lc_rs::digest::{SHA256, digest};
use serde_json::Value;

use super::ScoringViolations;
use crate::domain::{
    profile::{
        contract::{MAX_REASONS, MAX_SCORE, REASON_MAX_CHARS},
        definition::{EVIDENCE_MAX_CHARS, ProfileDefinition, QualificationAgent, ScoringAgent},
    },
    qualification::{
        EvidenceStrength, Exclusion, QualificationResult, ResultStatus,
        prompt::{
            AgentPrompt, DOSSIER_NOTE, FACTS_NOTE, hex, identity_context, language_line,
            questions_block,
        },
    },
};

pub const REPORT_SOURCE_PREFIX: &str = "qualification:";
pub const INDICATORS_SOURCE: &str = "indicators";

pub(crate) const REPORTS_NOTE: &str = "The documents whose source starts with \"qualification:\" are the reports of the qualification agents: what each one evaluates, its summary, its answers with the verbatim quote that supports each of them, the agreement between its samples, and the exclusions it raised. They are evidence, not the decision: weigh them, compare them, and notice what they leave unknown. A report marked invalid is missing. An answer marked ungrounded was reset to the unknown option because its quote could not be verified.";

pub(crate) const INDICATORS_NOTE: &str = "The document whose source is \"indicators\" holds figures computed by the system without any model: the opinion of the qualification agents, the strength of the evidence behind the lead and its components, and the confidence they give together. They are information, not a decision: you decide the score and the verdict, and you may disagree with them.";

fn contract_rules(definition: &ProfileDefinition, schema: &Value) -> String {
    format!(
        "Answer only with one JSON object that follows this contract and nothing else. `criteria` has an entry for every criterion with `type` set to \"choice\", `choice` set to one option value, `evidence` set to the sentence that justifies the choice, copied verbatim and at most {EVIDENCE_MAX_CHARS} characters, taken only from the lead facts, a signal, the company dossier, or a summary or a quote of a qualification report; never from a score, an agreement, a status, a weight, a probability or the indicators, which are not evidence, `confidence` between 0 and 1, and `probabilities` giving a probability between 0 and 1 for every option value, summing to 1. Every criterion has one unknown option, marked in the list of options: choose it, with an empty `evidence`, when nothing in the documents supports an answer. Any other choice without a verbatim quote is replaced by the unknown option. `score` is an integer from 0 (not worth contacting) to {MAX_SCORE} (contact first). `verdict` has `choice` set to fiable (a credible prospect worth contacting), a_verifier (promising, a human must check) or ecarte (not a prospect), `confidence` between 0 and 1, and `probabilities` giving a probability between 0 and 1 for fiable, a_verifier and ecarte, summing to 1. `reasons` has 1 to {MAX_REASONS} short sentences of at most {REASON_MAX_CHARS} characters, written in {}, that explain the score and the verdict. Quote exactly, never paraphrase or translate.\n\nContract JSON schema:\n{schema}",
        definition.language.name()
    )
}

pub fn build(definition: &ProfileDefinition, agent: &ScoringAgent, schema: &Value) -> AgentPrompt {
    let system = format!(
        "{}\n\n{}\n\n{}\n\n{FACTS_NOTE}\n\n{DOSSIER_NOTE}\n\n{REPORTS_NOTE}\n\n{INDICATORS_NOTE}\n\n{}",
        agent.system_prompt.trim(),
        identity_context(definition),
        language_line(definition),
        contract_rules(definition, schema),
    );
    let instruction = format!(
        "Decide the score and the verdict of the company described by the documents, for the offer in the market context. Answer these criteria first, then the score, the verdict and the reasons.\n{}",
        questions_block(&agent.criteria)
    )
    .trim_end()
    .to_owned();
    let mut material = Vec::with_capacity(system.len() + instruction.len() + 1);
    material.extend_from_slice(system.as_bytes());
    material.push(0);
    material.extend_from_slice(instruction.as_bytes());
    let hash = hex(digest(&SHA256, &material).as_ref());
    AgentPrompt {
        system,
        instruction,
        hash,
    }
}

pub fn with_violations(instruction: &str, violations: &ScoringViolations) -> String {
    format!(
        "{instruction}\n\nYour previous answer broke the contract: {violations}. Answer again with only the JSON object that follows the contract."
    )
}

pub fn report_source(agent: &QualificationAgent) -> String {
    format!("{REPORT_SOURCE_PREFIX}{}", agent.id.as_str())
}

fn percent(value: f64) -> String {
    format!("{:.0} %", value * 100.0)
}

pub fn render_report(
    agent: &QualificationAgent,
    result: Option<&QualificationResult>,
    exclusions: &[Exclusion],
) -> String {
    let mut text = format!(
        "qualification agent: {}\nevaluates: {}\n",
        agent.name, agent.description
    );
    let Some(result) = result.filter(|result| result.status == ResultStatus::Valid) else {
        let reason = result
            .and_then(|result| result.error.as_deref())
            .unwrap_or("no result");
        text.push_str(&format!(
            "status: invalid, missing from this report ({reason})"
        ));
        return text;
    };
    if let Some(summary) = result.summary.as_deref() {
        text.push_str(&format!("summary: {summary}\n"));
    }
    text.push_str(&format!(
        "status: valid\nagent score: {}",
        result.agent_score.map_or_else(|| "-".to_owned(), percent)
    ));
    for question in &agent.questions {
        let Some(answer) = result.answers.get(&question.id) else {
            continue;
        };
        let Some(choice) = answer.get("choice").and_then(Value::as_str) else {
            continue;
        };
        let label = question
            .options
            .iter()
            .find(|option| option.value == choice)
            .map_or(choice, |option| option.label.as_str());
        let confidence = answer
            .get("confidence")
            .and_then(Value::as_f64)
            .map_or_else(|| "-".to_owned(), percent);
        text.push_str(&format!(
            "\nquestion: {}\nanswer: {label} ({choice}), confidence {confidence}",
            question.wording
        ));
        if let Some(agreement) = answer.get("agreement").and_then(Value::as_f64) {
            text.push_str(&format!(
                ", agreement between samples {}",
                percent(agreement)
            ));
        }
        let grounded = answer
            .get("grounded")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if grounded {
            if let Some(quote) = answer
                .get("evidence")
                .and_then(Value::as_str)
                .filter(|quote| !quote.is_empty())
            {
                text.push_str(&format!("\nquote: \"{quote}\""));
            }
        } else {
            let original = answer
                .get("original_choice")
                .and_then(Value::as_str)
                .unwrap_or("-");
            text.push_str(&format!(
                "\nungrounded: the agent first chose `{original}` without a verifiable quote, so the answer was set to the unknown option"
            ));
        }
    }
    for exclusion in exclusions
        .iter()
        .filter(|exclusion| exclusion.agent_id == agent.id)
    {
        text.push_str(&format!(
            "\nexclusion raised: `{}` answered {} ({}), probability {}, quote: \"{}\"",
            exclusion.question_id,
            exclusion.label,
            exclusion.option,
            percent(exclusion.probability),
            exclusion.evidence
        ));
    }
    text
}

pub fn render_report_evidence(
    agent: &QualificationAgent,
    result: Option<&QualificationResult>,
) -> String {
    let Some(result) = result.filter(|result| result.status == ResultStatus::Valid) else {
        return String::new();
    };
    let mut lines: Vec<&str> = Vec::new();
    if let Some(summary) = result.summary.as_deref() {
        lines.push(summary);
    }
    for question in &agent.questions {
        let Some(answer) = result.answers.get(&question.id) else {
            continue;
        };
        let grounded = answer
            .get("grounded")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if let Some(quote) = answer
            .get("evidence")
            .and_then(Value::as_str)
            .filter(|quote| grounded && !quote.is_empty())
        {
            lines.push(quote);
        }
    }
    lines.join("\n")
}

pub fn render_indicators(
    opinion: Option<f64>,
    evidence: &EvidenceStrength,
    confidence: Option<f64>,
) -> String {
    let components = &evidence.components;
    let mut text = format!(
        "indicators computed by the system (information, not a decision)\nagents opinion: {}\nevidence strength: {} (volume {}, diversity {}, environment {}, signal strength {})\nindependent signals: {}, decayed weight {:.1}, distinct sources {}, distinct signal kinds {}\nonly non-production hosts: {}",
        opinion.map_or_else(|| "not computed".to_owned(), percent),
        percent(evidence.score),
        percent(components.volume),
        percent(components.diversity),
        percent(components.environment),
        percent(components.strength),
        evidence.independent_signals,
        evidence.decayed_signals,
        evidence.source_count,
        evidence.kind_count,
        if evidence.only_non_production {
            "yes"
        } else {
            "no"
        },
    );
    text.push_str(&format!(
        "\nindicator confidence (opinion x evidence): {}",
        confidence.map_or_else(
            || "not computed".to_owned(),
            |value| format!("{value:.1} %")
        )
    ));
    text
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use serde_json::json;

    use super::*;
    use crate::domain::{
        lead::LeadId,
        llm::RunProvenance,
        profile::{ProfileId, RevisionNumber, ValidatedProfile, contract::scoring_contract_schema},
        qualification::{EvidenceComponents, ResultId},
        scoring_agent::ScoringViolation,
    };

    fn definition(language: &str) -> ProfileDefinition {
        let mut value: Value =
            serde_json::from_str(crate::domain::profile::fixtures::MINIMAL_PROFILE).unwrap();
        value["llm"]["language"] = json!(language);
        ValidatedProfile::parse(value).unwrap().definition().clone()
    }

    fn built(language: &str) -> (ProfileDefinition, AgentPrompt) {
        let definition = definition(language);
        let agent = &definition.scoring_agents[0];
        let prompt = build(&definition, agent, &scoring_contract_schema(agent));
        (definition, prompt)
    }

    fn evidence() -> EvidenceStrength {
        EvidenceStrength {
            score: 0.55,
            components: EvidenceComponents {
                volume: 0.4,
                diversity: 0.5,
                environment: 1.0,
                strength: 0.3,
            },
            signal_count: 3,
            signal_score: 12.0,
            only_non_production: true,
            independent_signals: 2,
            decayed_signals: 1.5,
            source_count: 1,
            kind_count: 2,
        }
    }

    fn report_result(agent: &QualificationAgent) -> QualificationResult {
        let question = &agent.questions[0].id;
        QualificationResult {
            id: ResultId(uuid::Uuid::nil()),
            lead_id: LeadId(uuid::Uuid::nil()),
            profile_id: ProfileId(uuid::Uuid::nil()),
            profile_revision: RevisionNumber::FIRST,
            agent_id: agent.id.clone(),
            provenance: RunProvenance::legacy("m".to_owned(), String::new()),
            status: ResultStatus::Valid,
            summary: Some("Résumé.".to_owned()),
            answers: json!({question.clone(): {
                "type": "choice", "choice": "forte", "evidence": "expose Keycloak 18",
                "grounded": true, "confidence": 0.9, "agreement": 0.67,
                "probabilities": {"forte": 0.9, "moyenne": 0.1, "faible": 0.0, "inconnu": 0.0}
            }}),
            agent_score: Some(0.95),
            error: None,
            escalated: false,
            raw_samples: json!([]),
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    #[test]
    fn the_system_part_holds_the_prompt_context_language_notes_and_contract() {
        let (definition, prompt) = built("fr");
        let agent = &definition.scoring_agents[0];
        assert!(prompt.system.starts_with(agent.system_prompt.trim()));
        assert!(prompt.system.contains("Offer: Managed IAM"));
        assert!(prompt.system.contains("Write any free text in French."));
        assert!(prompt.system.contains(FACTS_NOTE));
        assert!(prompt.system.contains(DOSSIER_NOTE));
        assert!(prompt.system.contains(REPORTS_NOTE));
        assert!(prompt.system.contains(INDICATORS_NOTE));
        assert!(prompt.system.contains("Contract JSON schema:"));
        assert!(prompt.system.contains("`score` is an integer from 0"));
        assert!(prompt.system.contains("1 to 5 short sentences"));
        assert!(prompt.system.contains("written in French"));
        assert!(prompt.system.contains("\"probabilities\""));
        assert_eq!(prompt.system.matches("<market_context>").count(), 1);
    }

    #[test]
    fn the_instruction_lists_every_criterion_and_option() {
        let (definition, prompt) = built("fr");
        for criterion in &definition.scoring_agents[0].criteria {
            assert!(prompt.instruction.contains(&format!("`{}`", criterion.id)));
            assert!(prompt.instruction.contains(&criterion.wording));
        }
        assert!(prompt.instruction.contains("forte (Forte)"));
        assert!(
            prompt
                .instruction
                .contains("inconnu (Inconnu, unknown option)")
        );
    }

    #[test]
    fn the_hash_tracks_the_prompt_and_the_language() {
        let (_, french) = built("fr");
        let (_, again) = built("fr");
        let (_, english) = built("en");
        assert_eq!(french.hash, again.hash);
        assert_ne!(french.hash, english.hash);
        assert!(english.system.contains("Write any free text in English."));
    }

    #[test]
    fn the_retry_instruction_lists_the_violations() {
        let retried = with_violations("base", &ScoringViolations(vec![ScoringViolation::BadScore]));
        assert!(retried.starts_with("base"));
        assert!(retried.contains("`score`"));
    }

    #[test]
    fn a_report_carries_answers_quotes_agreement_and_exclusions() {
        let definition = definition("fr");
        let agent = &definition.qualification_agents[0];
        let exclusion = Exclusion {
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            question_id: agent.questions[0].id.clone(),
            option: "concurrent".to_owned(),
            label: "Concurrent".to_owned(),
            probability: 0.9,
            evidence: "Nous sommes concurrents".to_owned(),
            evidence_source: None,
        };
        let text = render_report(agent, Some(&report_result(agent)), &[exclusion]);
        assert!(text.starts_with(&format!("qualification agent: {}", agent.name)));
        assert!(text.contains("summary: Résumé."));
        assert!(text.contains("agent score: 95 %"));
        assert!(text.contains("confidence 90 %, agreement between samples 67 %"));
        assert!(text.contains("quote: \"expose Keycloak 18\""));
        assert!(text.contains("exclusion raised:"));
        assert!(text.contains("quote: \"Nous sommes concurrents\""));
        assert_eq!(
            report_source(agent),
            format!("qualification:{}", agent.id.as_str())
        );
    }

    #[test]
    fn an_ungrounded_answer_is_flagged_and_an_invalid_report_is_missing() {
        let definition = definition("fr");
        let agent = &definition.qualification_agents[0];
        let mut result = report_result(agent);
        result.answers = json!({agent.questions[0].id.clone(): {
            "type": "choice", "choice": "inconnu", "evidence": "invented", "grounded": false,
            "original_choice": "forte", "confidence": 0.9,
            "probabilities": {"forte": 0.0, "moyenne": 0.0, "faible": 0.0, "inconnu": 1.0}
        }});
        let text = render_report(agent, Some(&result), &[]);
        assert!(text.contains("ungrounded: the agent first chose `forte`"));
        assert!(!text.contains("invented"));
        let missing = render_report(agent, None, &[]);
        assert!(missing.contains("status: invalid, missing from this report (no result)"));
    }

    #[test]
    fn the_indicators_state_the_opinion_the_evidence_and_the_confidence() {
        let text = render_indicators(Some(0.72), &evidence(), Some(39.6));
        assert!(text.contains("not a decision"));
        assert!(text.contains("agents opinion: 72 %"));
        assert!(text.contains(
            "evidence strength: 55 % (volume 40 %, diversity 50 %, environment 100 %, signal strength 30 %)"
        ));
        assert!(text.contains("independent signals: 2, decayed weight 1.5"));
        assert!(text.contains("only non-production hosts: yes"));
        assert!(text.contains("indicator confidence (opinion x evidence): 39.6 %"));
        let pending = render_indicators(None, &evidence(), None);
        assert!(pending.contains("agents opinion: not computed"));
        assert!(pending.contains("indicator confidence (opinion x evidence): not computed"));
    }
}
