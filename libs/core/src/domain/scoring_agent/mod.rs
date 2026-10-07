use std::fmt;

use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::domain::{
    lead::{LeadId, uuid_id},
    llm::{RunProvenance, UntrustedDocument},
    profile::{
        ProfileId, RevisionNumber,
        contract::{MAX_REASONS, MAX_SCORE, REASON_MAX_CHARS, VERDICT_VALUES},
        definition::{AgentId, ScoringAgent},
    },
    qualification::{
        ResultStatus, ValidAnswers, Verdict, Violation, most_agreeing, stored_answers_for, unit,
        validate_questions, vote_answers,
    },
};

pub mod combination;
pub mod ports;
pub mod prompt;

pub use combination::{AgentDecision, Decision, calibrate_verdict, combine, decide, round_score};

uuid_id!(ScoringResultId);

const PROBABILITY_TOLERANCE: f64 = 0.05;
const EPSILON: f64 = 1e-9;
const PRUDENT_ORDER: [Verdict; 3] = [Verdict::ToCheck, Verdict::Discarded, Verdict::Reliable];

fn count(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerdictProbabilities {
    pub reliable: f64,
    pub to_check: f64,
    pub discarded: f64,
}

impl VerdictProbabilities {
    pub const fn new(reliable: f64, to_check: f64, discarded: f64) -> Self {
        Self {
            reliable,
            to_check,
            discarded,
        }
    }

    pub fn get(&self, verdict: Verdict) -> f64 {
        match verdict {
            Verdict::Reliable => self.reliable,
            Verdict::ToCheck => self.to_check,
            Verdict::Discarded => self.discarded,
        }
    }

    pub fn sum(&self) -> f64 {
        self.reliable + self.to_check + self.discarded
    }

    #[must_use]
    pub fn normalised(self) -> Self {
        let sum = self.sum();
        if sum <= EPSILON {
            return self;
        }
        Self::new(
            self.reliable / sum,
            self.to_check / sum,
            self.discarded / sum,
        )
    }

    pub fn argmax(&self) -> Verdict {
        let mut best = PRUDENT_ORDER[0];
        for verdict in PRUDENT_ORDER {
            if self.get(verdict) > self.get(best) + EPSILON {
                best = verdict;
            }
        }
        best
    }

    pub fn weighted_mean(items: &[(f64, Self)]) -> Option<Self> {
        let total: f64 = items.iter().map(|(weight, _)| weight).sum();
        if total <= 0.0 {
            return None;
        }
        let mean = |pick: fn(&Self) -> f64| {
            items
                .iter()
                .map(|(weight, probabilities)| weight * pick(probabilities))
                .sum::<f64>()
                / total
        };
        Some(
            Self::new(
                mean(|p| p.reliable),
                mean(|p| p.to_check),
                mean(|p| p.discarded),
            )
            .normalised(),
        )
    }

    pub fn to_json(&self) -> Value {
        json!({
            "fiable": self.reliable,
            "a_verifier": self.to_check,
            "ecarte": self.discarded,
        })
    }

    pub fn from_json(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let read = |key: &str| object.get(key).and_then(unit);
        Some(Self::new(
            read("fiable")?,
            read("a_verifier")?,
            read("ecarte")?,
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerdictAnswer {
    pub choice: Verdict,
    pub confidence: f64,
    pub probabilities: VerdictProbabilities,
    pub declared_choice: Option<Verdict>,
}

impl VerdictAnswer {
    pub fn from_probabilities(probabilities: VerdictProbabilities) -> Self {
        let choice = probabilities.argmax();
        Self {
            choice,
            confidence: probabilities.get(choice),
            probabilities,
            declared_choice: None,
        }
    }

    pub fn to_json(&self) -> Value {
        let mut value = json!({
            "choice": self.choice.as_str(),
            "confidence": self.confidence,
            "probabilities": self.probabilities.to_json(),
        });
        if let Some(declared) = self.declared_choice {
            value["declared_choice"] = json!(declared.as_str());
        }
        value
    }

    pub fn from_json(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        Some(Self {
            choice: Verdict::parse(object.get("choice")?.as_str()?)?,
            confidence: object.get("confidence").and_then(unit)?,
            probabilities: VerdictProbabilities::from_json(object.get("probabilities")?)?,
            declared_choice: object
                .get("declared_choice")
                .and_then(Value::as_str)
                .and_then(Verdict::parse),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoringOutput {
    pub criteria: ValidAnswers,
    pub score: u8,
    pub verdict: VerdictAnswer,
    pub reasons: Vec<String>,
}

impl ScoringOutput {
    pub fn to_json(&self) -> Value {
        json!({
            "criteria": self.criteria.to_json(),
            "score": self.score,
            "verdict": self.verdict.to_json(),
            "reasons": self.reasons,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum ScoringViolation {
    #[error("la réponse n'est pas un objet JSON")]
    NotAnObject,
    #[error("la clé `criteria` est absente ou n'est pas un objet")]
    MissingCriteria,
    #[error("{0}")]
    Criterion(Violation),
    #[error("`score` doit être un entier entre 0 et {MAX_SCORE}")]
    BadScore,
    #[error("la clé `verdict` est absente ou n'est pas un objet")]
    MissingVerdict,
    #[error("`verdict.choice` doit être fiable, a_verifier ou ecarte, reçu {choice}")]
    BadVerdictChoice { choice: String },
    #[error("`verdict.confidence` doit être un nombre entre 0 et 1")]
    BadVerdictConfidence,
    #[error("`verdict.probabilities` doit contenir exactement fiable, a_verifier et ecarte")]
    VerdictProbabilityKeys,
    #[error("`verdict.probabilities.{option}` doit être entre 0 et 1")]
    BadVerdictProbability { option: String },
    #[error("les probabilités du verdict totalisent {sum}, attendu 1 à 0,05 près")]
    VerdictProbabilitySum { sum: f64 },
    #[error("`reasons` doit contenir de 1 à {MAX_REASONS} chaînes non vides")]
    BadReasons,
    #[error("`reasons[{index}]` fait {length} caractères, le maximum est {REASON_MAX_CHARS}")]
    ReasonTooLong { index: usize, length: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoringViolations(pub Vec<ScoringViolation>);

impl fmt::Display for ScoringViolations {
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

fn check_score(root: &Map<String, Value>) -> Result<u8, ScoringViolation> {
    root.get("score")
        .and_then(Value::as_f64)
        .filter(|value| {
            value.is_finite()
                && value.fract() == 0.0
                && (0.0..=f64::from(MAX_SCORE)).contains(value)
        })
        .map(round_score)
        .ok_or(ScoringViolation::BadScore)
}

fn check_verdict(root: &Map<String, Value>) -> Result<VerdictAnswer, Vec<ScoringViolation>> {
    let Some(object) = root.get("verdict").and_then(Value::as_object) else {
        return Err(vec![ScoringViolation::MissingVerdict]);
    };
    let mut violations = Vec::new();
    let declared = object
        .get("choice")
        .and_then(Value::as_str)
        .and_then(|raw| {
            VERDICT_VALUES
                .contains(&raw)
                .then(|| Verdict::parse(raw))
                .flatten()
        });
    if declared.is_none() {
        violations.push(ScoringViolation::BadVerdictChoice {
            choice: object
                .get("choice")
                .map_or_else(|| "null".to_owned(), Value::to_string),
        });
    }
    if object.get("confidence").and_then(unit).is_none() {
        violations.push(ScoringViolation::BadVerdictConfidence);
    }
    let probabilities = check_verdict_probabilities(object.get("probabilities"), &mut violations);
    match (declared, probabilities) {
        (Some(declared), Some(probabilities)) if violations.is_empty() => {
            let mut answer = VerdictAnswer::from_probabilities(probabilities);
            if answer.choice != declared {
                answer.declared_choice = Some(declared);
            }
            Ok(answer)
        }
        _ => Err(violations),
    }
}

fn check_verdict_probabilities(
    raw: Option<&Value>,
    violations: &mut Vec<ScoringViolation>,
) -> Option<VerdictProbabilities> {
    let Some(object) = raw.and_then(Value::as_object) else {
        violations.push(ScoringViolation::VerdictProbabilityKeys);
        return None;
    };
    if object.len() != VERDICT_VALUES.len()
        || !VERDICT_VALUES.iter().all(|key| object.contains_key(*key))
    {
        violations.push(ScoringViolation::VerdictProbabilityKeys);
        return None;
    }
    let mut parsed = [0.0; 3];
    let mut bounded = true;
    for (slot, key) in parsed.iter_mut().zip(VERDICT_VALUES) {
        match object.get(key).and_then(unit) {
            Some(probability) => *slot = probability,
            None => {
                bounded = false;
                violations.push(ScoringViolation::BadVerdictProbability {
                    option: key.to_owned(),
                });
            }
        }
    }
    if !bounded {
        return None;
    }
    let sum: f64 = parsed.iter().sum();
    if sum <= EPSILON || (sum - 1.0).abs() > PROBABILITY_TOLERANCE + EPSILON {
        violations.push(ScoringViolation::VerdictProbabilitySum { sum });
        return None;
    }
    Some(VerdictProbabilities::new(parsed[0], parsed[1], parsed[2]).normalised())
}

fn check_reasons(root: &Map<String, Value>) -> Result<Vec<String>, Vec<ScoringViolation>> {
    let Some(items) = root.get("reasons").and_then(Value::as_array) else {
        return Err(vec![ScoringViolation::BadReasons]);
    };
    if items.is_empty() || items.len() > MAX_REASONS {
        return Err(vec![ScoringViolation::BadReasons]);
    }
    let mut reasons = Vec::with_capacity(items.len());
    let mut violations = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(text) = item.as_str().map(str::trim).filter(|text| !text.is_empty()) else {
            violations.push(ScoringViolation::BadReasons);
            continue;
        };
        let length = text.chars().count();
        if length > REASON_MAX_CHARS {
            violations.push(ScoringViolation::ReasonTooLong { index, length });
            continue;
        }
        reasons.push(text.to_owned());
    }
    if violations.is_empty() {
        Ok(reasons)
    } else {
        Err(violations)
    }
}

pub fn validate_scoring(
    agent: &ScoringAgent,
    output: &Value,
    documents: &[UntrustedDocument<'_>],
) -> Result<ScoringOutput, ScoringViolations> {
    let Some(root) = output.as_object() else {
        return Err(ScoringViolations(vec![ScoringViolation::NotAnObject]));
    };
    let mut violations = Vec::new();
    let criteria = match root.get("criteria").and_then(Value::as_object) {
        Some(answers) => {
            let mut found = Vec::new();
            let valid = validate_questions(&agent.criteria, answers, documents, &mut found);
            violations.extend(found.into_iter().map(ScoringViolation::Criterion));
            Some(valid)
        }
        None => {
            violations.push(ScoringViolation::MissingCriteria);
            None
        }
    };
    let score = check_score(root)
        .map_err(|violation| violations.push(violation))
        .ok();
    let verdict = check_verdict(root)
        .map_err(|found| violations.extend(found))
        .ok();
    let reasons = check_reasons(root)
        .map_err(|found| violations.extend(found))
        .ok();
    match (criteria, score, verdict, reasons) {
        (Some(criteria), Some(score), Some(verdict), Some(reasons)) if violations.is_empty() => {
            Ok(ScoringOutput {
                criteria: ValidAnswers(criteria),
                score,
                verdict,
                reasons,
            })
        }
        _ => Err(ScoringViolations(violations)),
    }
}

fn mean_score(samples: &[ScoringOutput]) -> u8 {
    let total: f64 = samples.iter().map(|sample| f64::from(sample.score)).sum();
    round_score(total / count(samples.len()))
}

fn mean_probabilities(samples: &[ScoringOutput]) -> VerdictProbabilities {
    let total = count(samples.len());
    let mean = |pick: fn(&VerdictProbabilities) -> f64| {
        samples
            .iter()
            .map(|sample| pick(&sample.verdict.probabilities))
            .sum::<f64>()
            / total
    };
    VerdictProbabilities::new(
        mean(|p| p.reliable),
        mean(|p| p.to_check),
        mean(|p| p.discarded),
    )
    .normalised()
}

pub fn vote_scoring(agent: &ScoringAgent, samples: &[ScoringOutput]) -> Option<ScoringOutput> {
    let answers: Vec<&ValidAnswers> = samples.iter().map(|sample| &sample.criteria).collect();
    let criteria = vote_answers(&agent.criteria, &answers)?;
    if samples.len() == 1 {
        return Some(ScoringOutput {
            criteria,
            ..samples[0].clone()
        });
    }
    let verdict = VerdictAnswer::from_probabilities(mean_probabilities(samples));
    let aligned: Vec<usize> = samples
        .iter()
        .enumerate()
        .filter(|(_, sample)| sample.verdict.choice == verdict.choice)
        .map(|(index, _)| index)
        .collect();
    let candidates: Vec<usize> = if aligned.is_empty() {
        (0..samples.len()).collect()
    } else {
        aligned
    };
    let pool: Vec<&ValidAnswers> = candidates.iter().map(|index| answers[*index]).collect();
    let best = candidates[most_agreeing(&pool, &criteria)];
    Some(ScoringOutput {
        criteria,
        score: mean_score(samples),
        verdict,
        reasons: samples[best].reasons.clone(),
    })
}

pub fn scoring_sample_record(index: u32, outcome: Result<&ScoringOutput, &str>) -> Value {
    match outcome {
        Ok(output) => {
            let mut record = output.to_json();
            record["index"] = json!(index);
            record["valid"] = json!(true);
            record
        }
        Err(error) => json!({
            "index": index,
            "valid": false,
            "error": error,
        }),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoringResult {
    pub id: ScoringResultId,
    pub lead_id: LeadId,
    pub profile_id: ProfileId,
    pub profile_revision: RevisionNumber,
    pub agent_id: AgentId,
    pub provenance: RunProvenance,
    pub status: ResultStatus,
    pub criteria: Value,
    pub score: Option<u8>,
    pub verdict: Option<VerdictAnswer>,
    pub reasons: Vec<String>,
    pub error: Option<String>,
    pub raw_samples: Value,
    pub created_at: DateTime<Utc>,
}

impl ScoringResult {
    pub fn valid_parts(&self) -> Option<(u8, &VerdictAnswer)> {
        if self.status != ResultStatus::Valid {
            return None;
        }
        Some((self.score?, self.verdict.as_ref()?))
    }

    pub fn criteria_answers(&self, agent: &ScoringAgent) -> Option<ValidAnswers> {
        stored_answers_for(&agent.criteria, &self.criteria)
    }
}

#[cfg(test)]
mod tests;
