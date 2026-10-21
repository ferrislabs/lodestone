use super::{ScoringResult, VerdictProbabilities};
use crate::domain::{
    calibration::{Calibrations, calibrate_verdict},
    profile::{contract::MAX_SCORE, definition::ScoringAgent},
    qualification::{Exclusion, Verdict, VerdictReason},
};

const EXCLUDED_CONFIDENCE: f64 = 100.0;

pub fn round_score(value: f64) -> u8 {
    let rounded = value.round().clamp(0.0, f64::from(MAX_SCORE));
    (0..=MAX_SCORE)
        .find(|candidate| f64::from(*candidate) >= rounded)
        .unwrap_or(MAX_SCORE)
}

#[derive(Debug, Clone, Copy)]
pub struct AgentDecision<'a> {
    pub agent: &'a ScoringAgent,
    pub score: u8,
    pub probabilities: VerdictProbabilities,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub score: Option<u8>,
    pub verdict: Option<Verdict>,
    pub confidence: Option<f64>,
    pub probabilities: Option<VerdictProbabilities>,
    pub reason: Option<VerdictReason>,
    pub exclusions: Vec<Exclusion>,
    pub contributors: usize,
}

impl Decision {
    pub fn reason_text(&self) -> Option<&'static str> {
        self.reason.map(VerdictReason::as_str)
    }
}

fn percent(probability: f64) -> f64 {
    (probability * 1000.0).round() / 10.0
}

pub fn combine(
    inputs: &[AgentDecision<'_>],
    calibrations: &Calibrations,
    exclusions: Vec<Exclusion>,
) -> Decision {
    let weights: Vec<f64> = inputs
        .iter()
        .map(|input| f64::from(input.agent.weight))
        .collect();
    let total: f64 = weights.iter().sum();
    let score = (total > 0.0).then(|| {
        let weighted: f64 = inputs
            .iter()
            .zip(&weights)
            .map(|(input, weight)| weight * f64::from(input.score))
            .sum();
        round_score(weighted / total)
    });
    let calibrated: Vec<(f64, VerdictProbabilities)> = inputs
        .iter()
        .zip(&weights)
        .map(|(input, weight)| {
            (
                *weight,
                calibrate_verdict(input.probabilities, input.agent, calibrations),
            )
        })
        .collect();
    let probabilities = VerdictProbabilities::weighted_mean(&calibrated);
    if !exclusions.is_empty() {
        return Decision {
            score,
            verdict: Some(Verdict::Discarded),
            confidence: Some(EXCLUDED_CONFIDENCE),
            probabilities,
            reason: Some(VerdictReason::Excluded),
            exclusions,
            contributors: inputs.len(),
        };
    }
    match probabilities {
        Some(probabilities) => {
            let verdict = probabilities.argmax();
            Decision {
                score,
                verdict: Some(verdict),
                confidence: Some(percent(probabilities.get(verdict))),
                probabilities: Some(probabilities),
                reason: None,
                exclusions,
                contributors: inputs.len(),
            }
        }
        None => Decision {
            score: None,
            verdict: None,
            confidence: None,
            probabilities: None,
            reason: Some(VerdictReason::AwaitingScoring),
            exclusions,
            contributors: 0,
        },
    }
}

pub fn decide(
    agents: &[ScoringAgent],
    results: &[ScoringResult],
    calibrations: &Calibrations,
    exclusions: Vec<Exclusion>,
) -> Decision {
    let inputs: Vec<AgentDecision<'_>> = agents
        .iter()
        .filter(|agent| agent.enabled)
        .filter_map(|agent| {
            let result = results.iter().find(|result| result.agent_id == agent.id)?;
            let (score, verdict) = result.valid_parts()?;
            Some(AgentDecision {
                agent,
                score,
                probabilities: verdict.probabilities,
            })
        })
        .collect();
    combine(&inputs, calibrations, exclusions)
}
