use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::{
    lead::str_enum,
    profile::{
        ProfileId,
        definition::{AgentId, ProfileDefinition, QualificationAgent, ScoringAgent},
    },
    qualification::{ValidAnswers, agent_score, stored_answers},
    scoring_agent::{VerdictAnswer, VerdictProbabilities},
};

pub mod isotonic;
pub mod ports;

pub use isotonic::{IsotonicMap, Point, Sample};

pub const MIN_SAMPLES: u64 = 50;
pub const MIN_PER_CLASS: u64 = 10;
pub const RELIABILITY_BINS: usize = 10;
pub const GOOD_VERDICT_SHARE: f64 = 0.5;
pub const CALIBRATION_JOB_KIND: &str = "quality.calibrate";
pub const CALIBRATION_JOB_INTERVAL_HOURS: i64 = 24;

fn count(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

fn total(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CalibrationKind {
    Scoring,
    Qualification,
}

str_enum!(CalibrationKind {
    Scoring => "scoring",
    Qualification => "qualification",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationStatus {
    Active,
    Insufficient,
    Disabled,
}

str_enum!(CalibrationStatus {
    Active => "active",
    Insufficient => "insufficient",
    Disabled => "disabled",
});

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReliabilityBin {
    pub n: u64,
    pub predicted: f64,
    pub observed: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    pub profile_id: ProfileId,
    pub kind: CalibrationKind,
    pub agent_id: AgentId,
    pub map: Option<IsotonicMap>,
    pub n: u64,
    pub positives: u64,
    pub brier_before: Option<f64>,
    pub brier_after: Option<f64>,
    pub reliability: Vec<ReliabilityBin>,
    pub fitted_at: DateTime<Utc>,
    pub enabled: bool,
}

impl Calibration {
    pub fn applies(&self) -> bool {
        self.enabled && self.map.is_some()
    }

    pub fn predict(&self, x: f64) -> Option<f64> {
        self.map.as_ref().map(|map| map.predict(x))
    }

    pub fn status(&self, learned: bool) -> CalibrationStatus {
        if !learned {
            CalibrationStatus::Disabled
        } else if self.applies() {
            CalibrationStatus::Active
        } else {
            CalibrationStatus::Insufficient
        }
    }

    fn same_effect(&self, other: &Self) -> bool {
        self.applies() == other.applies() && (!self.applies() || self.map == other.map)
    }

    fn empty(
        profile_id: ProfileId,
        kind: CalibrationKind,
        agent_id: &AgentId,
        at: DateTime<Utc>,
    ) -> Self {
        Self {
            profile_id,
            kind,
            agent_id: agent_id.clone(),
            map: None,
            n: 0,
            positives: 0,
            brier_before: None,
            brier_after: None,
            reliability: Vec::new(),
            fitted_at: at,
            enabled: false,
        }
    }
}

pub fn agent_learns(
    definition: &ProfileDefinition,
    kind: CalibrationKind,
    agent: &AgentId,
) -> bool {
    match kind {
        CalibrationKind::Scoring => definition
            .scoring_agents
            .iter()
            .any(|item| &item.id == agent && item.learned_calibration),
        CalibrationKind::Qualification => definition
            .qualification_agents
            .iter()
            .any(|item| &item.id == agent && item.learned_calibration),
    }
}

pub fn effect_changed(
    definition: &ProfileDefinition,
    before: &[Calibration],
    after: &[Calibration],
) -> bool {
    let matching = |known: &[Calibration], wanted: &Calibration| {
        known
            .iter()
            .find(|item| item.kind == wanted.kind && item.agent_id == wanted.agent_id)
            .is_some_and(|item| item.same_effect(wanted))
    };
    let counts =
        |item: &&Calibration| item.applies() && agent_learns(definition, item.kind, &item.agent_id);
    let lost = before
        .iter()
        .filter(counts)
        .any(|item| !matching(after, item));
    let gained = after
        .iter()
        .filter(counts)
        .any(|item| !matching(before, item));
    lost || gained
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Calibrations {
    applied: Vec<Calibration>,
}

impl Calibrations {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn for_definition(stored: Vec<Calibration>, definition: &ProfileDefinition) -> Self {
        Self {
            applied: stored
                .into_iter()
                .filter(|item| {
                    item.applies() && agent_learns(definition, item.kind, &item.agent_id)
                })
                .collect(),
        }
    }

    pub fn applied(stored: Vec<Calibration>, learned: bool) -> Self {
        if !learned {
            return Self::none();
        }
        Self {
            applied: stored.into_iter().filter(Calibration::applies).collect(),
        }
    }

    fn find(&self, kind: CalibrationKind, agent: &AgentId) -> Option<&Calibration> {
        self.applied
            .iter()
            .find(|item| item.kind == kind && &item.agent_id == agent)
    }

    pub fn scoring(&self, agent: &AgentId) -> Option<&Calibration> {
        self.find(CalibrationKind::Scoring, agent)
    }

    pub fn qualification(&self, agent: &AgentId) -> Option<&Calibration> {
        self.find(CalibrationKind::Qualification, agent)
    }

    pub fn is_empty(&self) -> bool {
        self.applied.is_empty()
    }
}

impl VerdictProbabilities {
    pub fn good(&self) -> f64 {
        self.reliable + GOOD_VERDICT_SHARE * self.to_check
    }

    #[must_use]
    pub fn with_good(self, good: f64) -> Self {
        let current = self.normalised();
        let to_check = current.to_check.clamp(0.0, 1.0);
        let room = 1.0 - to_check;
        let reliable = (good - GOOD_VERDICT_SHARE * to_check).clamp(0.0, room);
        Self::new(reliable, to_check, room - reliable)
    }
}

pub fn calibrate_verdict(
    probabilities: VerdictProbabilities,
    agent: &ScoringAgent,
    calibrations: &Calibrations,
) -> VerdictProbabilities {
    calibrations
        .scoring(&agent.id)
        .and_then(|calibration| calibration.predict(probabilities.normalised().good()))
        .map_or(probabilities, |good| probabilities.with_good(good))
}

pub fn raw_agent_score(agent: &QualificationAgent, answers: &ValidAnswers) -> f64 {
    agent_score(agent, answers, 0.0)
}

pub fn opinion_score(
    agent: &QualificationAgent,
    answers: &ValidAnswers,
    floor: f64,
    calibrations: &Calibrations,
) -> f64 {
    calibrations
        .qualification(&agent.id)
        .and_then(|calibration| calibration.predict(raw_agent_score(agent, answers)))
        .unwrap_or_else(|| agent_score(agent, answers, floor))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Labelled {
    pub x: f64,
    pub validated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fit {
    pub n: u64,
    pub positives: u64,
    pub brier_before: Option<f64>,
    pub brier_after: Option<f64>,
    pub map: Option<IsotonicMap>,
    pub reliability: Vec<ReliabilityBin>,
}

pub fn sufficient(n: u64, positives: u64) -> bool {
    n >= MIN_SAMPLES && positives >= MIN_PER_CLASS && n.saturating_sub(positives) >= MIN_PER_CLASS
}

fn outcome(validated: bool) -> f64 {
    if validated { 1.0 } else { 0.0 }
}

fn brier(predictions: impl Iterator<Item = (f64, bool)>) -> Option<f64> {
    let (sum, items) = predictions.fold((0.0, 0usize), |(sum, items), (predicted, validated)| {
        (sum + (predicted - outcome(validated)).powi(2), items + 1)
    });
    (items > 0).then(|| sum / count(items))
}

fn bin_of(x: f64) -> usize {
    (0..RELIABILITY_BINS)
        .rev()
        .find(|bin| x >= count(*bin) / count(RELIABILITY_BINS))
        .unwrap_or(0)
}

pub fn reliability(observations: &[Labelled]) -> Vec<ReliabilityBin> {
    let mut bins = [(0usize, 0.0_f64, 0.0_f64); RELIABILITY_BINS];
    for observation in observations {
        let slot = &mut bins[bin_of(observation.x)];
        slot.0 += 1;
        slot.1 += observation.x;
        slot.2 += outcome(observation.validated);
    }
    bins.iter()
        .filter(|(items, _, _)| *items > 0)
        .map(|(items, predicted, observed)| ReliabilityBin {
            n: total(*items),
            predicted: predicted / count(*items),
            observed: observed / count(*items),
        })
        .collect()
}

pub fn fit(observations: &[Labelled]) -> Fit {
    let clamped: Vec<Labelled> = observations
        .iter()
        .filter(|observation| observation.x.is_finite())
        .map(|observation| Labelled {
            x: observation.x.clamp(0.0, 1.0),
            validated: observation.validated,
        })
        .collect();
    let n = total(clamped.len());
    let positives = total(clamped.iter().filter(|item| item.validated).count());
    let brier_before = brier(clamped.iter().map(|item| (item.x, item.validated)));
    let map = sufficient(n, positives)
        .then(|| {
            let samples: Vec<Sample> = clamped
                .iter()
                .map(|item| Sample::unit(item.x, outcome(item.validated)))
                .collect();
            IsotonicMap::fit(&samples)
        })
        .flatten();
    let brier_after = map.as_ref().and_then(|map| {
        brier(
            clamped
                .iter()
                .map(|item| (map.predict(item.x), item.validated)),
        )
    });
    Fit {
        n,
        positives,
        brier_before,
        brier_after,
        map,
        reliability: reliability(&clamped),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabelledScoring {
    pub agent_id: AgentId,
    pub validated: bool,
    pub verdict: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabelledQualification {
    pub agent_id: AgentId,
    pub validated: bool,
    pub answers: Value,
}

fn calibration_of(
    profile_id: ProfileId,
    kind: CalibrationKind,
    agent_id: &AgentId,
    observations: &[Labelled],
    at: DateTime<Utc>,
) -> Calibration {
    if observations.is_empty() {
        return Calibration::empty(profile_id, kind, agent_id, at);
    }
    let fitted = fit(observations);
    Calibration {
        profile_id,
        kind,
        agent_id: agent_id.clone(),
        enabled: fitted.map.is_some(),
        map: fitted.map,
        n: fitted.n,
        positives: fitted.positives,
        brier_before: fitted.brier_before,
        brier_after: fitted.brier_after,
        reliability: fitted.reliability,
        fitted_at: at,
    }
}

pub fn fit_profile(
    profile_id: ProfileId,
    definition: &ProfileDefinition,
    scoring: &[LabelledScoring],
    qualification: &[LabelledQualification],
    at: DateTime<Utc>,
) -> Vec<Calibration> {
    let mut fitted = Vec::new();
    for agent in &definition.scoring_agents {
        let observations: Vec<Labelled> = scoring
            .iter()
            .filter(|row| row.agent_id == agent.id)
            .filter_map(|row| {
                let answer = VerdictAnswer::from_json(&row.verdict)?;
                Some(Labelled {
                    x: answer.probabilities.normalised().good(),
                    validated: row.validated,
                })
            })
            .collect();
        fitted.push(calibration_of(
            profile_id,
            CalibrationKind::Scoring,
            &agent.id,
            &observations,
            at,
        ));
    }
    for agent in &definition.qualification_agents {
        let observations: Vec<Labelled> = qualification
            .iter()
            .filter(|row| row.agent_id == agent.id)
            .filter_map(|row| {
                let answers = stored_answers(agent, &row.answers)?;
                Some(Labelled {
                    x: raw_agent_score(agent, &answers),
                    validated: row.validated,
                })
            })
            .collect();
        fitted.push(calibration_of(
            profile_id,
            CalibrationKind::Qualification,
            &agent.id,
            &observations,
            at,
        ));
    }
    fitted
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentCalibration {
    pub kind: CalibrationKind,
    pub agent_id: String,
    pub name: String,
    pub n: u64,
    pub positives: u64,
    pub brier_before: Option<f64>,
    pub brier_after: Option<f64>,
    pub status: CalibrationStatus,
    pub reliability: Vec<ReliabilityBin>,
    pub fitted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileCalibration {
    pub profile_id: ProfileId,
    pub name: String,
    pub agents: Vec<AgentCalibration>,
}

pub fn profile_report(
    profile_id: ProfileId,
    name: &str,
    definition: &ProfileDefinition,
    stored: &[Calibration],
) -> ProfileCalibration {
    let entry = |kind: CalibrationKind, id: &AgentId, label: &str, learned: bool| {
        let found = stored
            .iter()
            .find(|item| item.kind == kind && &item.agent_id == id);
        AgentCalibration {
            kind,
            agent_id: id.as_str().to_owned(),
            name: label.to_owned(),
            n: found.map_or(0, |item| item.n),
            positives: found.map_or(0, |item| item.positives),
            brier_before: found.and_then(|item| item.brier_before),
            brier_after: found.and_then(|item| item.brier_after),
            status: found.map_or(
                if learned {
                    CalibrationStatus::Insufficient
                } else {
                    CalibrationStatus::Disabled
                },
                |item| item.status(learned),
            ),
            reliability: found
                .map(|item| item.reliability.clone())
                .unwrap_or_default(),
            fitted_at: found.map(|item| item.fitted_at),
        }
    };
    let agents = definition
        .scoring_agents
        .iter()
        .map(|agent| {
            entry(
                CalibrationKind::Scoring,
                &agent.id,
                &agent.name,
                agent.learned_calibration,
            )
        })
        .chain(definition.qualification_agents.iter().map(|agent| {
            entry(
                CalibrationKind::Qualification,
                &agent.id,
                &agent.name,
                agent.learned_calibration,
            )
        }))
        .collect();
    ProfileCalibration {
        profile_id,
        name: name.to_owned(),
        agents,
    }
}

#[cfg(test)]
mod tests;
