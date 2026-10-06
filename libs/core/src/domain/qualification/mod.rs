use std::fmt;

use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::domain::{
    lead::{LeadId, str_enum, uuid_id},
    llm::provider::ProviderId,
    profile::{
        ProfileId, RevisionNumber,
        definition::{AgentId, QualificationAgent, Question, VerdictThresholds},
    },
};

pub mod ports;
pub mod prompt;

uuid_id!(ResultId);

const PROBABILITY_TOLERANCE: f64 = 0.05;
const EPSILON: f64 = 1e-9;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum Violation {
    #[error("la réponse n'est pas un objet JSON")]
    NotAnObject,
    #[error("la clé `answers` est absente ou n'est pas un objet")]
    MissingAnswers,
    #[error("question `{question}` : pas de réponse")]
    MissingQuestion { question: String },
    #[error("question `{question}` : la réponse n'est pas un objet")]
    MalformedAnswer { question: String },
    #[error("question `{question}` : `type` doit valoir \"choice\"")]
    WrongType { question: String },
    #[error("question `{question}` : `choice` doit être une des options, reçu {choice}")]
    UnknownChoice { question: String, choice: String },
    #[error("question `{question}` : `confidence` doit être un nombre entre 0 et 1")]
    BadConfidence { question: String },
    #[error("question `{question}` : `probabilities` doit contenir exactement les options")]
    ProbabilityKeys { question: String },
    #[error("question `{question}` : la probabilité de `{option}` doit être entre 0 et 1")]
    BadProbability { question: String, option: String },
    #[error("question `{question}` : les probabilités totalisent {sum}, attendu 1 à 0,05 près")]
    ProbabilitySum { question: String, sum: f64 },
}

impl Violation {
    pub fn question(&self) -> Option<&str> {
        match self {
            Self::NotAnObject | Self::MissingAnswers => None,
            Self::MissingQuestion { question }
            | Self::MalformedAnswer { question }
            | Self::WrongType { question }
            | Self::UnknownChoice { question, .. }
            | Self::BadConfidence { question }
            | Self::ProbabilityKeys { question }
            | Self::BadProbability { question, .. }
            | Self::ProbabilitySum { question, .. } => Some(question),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Violations(pub Vec<Violation>);

impl fmt::Display for Violations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, violation) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(" ; ")?;
            }
            write!(f, "{violation}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub question_id: String,
    pub choice: String,
    pub confidence: f64,
    pub probabilities: Vec<(String, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidAnswers(pub Vec<Answer>);

impl ValidAnswers {
    pub fn to_json(&self) -> Value {
        let mut answers = Map::new();
        for answer in &self.0 {
            let probabilities: Map<String, Value> = answer
                .probabilities
                .iter()
                .map(|(option, probability)| (option.clone(), json!(probability)))
                .collect();
            answers.insert(
                answer.question_id.clone(),
                json!({
                    "type": "choice",
                    "choice": answer.choice,
                    "confidence": answer.confidence,
                    "probabilities": probabilities,
                }),
            );
        }
        Value::Object(answers)
    }
}

fn unit(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|number| number.is_finite() && (0.0..=1.0).contains(number))
}

fn check_question(question: &Question, raw: &Value) -> Result<Answer, Vec<Violation>> {
    let id = question.id.clone();
    let Some(object) = raw.as_object() else {
        return Err(vec![Violation::MalformedAnswer { question: id }]);
    };
    let mut violations = Vec::new();
    if object.get("type").and_then(Value::as_str) != Some("choice") {
        violations.push(Violation::WrongType {
            question: id.clone(),
        });
    }
    let choice = object.get("choice").and_then(Value::as_str);
    if !choice.is_some_and(|choice| question.options.iter().any(|option| option.value == choice)) {
        violations.push(Violation::UnknownChoice {
            question: id.clone(),
            choice: object
                .get("choice")
                .map_or_else(|| "null".to_owned(), Value::to_string),
        });
    }
    let confidence = object.get("confidence").and_then(unit);
    if confidence.is_none() {
        violations.push(Violation::BadConfidence {
            question: id.clone(),
        });
    }
    let probabilities = check_probabilities(question, object.get("probabilities"), &mut violations);
    match (choice, confidence, probabilities) {
        (Some(choice), Some(confidence), Some(probabilities)) if violations.is_empty() => {
            Ok(Answer {
                question_id: id,
                choice: choice.to_owned(),
                confidence,
                probabilities,
            })
        }
        _ => Err(violations),
    }
}

fn check_probabilities(
    question: &Question,
    raw: Option<&Value>,
    violations: &mut Vec<Violation>,
) -> Option<Vec<(String, f64)>> {
    let id = &question.id;
    let Some(object) = raw.and_then(Value::as_object) else {
        violations.push(Violation::ProbabilityKeys {
            question: id.clone(),
        });
        return None;
    };
    let keys_match = object.len() == question.options.len()
        && question
            .options
            .iter()
            .all(|option| object.contains_key(&option.value));
    if !keys_match {
        violations.push(Violation::ProbabilityKeys {
            question: id.clone(),
        });
        return None;
    }
    let mut parsed = Vec::with_capacity(question.options.len());
    let mut bounded = true;
    for option in &question.options {
        match object.get(&option.value).and_then(unit) {
            Some(probability) => parsed.push((option.value.clone(), probability)),
            None => {
                bounded = false;
                violations.push(Violation::BadProbability {
                    question: id.clone(),
                    option: option.value.clone(),
                });
            }
        }
    }
    if !bounded {
        return None;
    }
    let sum: f64 = parsed.iter().map(|(_, probability)| probability).sum();
    if sum <= EPSILON || (sum - 1.0).abs() > PROBABILITY_TOLERANCE + EPSILON {
        violations.push(Violation::ProbabilitySum {
            question: id.clone(),
            sum,
        });
        return None;
    }
    Some(
        parsed
            .into_iter()
            .map(|(option, probability)| (option, probability / sum))
            .collect(),
    )
}

pub fn validate_answers(
    agent: &QualificationAgent,
    output: &Value,
) -> Result<ValidAnswers, Violations> {
    let Some(root) = output.as_object() else {
        return Err(Violations(vec![Violation::NotAnObject]));
    };
    let Some(answers) = root.get("answers").and_then(Value::as_object) else {
        return Err(Violations(vec![Violation::MissingAnswers]));
    };
    let mut valid = Vec::with_capacity(agent.questions.len());
    let mut violations = Vec::new();
    for question in &agent.questions {
        match answers.get(&question.id) {
            None => violations.push(Violation::MissingQuestion {
                question: question.id.clone(),
            }),
            Some(raw) => match check_question(question, raw) {
                Ok(answer) => valid.push(answer),
                Err(found) => violations.extend(found),
            },
        }
    }
    if violations.is_empty() {
        Ok(ValidAnswers(valid))
    } else {
        Err(Violations(violations))
    }
}

pub fn question_score(question: &Question, answer: &Answer) -> f64 {
    answer
        .probabilities
        .iter()
        .map(|(value, probability)| {
            let score = question
                .options
                .iter()
                .find(|option| &option.value == value)
                .map_or(0.0, |option| option.score);
            probability * score
        })
        .sum()
}

pub fn agent_score(agent: &QualificationAgent, answers: &ValidAnswers) -> f64 {
    let mut weighted = 0.0;
    let mut total = 0.0;
    for question in &agent.questions {
        let Some(answer) = answers
            .0
            .iter()
            .find(|answer| answer.question_id == question.id)
        else {
            continue;
        };
        let weight = f64::from(question.weight);
        weighted += weight * question_score(question, answer);
        total += weight;
    }
    if total <= 0.0 { 0.0 } else { weighted / total }
}

pub fn lead_confidence(results: &[(u8, f64)]) -> Option<f64> {
    let total: f64 = results.iter().map(|(weight, _)| f64::from(*weight)).sum();
    if total <= 0.0 {
        return None;
    }
    let weighted: f64 = results
        .iter()
        .map(|(weight, score)| f64::from(*weight) * score)
        .sum();
    Some((weighted / total * 1000.0).round() / 10.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Verdict {
    Reliable,
    ToCheck,
    Discarded,
}

str_enum!(Verdict {
    Reliable => "fiable",
    ToCheck => "a_verifier",
    Discarded => "ecarte",
});

pub fn verdict_for(confidence: f64, thresholds: VerdictThresholds) -> Verdict {
    if confidence >= f64::from(thresholds.reliable_from) {
        Verdict::Reliable
    } else if confidence < f64::from(thresholds.discard_below) {
        Verdict::Discarded
    } else {
        Verdict::ToCheck
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Judgement {
    pub confidence: Option<f64>,
    pub verdict: Option<Verdict>,
}

pub fn judge(results: &[(u8, f64)], thresholds: VerdictThresholds) -> Judgement {
    let confidence = lead_confidence(results);
    Judgement {
        confidence,
        verdict: confidence.map(|value| verdict_for(value, thresholds)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultStatus {
    Valid,
    Invalid,
}

str_enum!(ResultStatus {
    Valid => "valid",
    Invalid => "invalid",
});

#[derive(Debug, Clone, PartialEq)]
pub struct QualificationResult {
    pub id: ResultId,
    pub lead_id: LeadId,
    pub profile_id: ProfileId,
    pub profile_revision: RevisionNumber,
    pub agent_id: AgentId,
    pub provider_id: Option<ProviderId>,
    pub model: String,
    pub prompt_hash: String,
    pub status: ResultStatus,
    pub answers: Value,
    pub agent_score: Option<f64>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub const SUMMARY_FAILED_AFTER: u32 = 5;
pub const SUMMARY_ERROR_MAX_CHARS: usize = 200;

pub fn summary_retry_delay(attempts: u32) -> chrono::Duration {
    match attempts {
        0 => chrono::Duration::zero(),
        1 => chrono::Duration::minutes(1),
        2 => chrono::Duration::minutes(5),
        3 => chrono::Duration::minutes(30),
        4 => chrono::Duration::hours(2),
        _ => chrono::Duration::hours(6),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryState {
    pub attempts: u32,
    pub error: Option<String>,
    pub next_attempt_at: Option<DateTime<Utc>>,
}

impl SummaryState {
    pub fn is_failed(&self) -> bool {
        self.attempts >= SUMMARY_FAILED_AFTER
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    pub lead_id: LeadId,
    pub profile_revision: RevisionNumber,
    pub sources: Vec<String>,
    pub judgement: Judgement,
    pub at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile::definition::QuestionOption;

    fn option(value: &str, score: f64) -> QuestionOption {
        QuestionOption {
            value: value.to_owned(),
            label: value.to_uppercase(),
            score,
        }
    }

    fn question(id: &str, weight: u8) -> Question {
        Question {
            id: id.to_owned(),
            wording: "?".to_owned(),
            weight,
            options: vec![
                option("forte", 1.0),
                option("moyenne", 0.5),
                option("faible", 0.0),
            ],
        }
    }

    fn agent(questions: Vec<Question>) -> QualificationAgent {
        QualificationAgent {
            id: AgentId::new("pertinence"),
            name: "Pertinence".to_owned(),
            enabled: true,
            provider_id: None,
            system_prompt: "prompt".to_owned(),
            weight: 5,
            questions,
        }
    }

    fn answer(choice: &str, confidence: f64, forte: f64, moyenne: f64, faible: f64) -> Value {
        json!({
            "type": "choice",
            "choice": choice,
            "confidence": confidence,
            "probabilities": {"forte": forte, "moyenne": moyenne, "faible": faible}
        })
    }

    fn output(answers: Value) -> Value {
        json!({"answers": answers})
    }

    fn violations(agent: &QualificationAgent, value: Value) -> Vec<Violation> {
        validate_answers(agent, &value).unwrap_err().0
    }

    #[test]
    fn the_summary_retry_delay_follows_the_schedule() {
        let minutes = |attempts| summary_retry_delay(attempts).num_minutes();
        assert_eq!(minutes(0), 0);
        assert_eq!(
            [1, 2, 3, 4, 5, 6, 50].map(minutes),
            [1, 5, 30, 120, 360, 360, 360]
        );
        let state = |attempts| SummaryState {
            attempts,
            error: None,
            next_attempt_at: None,
        };
        assert!(!state(SUMMARY_FAILED_AFTER - 1).is_failed());
        assert!(state(SUMMARY_FAILED_AFTER).is_failed());
    }

    #[test]
    fn a_conforming_answer_is_accepted() {
        let agent = agent(vec![question("q", 5)]);
        let valid = validate_answers(
            &agent,
            &output(json!({"q": answer("forte", 0.9, 0.8, 0.15, 0.05)})),
        )
        .unwrap();
        assert_eq!(valid.0[0].choice, "forte");
        assert_eq!(valid.0[0].confidence, 0.9);
        assert_eq!(valid.to_json()["q"]["type"], "choice");
        let stored = valid.to_json()["q"]["probabilities"]["forte"]
            .as_f64()
            .unwrap();
        assert!((stored - 0.8).abs() < 1e-9);
    }

    #[test]
    fn a_missing_question_is_listed_with_its_id() {
        let agent = agent(vec![question("a", 5), question("b", 5)]);
        let found = violations(
            &agent,
            output(json!({"a": answer("forte", 0.9, 1.0, 0.0, 0.0)})),
        );
        assert_eq!(
            found,
            vec![Violation::MissingQuestion {
                question: "b".to_owned()
            }]
        );
        assert_eq!(found[0].question(), Some("b"));
        assert!(Violations(found).to_string().contains("`b`"));
    }

    #[test]
    fn the_output_must_be_an_object_with_answers() {
        let agent = agent(vec![question("q", 5)]);
        assert_eq!(violations(&agent, json!([])), vec![Violation::NotAnObject]);
        assert_eq!(
            violations(&agent, json!({"answers": 3})),
            vec![Violation::MissingAnswers]
        );
        assert_eq!(
            violations(&agent, output(json!({"q": "forte"}))),
            vec![Violation::MalformedAnswer {
                question: "q".to_owned()
            }]
        );
    }

    #[test]
    fn the_type_must_be_choice() {
        let agent = agent(vec![question("q", 5)]);
        let mut wrong = answer("forte", 0.9, 1.0, 0.0, 0.0);
        wrong["type"] = json!("text");
        assert_eq!(
            violations(&agent, output(json!({"q": wrong}))),
            vec![Violation::WrongType {
                question: "q".to_owned()
            }]
        );
    }

    #[test]
    fn an_unknown_choice_is_rejected() {
        let agent = agent(vec![question("q", 5)]);
        let found = violations(
            &agent,
            output(json!({"q": answer("enorme", 0.9, 1.0, 0.0, 0.0)})),
        );
        assert!(matches!(
            found.as_slice(),
            [Violation::UnknownChoice { question, .. }] if question == "q"
        ));
    }

    #[test]
    fn confidence_must_be_between_zero_and_one() {
        let agent = agent(vec![question("q", 5)]);
        for bad in [json!(1.2), json!(-0.1), json!("high"), Value::Null] {
            let mut value = answer("forte", 0.9, 1.0, 0.0, 0.0);
            value["confidence"] = bad;
            assert_eq!(
                violations(&agent, output(json!({"q": value}))),
                vec![Violation::BadConfidence {
                    question: "q".to_owned()
                }]
            );
        }
    }

    #[test]
    fn probability_keys_must_be_the_options() {
        let agent = agent(vec![question("q", 5)]);
        let mut missing = answer("forte", 0.9, 1.0, 0.0, 0.0);
        missing["probabilities"] = json!({"forte": 0.5, "moyenne": 0.5});
        let mut extra = answer("forte", 0.9, 1.0, 0.0, 0.0);
        extra["probabilities"] =
            json!({"forte": 0.5, "moyenne": 0.25, "faible": 0.25, "autre": 0.0});
        let mut renamed = answer("forte", 0.9, 1.0, 0.0, 0.0);
        renamed["probabilities"] = json!({"forte": 0.5, "moyenne": 0.25, "faibl": 0.25});
        for value in [missing, extra, renamed] {
            assert_eq!(
                violations(&agent, output(json!({"q": value}))),
                vec![Violation::ProbabilityKeys {
                    question: "q".to_owned()
                }]
            );
        }
    }

    #[test]
    fn each_probability_must_be_between_zero_and_one() {
        let agent = agent(vec![question("q", 5)]);
        let found = violations(
            &agent,
            output(json!({"q": answer("forte", 0.9, 1.5, -0.5, 0.0)})),
        );
        assert_eq!(
            found,
            vec![
                Violation::BadProbability {
                    question: "q".to_owned(),
                    option: "forte".to_owned()
                },
                Violation::BadProbability {
                    question: "q".to_owned(),
                    option: "moyenne".to_owned()
                },
            ]
        );
    }

    #[test]
    fn probabilities_far_from_one_are_rejected() {
        let agent = agent(vec![question("q", 5)]);
        for (forte, moyenne, faible) in [(0.5, 0.2, 0.1), (0.9, 0.5, 0.3)] {
            let found = violations(
                &agent,
                output(json!({"q": answer("forte", 0.9, forte, moyenne, faible)})),
            );
            assert!(matches!(
                found.as_slice(),
                [Violation::ProbabilitySum { question, .. }] if question == "q"
            ));
        }
    }

    #[test]
    fn probabilities_summing_to_zero_are_invalid() {
        let agent = agent(vec![question("q", 5)]);
        let found = violations(
            &agent,
            output(json!({"q": answer("forte", 0.9, 0.0, 0.0, 0.0)})),
        );
        assert!(matches!(
            found.as_slice(),
            [Violation::ProbabilitySum { sum, .. }] if *sum == 0.0
        ));
    }

    #[test]
    fn probabilities_close_to_one_are_renormalised() {
        let agent = agent(vec![question("q", 5)]);
        let valid = validate_answers(
            &agent,
            &output(json!({"q": answer("forte", 0.9, 0.6, 0.3, 0.14)})),
        )
        .unwrap();
        let total: f64 = valid.0[0].probabilities.iter().map(|(_, p)| p).sum();
        assert!((total - 1.0).abs() < 1e-12);
        assert!((valid.0[0].probabilities[0].1 - 0.6 / 1.04).abs() < 1e-12);
        let low = validate_answers(
            &agent,
            &output(json!({"q": answer("forte", 0.9, 0.5, 0.3, 0.17)})),
        )
        .unwrap();
        let total: f64 = low.0[0].probabilities.iter().map(|(_, p)| p).sum();
        assert!((total - 1.0).abs() < 1e-12);
    }

    #[test]
    fn violations_of_several_questions_are_all_reported() {
        let agent = agent(vec![question("a", 5), question("b", 5), question("c", 5)]);
        let found = violations(
            &agent,
            output(json!({
                "a": answer("enorme", 0.9, 1.0, 0.0, 0.0),
                "c": answer("forte", 0.9, 1.0, 0.0, 0.0)
            })),
        );
        let questions: Vec<_> = found.iter().filter_map(Violation::question).collect();
        assert_eq!(questions, ["a", "b"]);
    }

    fn valid_answers(agent: &QualificationAgent, raw: Value) -> ValidAnswers {
        validate_answers(agent, &output(raw)).unwrap()
    }

    #[test]
    fn a_question_score_is_the_probability_weighted_option_score() {
        let question = question("q", 5);
        let agent = agent(vec![question.clone()]);
        let answers = valid_answers(&agent, json!({"q": answer("forte", 0.9, 0.5, 0.3, 0.2)}));
        assert!((question_score(&question, &answers.0[0]) - 0.65).abs() < 1e-12);
    }

    #[test]
    fn a_question_score_uses_the_renormalised_probabilities() {
        let question = question("q", 5);
        let agent = agent(vec![question.clone()]);
        let answers = valid_answers(&agent, json!({"q": answer("forte", 0.9, 1.0, 0.0, 0.04)}));
        assert!((question_score(&question, &answers.0[0]) - 1.0 / 1.04).abs() < 1e-12);
    }

    #[test]
    fn an_agent_score_weights_its_questions() {
        let agent = agent(vec![question("a", 3), question("b", 1)]);
        let answers = valid_answers(
            &agent,
            json!({
                "a": answer("forte", 0.9, 1.0, 0.0, 0.0),
                "b": answer("faible", 0.9, 0.0, 0.0, 1.0)
            }),
        );
        assert!((agent_score(&agent, &answers) - 0.75).abs() < 1e-12);
    }

    #[test]
    fn the_lead_confidence_weights_agents_and_rounds_to_one_decimal() {
        assert_eq!(lead_confidence(&[(5, 0.8)]), Some(80.0));
        assert_eq!(lead_confidence(&[(1, 1.0), (3, 0.0)]), Some(25.0));
        assert_eq!(lead_confidence(&[(1, 1.0), (2, 0.0)]), Some(33.3));
        assert_eq!(lead_confidence(&[(1, 0.0), (2, 1.0)]), Some(66.7));
        assert_eq!(lead_confidence(&[(10, 0.62345)]), Some(62.3));
    }

    #[test]
    fn no_valid_result_means_no_confidence_and_no_verdict() {
        assert_eq!(lead_confidence(&[]), None);
        let judgement = judge(&[], VerdictThresholds::default());
        assert_eq!(judgement.confidence, None);
        assert_eq!(judgement.verdict, None);
    }

    #[test]
    fn thresholds_decide_the_verdict() {
        let thresholds = VerdictThresholds {
            reliable_from: 70,
            discard_below: 30,
        };
        assert_eq!(verdict_for(70.0, thresholds), Verdict::Reliable);
        assert_eq!(verdict_for(100.0, thresholds), Verdict::Reliable);
        assert_eq!(verdict_for(69.9, thresholds), Verdict::ToCheck);
        assert_eq!(verdict_for(30.0, thresholds), Verdict::ToCheck);
        assert_eq!(verdict_for(29.9, thresholds), Verdict::Discarded);
        assert_eq!(verdict_for(0.0, thresholds), Verdict::Discarded);
        let custom = VerdictThresholds {
            reliable_from: 90,
            discard_below: 50,
        };
        assert_eq!(verdict_for(80.0, custom), Verdict::ToCheck);
        assert_eq!(verdict_for(49.0, custom), Verdict::Discarded);
        let judged = judge(&[(5, 0.9)], thresholds);
        assert_eq!(judged.confidence, Some(90.0));
        assert_eq!(judged.verdict, Some(Verdict::Reliable));
    }

    #[test]
    fn verdicts_and_statuses_round_trip_through_text() {
        for verdict in [Verdict::Reliable, Verdict::ToCheck, Verdict::Discarded] {
            assert_eq!(Verdict::parse(verdict.as_str()), Some(verdict));
        }
        assert_eq!(Verdict::Reliable.as_str(), "fiable");
        assert_eq!(Verdict::ToCheck.as_str(), "a_verifier");
        assert_eq!(Verdict::Discarded.as_str(), "ecarte");
        for status in [ResultStatus::Valid, ResultStatus::Invalid] {
            assert_eq!(ResultStatus::parse(status.as_str()), Some(status));
        }
    }
}
