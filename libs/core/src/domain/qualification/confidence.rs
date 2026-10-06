use serde::{Deserialize, Serialize};

use super::Verdict;
use crate::domain::{
    facts::LeadFacts,
    lead::str_enum,
    profile::definition::{EvidenceSettings, EvidenceWeights, VerdictThresholds},
};

pub const SCORING_VERSION: i32 = 1;

const VOLUME_SCALE: f64 = 3.0;
const SOURCES_FOR_FULL_DIVERSITY: f64 = 2.0;
const UNKNOWN_ENVIRONMENT: f64 = 0.5;
const INDEPENDENT_ANGLES: usize = 2;

fn count(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

pub fn calibrate(probabilities: &[(String, f64)], floor: f64) -> Vec<(String, f64)> {
    let clamped: Vec<(String, f64)> = probabilities
        .iter()
        .map(|(option, probability)| (option.clone(), probability.clamp(floor, 1.0 - floor)))
        .collect();
    let total: f64 = clamped.iter().map(|(_, probability)| probability).sum();
    if total <= 0.0 {
        return clamped;
    }
    clamped
        .into_iter()
        .map(|(option, probability)| (option, probability / total))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AgentOpinion {
    pub weight: u8,
    pub score: f64,
    pub axes: usize,
}

pub fn opinion(agents: &[AgentOpinion]) -> Option<f64> {
    let total: f64 = agents.iter().map(|agent| f64::from(agent.weight)).sum();
    if total <= 0.0 {
        return None;
    }
    let weighted: f64 = agents
        .iter()
        .map(|agent| f64::from(agent.weight) * agent.score)
        .sum();
    Some(weighted / total)
}

pub fn has_independent_angles(agents: &[AgentOpinion]) -> bool {
    agents.len() >= INDEPENDENT_ANGLES
        || agents.iter().any(|agent| agent.axes >= INDEPENDENT_ANGLES)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EvidenceComponents {
    pub volume: f64,
    pub diversity: f64,
    pub environment: f64,
    pub strength: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EvidenceStrength {
    pub score: f64,
    pub components: EvidenceComponents,
    pub signal_count: usize,
    pub signal_score: f64,
    pub only_non_production: bool,
}

fn weighted_mean(components: &EvidenceComponents, weights: &EvidenceWeights) -> f64 {
    let pairs = [
        (weights.volume, components.volume),
        (weights.diversity, components.diversity),
        (weights.environment, components.environment),
        (weights.strength, components.strength),
    ];
    let total: f64 = pairs.iter().map(|(weight, _)| f64::from(*weight)).sum();
    if total <= 0.0 {
        return 0.0;
    }
    let weighted: f64 = pairs
        .iter()
        .map(|(weight, value)| f64::from(*weight) * value)
        .sum();
    weighted / total
}

impl EvidenceStrength {
    pub fn compute(facts: &LeadFacts, signal_score: f64, settings: &EvidenceSettings) -> Self {
        let components = EvidenceComponents {
            volume: 1.0 - (-count(facts.signal_count) / VOLUME_SCALE).exp(),
            diversity: (count(facts.sources.len()) / SOURCES_FOR_FULL_DIVERSITY).min(1.0),
            environment: facts.production_share().unwrap_or(UNKNOWN_ENVIRONMENT),
            strength: (signal_score / settings.reference_score).clamp(0.0, 1.0),
        };
        Self {
            score: weighted_mean(&components, &settings.weights).clamp(0.0, 1.0),
            components,
            signal_count: facts.signal_count,
            signal_score,
            only_non_production: facts.only_non_production(),
        }
    }
}

pub fn blend(evidence: f64, influence: f64) -> f64 {
    (1.0 - influence) + influence * evidence
}

pub fn confidence_percent(opinion: f64, evidence: f64, influence: f64) -> f64 {
    (opinion * blend(evidence, influence) * 1000.0).round() / 10.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerdictCap {
    SingleAngle,
    WeakEvidence,
    NonProduction,
}

str_enum!(VerdictCap {
    SingleAngle => "single_angle",
    WeakEvidence => "weak_evidence",
    NonProduction => "non_production",
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

pub fn cap_for(
    angles: bool,
    evidence: &EvidenceStrength,
    thresholds: VerdictThresholds,
) -> Option<VerdictCap> {
    if evidence.score < thresholds.min_evidence {
        return Some(if evidence.only_non_production {
            VerdictCap::NonProduction
        } else {
            VerdictCap::WeakEvidence
        });
    }
    (!angles).then_some(VerdictCap::SingleAngle)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Judgement {
    pub opinion: Option<f64>,
    pub evidence: EvidenceStrength,
    pub confidence: Option<f64>,
    pub verdict: Option<Verdict>,
    pub cap: Option<VerdictCap>,
}

pub fn judge(
    agents: &[AgentOpinion],
    evidence: EvidenceStrength,
    thresholds: VerdictThresholds,
    settings: &EvidenceSettings,
) -> Judgement {
    let opinion = opinion(agents);
    let confidence =
        opinion.map(|opinion| confidence_percent(opinion, evidence.score, settings.influence));
    let mut cap = None;
    let verdict = confidence.map(|confidence| {
        let verdict = verdict_for(confidence, thresholds);
        if verdict != Verdict::Reliable {
            return verdict;
        }
        cap = cap_for(has_independent_angles(agents), &evidence, thresholds);
        if cap.is_some() {
            Verdict::ToCheck
        } else {
            verdict
        }
    });
    Judgement {
        opinion,
        evidence,
        confidence,
        verdict,
        cap,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeadScoring {
    pub opinion: Option<f64>,
    pub evidence: EvidenceStrength,
    pub cap: Option<VerdictCap>,
}

impl From<&Judgement> for LeadScoring {
    fn from(judgement: &Judgement) -> Self {
        Self {
            opinion: judgement.opinion,
            evidence: judgement.evidence,
            cap: judgement.cap,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    use super::*;
    use crate::domain::{
        NormalizedDomain,
        lead::{Lead, LeadId, LeadStatus, Score, Signal, SignalId},
        profile::ProfileId,
        values::{Provenance, SignalKind, SourceName, UnitInterval},
    };

    fn probabilities(forte: f64, moyenne: f64, faible: f64) -> Vec<(String, f64)> {
        vec![
            ("forte".to_owned(), forte),
            ("moyenne".to_owned(), moyenne),
            ("faible".to_owned(), faible),
        ]
    }

    fn lead() -> Lead {
        let at = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        Lead {
            id: LeadId(uuid::Uuid::nil()),
            profile_id: ProfileId(uuid::Uuid::nil()),
            domain: NormalizedDomain::from_host("luzernrisk.com").unwrap(),
            status: LeadStatus::New,
            score: Score::new(12.1).unwrap(),
            score_breakdown: Vec::new(),
            confidence: None,
            verdict: None,
            qualified_at: None,
            first_seen_at: at,
            last_seen_at: at,
            updated_at: at,
        }
    }

    fn signal(host: &str, source: SourceName) -> Signal {
        Signal {
            id: SignalId(uuid::Uuid::nil()),
            lead_id: LeadId(uuid::Uuid::nil()),
            kind: SignalKind::parse("saas_provider").unwrap(),
            value: Some("auth0".to_owned()),
            source,
            raw: json!({"host": host, "target": "x.auth0.com"}),
            provenance: Provenance::parse(&format!("https://{host}/")).unwrap(),
            strength: UnitInterval::new(0.5).unwrap(),
            observed_at: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
            agent: None,
        }
    }

    fn settings() -> EvidenceSettings {
        EvidenceSettings::default()
    }

    fn luzern_evidence() -> EvidenceStrength {
        let signals = [signal("login.dev.luzernrisk.com", SourceName::DnsCname)];
        let facts = LeadFacts::compute(&lead(), &signals, &[]);
        EvidenceStrength::compute(&facts, 12.1, &settings())
    }

    fn strong_evidence() -> EvidenceStrength {
        EvidenceStrength {
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
        }
    }

    fn agent(score: f64, axes: usize) -> AgentOpinion {
        AgentOpinion {
            weight: 5,
            score,
            axes,
        }
    }

    #[test]
    fn calibration_clamps_and_renormalises() {
        let calibrated = calibrate(&probabilities(1.0, 0.0, 0.0), 0.05);
        let total: f64 = calibrated.iter().map(|(_, p)| p).sum();
        assert!((total - 1.0).abs() < 1e-12);
        assert!((calibrated[0].1 - 0.95 / 1.05).abs() < 1e-12);
        assert!((calibrated[1].1 - 0.05 / 1.05).abs() < 1e-12);
        assert!(calibrated[0].1 < 0.91);
    }

    #[test]
    fn a_zero_floor_leaves_the_probabilities_untouched() {
        let calibrated = calibrate(&probabilities(1.0, 0.0, 0.0), 0.0);
        assert_eq!(calibrated, probabilities(1.0, 0.0, 0.0));
    }

    #[test]
    fn calibration_keeps_balanced_probabilities() {
        let calibrated = calibrate(&probabilities(0.5, 0.3, 0.2), 0.05);
        assert!((calibrated[0].1 - 0.5).abs() < 1e-12);
        assert!((calibrated[2].1 - 0.2).abs() < 1e-12);
    }

    #[test]
    fn the_opinion_is_the_weighted_mean_of_the_agent_scores() {
        let agents = [
            AgentOpinion {
                weight: 1,
                score: 1.0,
                axes: 1,
            },
            AgentOpinion {
                weight: 3,
                score: 0.0,
                axes: 1,
            },
        ];
        assert_eq!(opinion(&agents), Some(0.25));
        assert_eq!(opinion(&[]), None);
    }

    #[test]
    fn one_angle_needs_two_agents_or_two_axes() {
        assert!(!has_independent_angles(&[]));
        assert!(!has_independent_angles(&[agent(1.0, 1)]));
        assert!(has_independent_angles(&[agent(1.0, 2)]));
        assert!(has_independent_angles(&[agent(1.0, 1), agent(0.5, 1)]));
    }

    #[test]
    fn a_single_dev_cname_has_weak_evidence() {
        let evidence = luzern_evidence();
        assert!(evidence.only_non_production);
        assert!((evidence.components.volume - (1.0 - (-1.0f64 / 3.0).exp())).abs() < 1e-12);
        assert_eq!(evidence.components.diversity, 0.5);
        assert_eq!(evidence.components.environment, 0.0);
        assert!((evidence.components.strength - 0.242).abs() < 1e-12);
        let expected = ((1.0 - (-1.0f64 / 3.0).exp()) + 0.5 + 0.0 + 0.242) / 4.0;
        assert!((evidence.score - expected).abs() < 1e-12);
    }

    #[test]
    fn evidence_without_known_hosts_is_neutral_on_environment() {
        let facts = LeadFacts::compute(&lead(), &[], &[]);
        let evidence = EvidenceStrength::compute(&facts, 0.0, &settings());
        assert_eq!(evidence.components.environment, 0.5);
        assert_eq!(evidence.components.volume, 0.0);
        assert!(!evidence.only_non_production);
    }

    #[test]
    fn evidence_weights_and_reference_score_are_applied() {
        let signals = [signal("login.dev.luzernrisk.com", SourceName::DnsCname)];
        let facts = LeadFacts::compute(&lead(), &signals, &[]);
        let only_diversity = EvidenceSettings {
            weights: EvidenceWeights {
                volume: 0,
                diversity: 1,
                environment: 0,
                strength: 0,
            },
            ..settings()
        };
        assert_eq!(
            EvidenceStrength::compute(&facts, 12.1, &only_diversity).score,
            0.5
        );
        let low_reference = EvidenceSettings {
            reference_score: 10.0,
            ..settings()
        };
        assert_eq!(
            EvidenceStrength::compute(&facts, 12.1, &low_reference)
                .components
                .strength,
            1.0
        );
    }

    #[test]
    fn the_confidence_blends_the_opinion_with_the_evidence() {
        assert_eq!(confidence_percent(1.0, 1.0, 0.6), 100.0);
        assert_eq!(confidence_percent(1.0, 0.0, 0.6), 40.0);
        assert_eq!(confidence_percent(0.5, 0.5, 0.6), 35.0);
        assert_eq!(confidence_percent(0.8, 0.0, 0.0), 80.0);
        assert_eq!(confidence_percent(0.8, 0.3, 1.0), 24.0);
    }

    #[test]
    fn no_valid_agent_means_no_confidence_and_no_verdict() {
        let judged = judge(
            &[],
            strong_evidence(),
            VerdictThresholds::default(),
            &settings(),
        );
        assert_eq!(judged.confidence, None);
        assert_eq!(judged.verdict, None);
        assert_eq!(judged.cap, None);
    }

    #[test]
    fn thresholds_decide_the_verdict() {
        let thresholds = VerdictThresholds {
            reliable_from: 70,
            discard_below: 30,
            ..VerdictThresholds::default()
        };
        assert_eq!(verdict_for(70.0, thresholds), Verdict::Reliable);
        assert_eq!(verdict_for(69.9, thresholds), Verdict::ToCheck);
        assert_eq!(verdict_for(30.0, thresholds), Verdict::ToCheck);
        assert_eq!(verdict_for(29.9, thresholds), Verdict::Discarded);
    }

    #[test]
    fn two_angles_and_strong_evidence_make_a_lead_reliable() {
        let judged = judge(
            &[agent(0.95, 1), agent(0.9, 1)],
            strong_evidence(),
            VerdictThresholds::default(),
            &settings(),
        );
        assert_eq!(judged.verdict, Some(Verdict::Reliable));
        assert_eq!(judged.cap, None);
        assert!(judged.confidence.unwrap() >= 70.0);
    }

    #[test]
    fn one_agent_with_two_axes_is_enough_angles() {
        let judged = judge(
            &[agent(0.95, 2)],
            strong_evidence(),
            VerdictThresholds::default(),
            &settings(),
        );
        assert_eq!(judged.verdict, Some(Verdict::Reliable));
    }

    #[test]
    fn a_single_angle_caps_a_confident_lead_at_to_check() {
        let judged = judge(
            &[agent(0.95, 1)],
            strong_evidence(),
            VerdictThresholds::default(),
            &settings(),
        );
        assert!(judged.confidence.unwrap() >= 70.0);
        assert_eq!(judged.verdict, Some(Verdict::ToCheck));
        assert_eq!(judged.cap, Some(VerdictCap::SingleAngle));
    }

    #[test]
    fn weak_evidence_caps_a_confident_lead_at_to_check() {
        let weak = EvidenceStrength {
            score: 0.45,
            ..strong_evidence()
        };
        let thresholds = VerdictThresholds {
            reliable_from: 60,
            ..VerdictThresholds::default()
        };
        let judged = judge(
            &[agent(1.0, 1), agent(1.0, 1)],
            weak,
            thresholds,
            &settings(),
        );
        assert!(judged.confidence.unwrap() >= 60.0);
        assert_eq!(judged.verdict, Some(Verdict::ToCheck));
        assert_eq!(judged.cap, Some(VerdictCap::WeakEvidence));
    }

    #[test]
    fn only_non_production_hosts_are_named_as_the_cap() {
        let evidence = luzern_evidence();
        let thresholds = VerdictThresholds {
            reliable_from: 50,
            discard_below: 10,
            ..VerdictThresholds::default()
        };
        let judged = judge(
            &[agent(1.0, 1), agent(1.0, 1)],
            evidence,
            thresholds,
            &settings(),
        );
        assert_eq!(judged.verdict, Some(Verdict::ToCheck));
        assert_eq!(judged.cap, Some(VerdictCap::NonProduction));
    }

    #[test]
    fn a_low_confidence_is_discarded_without_a_cap() {
        let judged = judge(
            &[agent(0.1, 1)],
            strong_evidence(),
            VerdictThresholds::default(),
            &settings(),
        );
        assert_eq!(judged.verdict, Some(Verdict::Discarded));
        assert_eq!(judged.cap, None);
    }

    #[test]
    fn the_luzernrisk_lead_is_no_longer_reliable() {
        let evidence = luzern_evidence();
        let floor = VerdictThresholds::default().calibration_floor;
        let calibrated = calibrate(&probabilities(1.0, 0.0, 0.0), floor);
        let score = calibrated[0].1 * 1.0 + calibrated[1].1 * 0.5;
        let judged = judge(
            &[agent(score, 1)],
            evidence,
            VerdictThresholds::default(),
            &settings(),
        );
        let confidence = judged.confidence.unwrap();
        assert!(confidence < 55.0, "{confidence}");
        assert_eq!(judged.verdict, Some(Verdict::ToCheck));
        let uncapped = judge(
            &[agent(score, 1)],
            evidence,
            VerdictThresholds {
                reliable_from: 40,
                ..VerdictThresholds::default()
            },
            &settings(),
        );
        assert_eq!(uncapped.verdict, Some(Verdict::ToCheck));
        assert_eq!(uncapped.cap, Some(VerdictCap::NonProduction));
    }

    #[test]
    fn caps_round_trip_through_text() {
        for cap in [
            VerdictCap::SingleAngle,
            VerdictCap::WeakEvidence,
            VerdictCap::NonProduction,
        ] {
            assert_eq!(VerdictCap::parse(cap.as_str()), Some(cap));
        }
        assert_eq!(VerdictCap::parse("other"), None);
    }
}
