use serde::{Deserialize, Serialize};

use super::{QualificationResult, ResultStatus, stored_answers};
use crate::domain::profile::definition::{AgentId, QualificationAgent};

const EPSILON: f64 = 1e-9;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exclusion {
    pub agent_id: AgentId,
    pub agent_name: String,
    pub question_id: String,
    pub option: String,
    pub label: String,
    pub probability: f64,
    pub evidence: String,
    #[serde(default)]
    pub evidence_source: Option<String>,
}

pub fn exclusions(
    results: &[QualificationResult],
    agents: &[QualificationAgent],
) -> Vec<Exclusion> {
    let mut found = Vec::new();
    for agent in agents.iter().filter(|agent| agent.enabled) {
        let Some(result) = results
            .iter()
            .find(|result| result.agent_id == agent.id && result.status == ResultStatus::Valid)
        else {
            continue;
        };
        let Some(answers) = stored_answers(agent, &result.answers) else {
            continue;
        };
        for question in &agent.questions {
            let Some(answer) = answers
                .0
                .iter()
                .find(|answer| answer.question_id == question.id)
            else {
                continue;
            };
            if !answer.grounded || answer.evidence.is_empty() {
                continue;
            }
            let strongest = question
                .options
                .iter()
                .filter(|option| option.excludes && !option.is_unknown)
                .filter_map(|option| {
                    answer
                        .probabilities
                        .iter()
                        .find(|(value, _)| value == &option.value)
                        .map(|(_, probability)| (option, *probability))
                })
                .filter(|(_, probability)| probability + EPSILON >= agent.exclusion_threshold)
                .max_by(|left, right| left.1.total_cmp(&right.1));
            if let Some((option, probability)) = strongest {
                found.push(Exclusion {
                    agent_id: agent.id.clone(),
                    agent_name: agent.name.clone(),
                    question_id: question.id.clone(),
                    option: option.value.clone(),
                    label: option.label.clone(),
                    probability,
                    evidence: answer.evidence.clone(),
                    evidence_source: answer.evidence_source.clone(),
                });
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::{Value, json};

    use super::*;
    use crate::domain::{
        lead::LeadId,
        llm::RunProvenance,
        profile::{
            ProfileId, RevisionNumber,
            definition::{Question, QuestionOption},
        },
        qualification::{ResultId, confidence::judge},
    };

    fn option(value: &str, excludes: bool, unknown: bool) -> QuestionOption {
        QuestionOption {
            value: value.to_owned(),
            label: value.to_uppercase(),
            score: if excludes { 0.0 } else { 1.0 },
            is_unknown: unknown,
            excludes,
        }
    }

    fn question(id: &str) -> Question {
        Question {
            id: id.to_owned(),
            wording: "?".to_owned(),
            weight: 5,
            options: vec![
                option("oui", true, false),
                option("concurrent", true, false),
                option("non", false, false),
                option("inconnu", false, true),
            ],
        }
    }

    fn agent(id: &str, threshold: f64, questions: Vec<Question>) -> QualificationAgent {
        QualificationAgent {
            id: AgentId::new(id),
            name: format!("Agent {id}"),
            description: "d".to_owned(),
            enabled: true,
            provider_id: None,
            system_prompt: "p".to_owned(),
            weight: 1,
            exclusion_threshold: threshold,
            samples: 1,
            sampling_temperature: 0.7,
            learned_calibration: true,
            questions,
        }
    }

    fn answer(oui: f64, concurrent: f64, non: f64, inconnu: f64) -> Value {
        json!({
            "type": "choice",
            "choice": "oui",
            "evidence": "Nous sommes un concurrent direct.",
            "grounded": true,
            "confidence": 0.9,
            "probabilities": {"oui": oui, "concurrent": concurrent, "non": non, "inconnu": inconnu}
        })
    }

    fn result(agent: &str, status: ResultStatus, answers: Value) -> QualificationResult {
        QualificationResult {
            id: ResultId(uuid::Uuid::nil()),
            lead_id: LeadId(uuid::Uuid::nil()),
            profile_id: ProfileId(uuid::Uuid::nil()),
            profile_revision: RevisionNumber::FIRST,
            agent_id: AgentId::new(agent),
            provenance: RunProvenance::legacy("m".to_owned(), "v".to_owned()),
            status,
            summary: Some("s".to_owned()),
            answers,
            agent_score: Some(0.5),
            error: None,
            escalated: false,
            raw_samples: json!([]),
            created_at: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
        }
    }

    fn valid(agent: &str, answers: Value) -> QualificationResult {
        result(agent, ResultStatus::Valid, answers)
    }

    #[test]
    fn a_grounded_answer_at_the_threshold_excludes() {
        let agents = [agent("excl", 0.6, vec![question("q")])];
        let results = [valid("excl", json!({"q": answer(0.6, 0.0, 0.3, 0.1)}))];
        let found = exclusions(&results, &agents);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].agent_id, AgentId::new("excl"));
        assert_eq!(found[0].agent_name, "Agent excl");
        assert_eq!(found[0].question_id, "q");
        assert_eq!(found[0].option, "oui");
        assert_eq!(found[0].label, "OUI");
        assert!((found[0].probability - 0.6).abs() < 1e-9);
        assert_eq!(found[0].evidence, "Nous sommes un concurrent direct.");
    }

    #[test]
    fn a_probability_under_the_threshold_does_not_exclude() {
        let agents = [agent("excl", 0.6, vec![question("q")])];
        let results = [valid("excl", json!({"q": answer(0.55, 0.0, 0.4, 0.05)}))];
        assert!(exclusions(&results, &agents).is_empty());
    }

    #[test]
    fn the_threshold_is_the_one_of_the_agent() {
        let results = [valid("excl", json!({"q": answer(0.8, 0.0, 0.1, 0.1)}))];
        assert!(exclusions(&results, &[agent("excl", 0.9, vec![question("q")])]).is_empty());
        assert_eq!(
            exclusions(&results, &[agent("excl", 0.8, vec![question("q")])]).len(),
            1
        );
    }

    #[test]
    fn an_ungrounded_or_downgraded_answer_is_ignored() {
        let agents = [agent("excl", 0.6, vec![question("q")])];
        let mut ungrounded = answer(0.9, 0.0, 0.05, 0.05);
        ungrounded["grounded"] = json!(false);
        assert!(exclusions(&[valid("excl", json!({"q": ungrounded}))], &agents).is_empty());
        let downgraded = json!({
            "type": "choice",
            "choice": "inconnu",
            "evidence": "Nous sommes un concurrent direct.",
            "grounded": false,
            "original_choice": "oui",
            "confidence": 0.9,
            "probabilities": {"oui": 0.0, "concurrent": 0.0, "non": 0.0, "inconnu": 1.0}
        });
        assert!(exclusions(&[valid("excl", json!({"q": downgraded}))], &agents).is_empty());
    }

    #[test]
    fn an_answer_without_a_quote_is_ignored() {
        let agents = [agent("excl", 0.6, vec![question("q")])];
        let mut bare = answer(0.9, 0.0, 0.05, 0.05);
        bare["evidence"] = json!("");
        assert!(exclusions(&[valid("excl", json!({"q": bare}))], &agents).is_empty());
    }

    #[test]
    fn an_option_that_does_not_exclude_never_excludes() {
        let agents = [agent("excl", 0.6, vec![question("q")])];
        let results = [valid("excl", json!({"q": answer(0.0, 0.0, 0.95, 0.05)}))];
        assert!(exclusions(&results, &agents).is_empty());
    }

    #[test]
    fn invalid_results_and_disabled_agents_are_ignored() {
        let mut disabled = agent("off", 0.6, vec![question("q")]);
        disabled.enabled = false;
        let agents = [disabled, agent("excl", 0.6, vec![question("q")])];
        let results = [
            valid("off", json!({"q": answer(0.9, 0.0, 0.05, 0.05)})),
            result(
                "excl",
                ResultStatus::Invalid,
                json!({"q": answer(0.9, 0.0, 0.05, 0.05)}),
            ),
        ];
        assert!(exclusions(&results, &agents).is_empty());
    }

    #[test]
    fn several_axes_and_agents_each_produce_an_exclusion() {
        let agents = [
            agent("one", 0.6, vec![question("a"), question("b")]),
            agent("two", 0.5, vec![question("c")]),
        ];
        let results = [
            valid(
                "one",
                json!({"a": answer(0.7, 0.0, 0.2, 0.1), "b": answer(0.0, 0.8, 0.1, 0.1)}),
            ),
            valid("two", json!({"c": answer(0.5, 0.0, 0.4, 0.1)})),
        ];
        let found = exclusions(&results, &agents);
        let keys: Vec<(&str, &str, &str)> = found
            .iter()
            .map(|e| {
                (
                    e.agent_id.as_str(),
                    e.question_id.as_str(),
                    e.option.as_str(),
                )
            })
            .collect();
        assert_eq!(
            keys,
            [
                ("one", "a", "oui"),
                ("one", "b", "concurrent"),
                ("two", "c", "oui")
            ]
        );
    }

    #[test]
    fn the_strongest_excluding_option_of_an_axis_is_kept() {
        let agents = [agent("excl", 0.5, vec![question("q")])];
        let results = [valid("excl", json!({"q": answer(0.5, 0.5, 0.0, 0.0)}))];
        let found = exclusions(&results, &agents);
        assert_eq!(found.len(), 1);
        let results = [valid("excl", json!({"q": answer(0.45, 0.55, 0.0, 0.0)}))];
        assert_eq!(exclusions(&results, &agents)[0].option, "concurrent");
    }

    #[test]
    fn an_exclusion_overrides_a_reliable_confidence() {
        use crate::domain::{
            profile::definition::{EvidenceSettings, VerdictThresholds},
            qualification::{AgentOpinion, EvidenceComponents, EvidenceStrength, Verdict},
        };
        let evidence = EvidenceStrength {
            score: 0.9,
            components: EvidenceComponents {
                volume: 0.9,
                diversity: 1.0,
                environment: 1.0,
                strength: 0.8,
            },
            signal_count: 6,
            signal_score: 40.0,
            only_non_production: false,
            independent_signals: 6,
            decayed_signals: 6.0,
            source_count: 1,
            kind_count: 1,
        };
        let opinions = [
            AgentOpinion {
                weight: 5,
                score: 0.95,
                axes: 2,
            },
            AgentOpinion {
                weight: 5,
                score: 0.95,
                axes: 2,
            },
        ];
        let judged = judge(
            &opinions,
            evidence,
            VerdictThresholds::default(),
            &EvidenceSettings::default(),
        );
        assert_eq!(judged.verdict, Some(Verdict::Reliable));
        let agents = [agent("excl", 0.6, vec![question("q")])];
        let results = [valid("excl", json!({"q": answer(0.7, 0.0, 0.2, 0.1)}))];
        let found = exclusions(&results, &agents);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].option, "oui");
        assert_eq!(judged.verdict, Some(Verdict::Reliable));
        assert!(judged.confidence.unwrap() >= 70.0);
    }
}
