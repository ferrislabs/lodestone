use std::fmt;

use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::domain::{
    lead::{LeadId, str_enum, uuid_id},
    llm::{RunProvenance, UntrustedDocument, provider::ProviderId},
    profile::{
        ProfileId, RevisionNumber,
        definition::{AgentId, EVIDENCE_MAX_CHARS, QualificationAgent, Question},
    },
    prompt::normalise::contains_normalised,
};

pub mod confidence;
pub mod exclusion;
pub mod ports;
pub mod prompt;
pub mod voting;

pub use confidence::{
    AWAITING_SCORING_REASON, AgentOpinion, EvidenceComponents, EvidenceStrength, Judgement,
    LeadScoring, SCORING_VERSION, VerdictCap, VerdictReason, VerdictSource, calibrate, judge,
    verdict_for,
};
pub use exclusion::{Exclusion, exclusions};
pub use voting::{VOTE_SHARE_WEIGHT, most_agreeing, sample_record, vote, vote_answers};

uuid_id!(ResultId);

pub const SUMMARY_MAX_CHARS: usize = 600;
const PROBABILITY_TOLERANCE: f64 = 0.05;
const EPSILON: f64 = 1e-9;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum Violation {
    #[error("la réponse n'est pas un objet JSON")]
    NotAnObject,
    #[error("la clé `answers` est absente ou n'est pas un objet")]
    MissingAnswers,
    #[error("la clé `summary` est absente ou vide")]
    MissingSummary,
    #[error("`summary` fait {length} caractères, le maximum est {SUMMARY_MAX_CHARS}")]
    SummaryTooLong { length: usize },
    #[error("question `{question}` : pas de réponse")]
    MissingQuestion { question: String },
    #[error("question `{question}` : la réponse n'est pas un objet")]
    MalformedAnswer { question: String },
    #[error("question `{question}` : `type` doit valoir \"choice\"")]
    WrongType { question: String },
    #[error("question `{question}` : `choice` doit être une des options, reçu {choice}")]
    UnknownChoice { question: String, choice: String },
    #[error("question `{question}` : `evidence` doit être une chaîne de caractères")]
    BadEvidence { question: String },
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
            Self::NotAnObject
            | Self::MissingAnswers
            | Self::MissingSummary
            | Self::SummaryTooLong { .. } => None,
            Self::MissingQuestion { question }
            | Self::MalformedAnswer { question }
            | Self::WrongType { question }
            | Self::UnknownChoice { question, .. }
            | Self::BadEvidence { question }
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
    pub evidence: String,
    pub evidence_source: Option<String>,
    pub grounded: bool,
    pub original_choice: Option<String>,
    pub agreement: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidAnswers(pub Vec<Answer>);

#[derive(Debug, Clone, PartialEq)]
pub struct ValidOutput {
    pub summary: String,
    pub answers: ValidAnswers,
}

impl ValidAnswers {
    pub fn ungrounded(&self) -> Vec<&str> {
        self.0
            .iter()
            .filter(|answer| !answer.grounded)
            .map(|answer| answer.question_id.as_str())
            .collect()
    }

    pub fn to_json(&self) -> Value {
        let mut answers = Map::new();
        for answer in &self.0 {
            let probabilities: Map<String, Value> = answer
                .probabilities
                .iter()
                .map(|(option, probability)| (option.clone(), json!(probability)))
                .collect();
            let mut entry = json!({
                "type": "choice",
                "choice": answer.choice,
                "evidence": answer.evidence,
                "grounded": answer.grounded,
                "confidence": answer.confidence,
                "probabilities": probabilities,
            });
            if let Some(source) = &answer.evidence_source {
                entry["evidence_source"] = json!(source);
            }
            if let Some(original) = &answer.original_choice {
                entry["original_choice"] = json!(original);
            }
            if let Some(agreement) = answer.agreement {
                entry["agreement"] = json!(agreement);
            }
            answers.insert(answer.question_id.clone(), entry);
        }
        Value::Object(answers)
    }
}

pub(crate) fn unit(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|number| number.is_finite() && (0.0..=1.0).contains(number))
}

pub(crate) fn check_question(question: &Question, raw: &Value) -> Result<Answer, Vec<Violation>> {
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
                evidence: object
                    .get("evidence")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                evidence_source: object
                    .get("evidence_source")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                grounded: object
                    .get("grounded")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                original_choice: object
                    .get("original_choice")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                agreement: object.get("agreement").and_then(unit),
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

fn check_summary(root: &Map<String, Value>) -> Result<String, Violation> {
    let summary = root
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|summary| !summary.is_empty())
        .ok_or(Violation::MissingSummary)?;
    let length = summary.chars().count();
    if length > SUMMARY_MAX_CHARS {
        return Err(Violation::SummaryTooLong { length });
    }
    Ok(summary.to_owned())
}

fn is_unknown_choice(question: &Question, choice: &str) -> bool {
    question
        .options
        .iter()
        .any(|option| option.is_unknown && option.value == choice)
}

pub(crate) fn ground(
    question: &Question,
    mut answer: Answer,
    documents: &[UntrustedDocument<'_>],
) -> Answer {
    answer.evidence = answer
        .evidence
        .trim()
        .chars()
        .take(EVIDENCE_MAX_CHARS)
        .collect();
    let source = documents
        .iter()
        .find(|document| contains_normalised(document.content, &answer.evidence))
        .map(|document| document.source.to_owned());
    answer.grounded = true;
    answer.original_choice = None;
    answer.agreement = None;
    answer.evidence_source = source;
    if answer.evidence_source.is_some() {
        return answer;
    }
    if is_unknown_choice(question, &answer.choice) {
        answer.evidence = String::new();
        return answer;
    }
    answer.grounded = false;
    if let Some(unknown) = question.unknown_option() {
        answer.original_choice = Some(std::mem::replace(&mut answer.choice, unknown.value.clone()));
        answer.probabilities = question
            .options
            .iter()
            .map(|option| {
                let probability = if option.is_unknown { 1.0 } else { 0.0 };
                (option.value.clone(), probability)
            })
            .collect();
    }
    answer
}

pub fn validate_answers(
    agent: &QualificationAgent,
    output: &Value,
    documents: &[UntrustedDocument<'_>],
) -> Result<ValidOutput, Violations> {
    let Some(root) = output.as_object() else {
        return Err(Violations(vec![Violation::NotAnObject]));
    };
    let Some(answers) = root.get("answers").and_then(Value::as_object) else {
        return Err(Violations(vec![Violation::MissingAnswers]));
    };
    let mut violations = Vec::new();
    let summary = match check_summary(root) {
        Ok(summary) => Some(summary),
        Err(violation) => {
            violations.push(violation);
            None
        }
    };
    let valid = validate_questions(&agent.questions, answers, documents, &mut violations);
    match summary {
        Some(summary) if violations.is_empty() => Ok(ValidOutput {
            summary,
            answers: ValidAnswers(valid),
        }),
        _ => Err(Violations(violations)),
    }
}

pub(crate) fn validate_questions(
    questions: &[Question],
    answers: &Map<String, Value>,
    documents: &[UntrustedDocument<'_>],
    violations: &mut Vec<Violation>,
) -> Vec<Answer> {
    let mut valid = Vec::with_capacity(questions.len());
    for question in questions {
        match answers.get(&question.id) {
            None => violations.push(Violation::MissingQuestion {
                question: question.id.clone(),
            }),
            Some(raw) => match check_question(question, raw) {
                Ok(answer) if raw.get("evidence").is_some_and(Value::is_string) => {
                    valid.push(ground(question, answer, documents));
                }
                Ok(_) => violations.push(Violation::BadEvidence {
                    question: question.id.clone(),
                }),
                Err(found) => violations.extend(found),
            },
        }
    }
    valid
}

pub fn question_score(question: &Question, answer: &Answer, floor: f64) -> f64 {
    calibrate(&answer.probabilities, floor)
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

pub fn agent_score(agent: &QualificationAgent, answers: &ValidAnswers, floor: f64) -> f64 {
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
        weighted += weight * question_score(question, answer, floor);
        total += weight;
    }
    if total <= 0.0 { 0.0 } else { weighted / total }
}

pub fn stored_answers(agent: &QualificationAgent, answers: &Value) -> Option<ValidAnswers> {
    stored_answers_for(&agent.questions, answers)
}

pub fn stored_answers_for(questions: &[Question], answers: &Value) -> Option<ValidAnswers> {
    let object = answers.as_object()?;
    questions
        .iter()
        .map(|question| check_question(question, object.get(&question.id)?).ok())
        .collect::<Option<Vec<_>>>()
        .map(ValidAnswers)
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
    pub provenance: RunProvenance,
    pub status: ResultStatus,
    pub summary: Option<String>,
    pub answers: Value,
    pub agent_score: Option<f64>,
    pub error: Option<String>,
    pub escalated: bool,
    pub raw_samples: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DueGates {
    pub dossier: Option<DateTime<Utc>>,
    pub enrichment: Option<DateTime<Utc>>,
    pub http_probe: bool,
    pub company_jobs: bool,
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

pub const AGENT_FAILED_AFTER: u32 = SUMMARY_FAILED_AFTER;
pub const AGENT_ERROR_MAX_CHARS: usize = 400;

pub fn agent_retry_delay(attempts: u32) -> chrono::Duration {
    summary_retry_delay(attempts)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    Qualification,
    Scoring,
}

str_enum!(AgentKind {
    Qualification => "qualification",
    Scoring => "scoring",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFailure {
    pub kind: AgentKind,
    pub profile_revision: RevisionNumber,
    pub agent_id: String,
    pub provider_id: Option<ProviderId>,
    pub attempts: u32,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub error: String,
}

impl AgentFailure {
    pub fn is_failed(&self) -> bool {
        self.attempts >= AGENT_FAILED_AFTER
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCallFailure {
    pub lead_id: LeadId,
    pub kind: AgentKind,
    pub profile_revision: RevisionNumber,
    pub agent_id: String,
    pub provider_id: Option<ProviderId>,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    pub lead_id: LeadId,
    pub profile_revision: RevisionNumber,
    pub sources: Vec<String>,
    pub judgement: Judgement,
    pub exclusions: Vec<Exclusion>,
    pub scoring_version: i32,
    pub dossier_at: Option<DateTime<Utc>>,
    pub escalated_revision: Option<RevisionNumber>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rescoring {
    pub lead_id: LeadId,
    pub judgement: Judgement,
    pub exclusions: Vec<Exclusion>,
    pub scoring_version: i32,
    pub scored_at: DateTime<Utc>,
    pub agent_scores: Vec<(ResultId, f64)>,
}

pub const SCORING_MAX_AGE_HOURS: i32 = 24;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile::definition::QuestionOption;

    const DOCUMENT: &str =
        "Le site expose Keycloak 18 en production. Équipe sécurité de Zürich : 日本語のテキスト.";

    fn docs() -> [UntrustedDocument<'static>; 1] {
        [UntrustedDocument {
            source: "https://acme.example/about",
            content: DOCUMENT,
        }]
    }

    fn option(value: &str, score: f64) -> QuestionOption {
        QuestionOption {
            value: value.to_owned(),
            label: value.to_uppercase(),
            score,
            is_unknown: value == "inconnu",
            excludes: false,
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
                option("inconnu", 0.5),
            ],
        }
    }

    fn agent(questions: Vec<Question>) -> QualificationAgent {
        QualificationAgent {
            id: AgentId::new("pertinence"),
            name: "Pertinence".to_owned(),
            description: "La pertinence".to_owned(),
            enabled: true,
            provider_id: None,
            system_prompt: "prompt".to_owned(),
            exclusion_threshold: 0.6,
            samples: 1,
            sampling_temperature: 0.7,
            learned_calibration: true,
            questions,
        }
    }

    fn answer(choice: &str, confidence: f64, forte: f64, moyenne: f64, faible: f64) -> Value {
        json!({
            "type": "choice",
            "choice": choice,
            "evidence": "expose Keycloak 18 en production",
            "confidence": confidence,
            "probabilities": {"forte": forte, "moyenne": moyenne, "faible": faible, "inconnu": 0.0}
        })
    }

    fn output(answers: Value) -> Value {
        json!({"summary": "Ce que l'agent a observé.", "answers": answers})
    }

    fn violations(agent: &QualificationAgent, value: Value) -> Vec<Violation> {
        validate_answers(agent, &value, &docs()).unwrap_err().0
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
            &docs(),
        )
        .unwrap();
        assert_eq!(valid.answers.0[0].choice, "forte");
        assert_eq!(valid.answers.0[0].confidence, 0.9);
        assert_eq!(valid.answers.to_json()["q"]["type"], "choice");
        let stored = valid.answers.to_json()["q"]["probabilities"]["forte"]
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
    fn the_summary_is_required_trimmed_and_bounded() {
        let agent = agent(vec![question("q", 5)]);
        let answers = json!({"q": answer("forte", 0.9, 0.8, 0.15, 0.05)});
        let valid = validate_answers(
            &agent,
            &json!({"summary": "  Une observation.  ", "answers": answers}),
            &docs(),
        )
        .unwrap();
        assert_eq!(valid.summary, "Une observation.");
        for bad in [json!(null), json!(""), json!("   "), json!(3)] {
            assert_eq!(
                violations(&agent, json!({"summary": bad, "answers": answers})),
                vec![Violation::MissingSummary]
            );
        }
        assert_eq!(
            violations(&agent, json!({"answers": answers})),
            vec![Violation::MissingSummary]
        );
        let longest = "é".repeat(600);
        assert!(
            validate_answers(
                &agent,
                &json!({"summary": longest, "answers": answers}),
                &docs()
            )
            .is_ok()
        );
        let too_long = "é".repeat(601);
        assert_eq!(
            violations(&agent, json!({"summary": too_long, "answers": answers})),
            vec![Violation::SummaryTooLong { length: 601 }]
        );
    }

    #[test]
    fn a_missing_summary_is_reported_with_the_answer_violations() {
        let agent = agent(vec![question("q", 5)]);
        let found = violations(&agent, json!({"answers": {}}));
        assert_eq!(
            found,
            vec![
                Violation::MissingSummary,
                Violation::MissingQuestion {
                    question: "q".to_owned()
                }
            ]
        );
        assert!(Violations(found).to_string().contains("`summary`"));
    }

    #[test]
    fn the_output_must_be_an_object_with_answers() {
        let agent = agent(vec![question("q", 5)]);
        assert_eq!(violations(&agent, json!([])), vec![Violation::NotAnObject]);
        assert_eq!(
            violations(&agent, json!({"summary": "ok", "answers": 3})),
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
            &docs(),
        )
        .unwrap();
        let total: f64 = valid.answers.0[0]
            .probabilities
            .iter()
            .map(|(_, p)| p)
            .sum();
        assert!((total - 1.0).abs() < 1e-12);
        assert!((valid.answers.0[0].probabilities[0].1 - 0.6 / 1.04).abs() < 1e-12);
        let low = validate_answers(
            &agent,
            &output(json!({"q": answer("forte", 0.9, 0.5, 0.3, 0.17)})),
            &docs(),
        )
        .unwrap();
        let total: f64 = low.answers.0[0].probabilities.iter().map(|(_, p)| p).sum();
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
        validate_answers(agent, &output(raw), &docs())
            .unwrap()
            .answers
    }

    #[test]
    fn a_question_score_is_the_probability_weighted_option_score() {
        let question = question("q", 5);
        let agent = agent(vec![question.clone()]);
        let answers = valid_answers(&agent, json!({"q": answer("forte", 0.9, 0.5, 0.3, 0.2)}));
        assert!((question_score(&question, &answers.0[0], 0.0) - 0.65).abs() < 1e-12);
    }

    #[test]
    fn a_question_score_uses_the_renormalised_probabilities() {
        let question = question("q", 5);
        let agent = agent(vec![question.clone()]);
        let answers = valid_answers(&agent, json!({"q": answer("forte", 0.9, 1.0, 0.0, 0.04)}));
        assert!((question_score(&question, &answers.0[0], 0.0) - 1.0 / 1.04).abs() < 1e-12);
    }

    #[test]
    fn a_question_score_is_calibrated_before_scoring() {
        let question = question("q", 5);
        let agent = agent(vec![question.clone()]);
        let answers = valid_answers(&agent, json!({"q": answer("forte", 1.0, 1.0, 0.0, 0.0)}));
        let calibrated = question_score(&question, &answers.0[0], 0.05);
        assert!((calibrated - 1.0 / 1.1).abs() < 1e-12);
        assert!(calibrated < 1.0);
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
        assert!((agent_score(&agent, &answers, 0.0) - 0.75).abs() < 1e-12);
    }

    #[test]
    fn stored_answers_are_read_back_for_a_rescore() {
        let agent = agent(vec![question("a", 3), question("b", 1)]);
        let answers = valid_answers(
            &agent,
            json!({
                "a": answer("forte", 0.9, 0.6, 0.3, 0.1),
                "b": answer("faible", 0.9, 0.0, 0.0, 1.0)
            }),
        );
        let read = stored_answers(&agent, &answers.to_json()).unwrap();
        assert_eq!(read.0.len(), 2);
        assert!(
            (agent_score(&agent, &read, 0.05) - agent_score(&agent, &answers, 0.05)).abs() < 1e-9
        );
        assert!(stored_answers(&agent, &json!({})).is_none());
        assert!(stored_answers(&agent, &json!(null)).is_none());
    }

    fn with_evidence(choice: &str, evidence: &str) -> Value {
        let mut value = answer(choice, 0.9, 0.0, 0.0, 0.0);
        value["probabilities"] =
            json!({"forte": 1.0, "moyenne": 0.0, "faible": 0.0, "inconnu": 0.0});
        value["evidence"] = json!(evidence);
        value
    }

    fn validated(raw: Value) -> Answer {
        let agent = agent(vec![question("q", 5)]);
        let mut valid = validate_answers(&agent, &output(json!({"q": raw})), &docs()).unwrap();
        valid.answers.0.remove(0)
    }

    #[test]
    fn a_quote_found_in_a_document_grounds_the_answer_with_its_source() {
        let answer = validated(with_evidence("forte", "expose Keycloak 18 en production"));
        assert!(answer.grounded);
        assert_eq!(answer.choice, "forte");
        assert_eq!(answer.original_choice, None);
        assert_eq!(answer.evidence, "expose Keycloak 18 en production");
        assert_eq!(
            answer.evidence_source.as_deref(),
            Some("https://acme.example/about")
        );
    }

    #[test]
    fn the_quote_is_checked_with_the_normalised_comparison() {
        let answer = validated(with_evidence(
            "forte",
            "  Expose  KEYCLOAK 18\nen production ",
        ));
        assert!(answer.grounded);
        assert_eq!(answer.evidence, "Expose  KEYCLOAK 18\nen production");
    }

    #[test]
    fn multibyte_quotes_are_verified_and_truncated_on_character_boundaries() {
        let answer = validated(with_evidence(
            "forte",
            "Équipe sécurité de Zürich : 日本語のテキスト",
        ));
        assert!(answer.grounded);
        assert_eq!(answer.choice, "forte");
        let long = format!("{}{}", "é".repeat(150), "日".repeat(200));
        let truncated = validated(with_evidence("forte", &long));
        assert!(!truncated.grounded);
        assert_eq!(truncated.evidence.chars().count(), 300);
        let prefix_document = "é".repeat(400);
        let documents = [UntrustedDocument {
            source: "doc",
            content: &prefix_document,
        }];
        let agent = agent(vec![question("q", 5)]);
        let kept = validate_answers(
            &agent,
            &output(json!({"q": with_evidence("forte", &prefix_document)})),
            &documents,
        )
        .unwrap();
        assert!(kept.answers.0[0].grounded);
        assert_eq!(kept.answers.0[0].evidence.chars().count(), 300);
    }

    #[test]
    fn an_unverifiable_quote_downgrades_the_choice_to_the_unknown_option() {
        for evidence in ["Keycloak 99 partout", "", "   "] {
            let answer = validated(with_evidence("forte", evidence));
            assert!(!answer.grounded, "{evidence}");
            assert_eq!(answer.choice, "inconnu");
            assert_eq!(answer.original_choice.as_deref(), Some("forte"));
            assert_eq!(answer.evidence_source, None);
            assert_eq!(
                answer.probabilities,
                vec![
                    ("forte".to_owned(), 0.0),
                    ("moyenne".to_owned(), 0.0),
                    ("faible".to_owned(), 0.0),
                    ("inconnu".to_owned(), 1.0),
                ]
            );
        }
    }

    #[test]
    fn the_unknown_option_needs_no_quote() {
        let mut raw = with_evidence("inconnu", "");
        raw["probabilities"] = json!({"forte": 0.0, "moyenne": 0.0, "faible": 0.0, "inconnu": 1.0});
        let answer = validated(raw);
        assert!(answer.grounded);
        assert_eq!(answer.choice, "inconnu");
        assert_eq!(answer.original_choice, None);
        let invented = validated(with_evidence("inconnu", "made up"));
        assert!(invented.grounded);
        assert_eq!(invented.evidence, "");
    }

    #[test]
    fn a_missing_or_non_text_evidence_breaks_the_contract() {
        let agent = agent(vec![question("q", 5)]);
        for bad in [Value::Null, json!(3), json!(["x"])] {
            let mut value = with_evidence("forte", "x");
            value["evidence"] = bad;
            assert_eq!(
                violations(&agent, output(json!({"q": value}))),
                vec![Violation::BadEvidence {
                    question: "q".to_owned()
                }]
            );
        }
        let mut absent = with_evidence("forte", "x");
        absent.as_object_mut().unwrap().remove("evidence");
        assert_eq!(
            violations(&agent, output(json!({"q": absent}))),
            vec![Violation::BadEvidence {
                question: "q".to_owned()
            }]
        );
    }

    #[test]
    fn a_downgrade_never_invalidates_the_whole_output() {
        let agent = agent(vec![question("a", 5), question("b", 5)]);
        let valid = validate_answers(
            &agent,
            &output(json!({
                "a": with_evidence("forte", "expose Keycloak 18"),
                "b": with_evidence("forte", "invented")
            })),
            &docs(),
        )
        .unwrap();
        assert_eq!(valid.answers.ungrounded(), ["b"]);
    }

    #[test]
    fn grounding_flags_survive_storage_and_rescoring_uses_the_downgraded_answers() {
        let question = question("q", 5);
        let agent = agent(vec![question.clone()]);
        let grounded = valid_answers(
            &agent,
            json!({"q": with_evidence("forte", "expose Keycloak 18")}),
        );
        let downgraded = valid_answers(&agent, json!({"q": with_evidence("forte", "invented")}));
        let stored = downgraded.to_json();
        assert_eq!(stored["q"]["grounded"], false);
        assert_eq!(stored["q"]["original_choice"], "forte");
        assert_eq!(stored["q"]["choice"], "inconnu");
        assert_eq!(stored["q"]["evidence"], "invented");
        let read = stored_answers(&agent, &stored).unwrap();
        assert_eq!(read, downgraded);
        assert_eq!(
            stored_answers(&agent, &grounded.to_json()).unwrap(),
            grounded
        );
        assert!((agent_score(&agent, &read, 0.0) - 0.5).abs() < 1e-12);
        assert!((agent_score(&agent, &grounded, 0.0) - 1.0).abs() < 1e-12);
        let calibrated = agent_score(&agent, &read, 0.05);
        assert!((calibrated - 0.5).abs() < 1e-9);
    }

    #[test]
    fn answers_stored_before_evidence_existed_are_still_read() {
        let agent = agent(vec![question("q", 5)]);
        let legacy = json!({"q": {
            "type": "choice", "choice": "forte", "confidence": 0.9,
            "probabilities": {"forte": 1.0, "moyenne": 0.0, "faible": 0.0, "inconnu": 0.0}
        }});
        let read = stored_answers(&agent, &legacy).unwrap();
        assert!(read.0[0].grounded);
        assert_eq!(read.0[0].evidence, "");
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
