use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{Exclusion, Verdict};
use crate::domain::{
    facts::LeadFacts,
    lead::str_enum,
    profile::definition::{EvidenceSettings, EvidenceWeights, VerdictThresholds},
};

pub const SCORING_VERSION: i32 = 3;

const VOLUME_SCALE: f64 = 3.0;
const SOURCES_FOR_FULL_DIVERSITY: f64 = 2.0;
const EXTRA_KIND_WEIGHT: f64 = 0.5;
const SECONDS_PER_DAY: f64 = 86_400.0;
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
    #[serde(default)]
    pub independent_signals: usize,
    #[serde(default)]
    pub decayed_signals: f64,
    #[serde(default)]
    pub source_count: usize,
    #[serde(default)]
    pub kind_count: usize,
}

pub fn decay(age_days: f64, half_life_days: f64) -> f64 {
    0.5_f64.powf(age_days.max(0.0) / half_life_days)
}

fn age_days(observed_at: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
    (now - observed_at)
        .to_std()
        .map_or(0.0, |age| age.as_secs_f64() / SECONDS_PER_DAY)
}

pub fn diversity(sources: usize, kinds: usize) -> f64 {
    ((count(sources) + EXTRA_KIND_WEIGHT * count(kinds.saturating_sub(1)))
        / SOURCES_FOR_FULL_DIVERSITY)
        .min(1.0)
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
    pub fn compute(
        facts: &LeadFacts,
        signal_score: f64,
        settings: &EvidenceSettings,
        now: DateTime<Utc>,
    ) -> Self {
        let decayed: f64 = facts
            .independent
            .iter()
            .map(|signal| {
                decay(
                    age_days(signal.observed_at, now),
                    f64::from(settings.half_life_days_for(signal.kind.as_str())),
                )
            })
            .sum();
        let independent = facts.independent_signals();
        let freshness = if independent == 0 {
            1.0
        } else {
            decayed / count(independent)
        };
        let components = EvidenceComponents {
            volume: 1.0 - (-decayed / VOLUME_SCALE).exp(),
            diversity: diversity(facts.sources.len(), facts.kinds.len()),
            environment: facts.production_share().unwrap_or(UNKNOWN_ENVIRONMENT),
            strength: (signal_score * freshness / settings.reference_score).clamp(0.0, 1.0),
        };
        Self {
            score: weighted_mean(&components, &settings.weights).clamp(0.0, 1.0),
            components,
            signal_count: facts.signal_count,
            signal_score,
            only_non_production: facts.only_non_production(),
            independent_signals: independent,
            decayed_signals: decayed,
            source_count: facts.sources.len(),
            kind_count: facts.kinds.len(),
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

#[derive(Debug, Clone, PartialEq)]
pub struct Judgement {
    pub opinion: Option<f64>,
    pub evidence: EvidenceStrength,
    pub confidence: Option<f64>,
    pub verdict: Option<Verdict>,
    pub cap: Option<VerdictCap>,
    pub exclusions: Vec<Exclusion>,
}

impl Judgement {
    #[must_use]
    pub fn excluding(mut self, exclusions: Vec<Exclusion>) -> Self {
        if !exclusions.is_empty() {
            self.verdict = Some(Verdict::Discarded);
            self.cap = None;
            self.exclusions = exclusions;
        }
        self
    }

    pub fn reason(&self) -> Option<VerdictReason> {
        if self.exclusions.is_empty() {
            self.cap.map(VerdictReason::Capped)
        } else {
            Some(VerdictReason::Excluded)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictReason {
    Capped(VerdictCap),
    Excluded,
}

pub const EXCLUDED_REASON: &str = "excluded";

impl VerdictReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Capped(cap) => cap.as_str(),
            Self::Excluded => EXCLUDED_REASON,
        }
    }
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
        exclusions: Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeadScoring {
    pub opinion: Option<f64>,
    pub evidence: EvidenceStrength,
    pub cap: Option<VerdictCap>,
    pub exclusions: Vec<Exclusion>,
}

impl LeadScoring {
    pub fn reason(&self) -> Option<VerdictReason> {
        if self.exclusions.is_empty() {
            self.cap.map(VerdictReason::Capped)
        } else {
            Some(VerdictReason::Excluded)
        }
    }
}

impl From<&Judgement> for LeadScoring {
    fn from(judgement: &Judgement) -> Self {
        Self {
            opinion: judgement.opinion,
            evidence: judgement.evidence,
            cap: judgement.cap,
            exclusions: judgement.exclusions.clone(),
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

    fn now() -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000, 0).unwrap()
    }

    fn luzern_evidence() -> EvidenceStrength {
        let signals = [signal("login.dev.luzernrisk.com", SourceName::DnsCname)];
        let facts = LeadFacts::compute(&lead(), &signals, &[]);
        EvidenceStrength::compute(&facts, 12.1, &settings(), now())
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
            independent_signals: 6,
            decayed_signals: 6.0,
            source_count: 2,
            kind_count: 2,
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
        let evidence = EvidenceStrength::compute(&facts, 0.0, &settings(), now());
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
            EvidenceStrength::compute(&facts, 12.1, &only_diversity, now()).score,
            0.5
        );
        let low_reference = EvidenceSettings {
            reference_score: 10.0,
            ..settings()
        };
        assert_eq!(
            EvidenceStrength::compute(&facts, 12.1, &low_reference, now())
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

    fn dated(kind: &str, value: &str, source: SourceName, age_days: i64) -> Signal {
        Signal {
            kind: SignalKind::parse(kind).unwrap(),
            value: Some(value.to_owned()),
            observed_at: now() - chrono::Duration::days(age_days),
            ..signal("sso.acme.com", source)
        }
    }

    fn evidence_of(signals: &[Signal], score: f64) -> EvidenceStrength {
        let facts = LeadFacts::compute(&lead(), signals, &[]);
        EvidenceStrength::compute(&facts, score, &settings(), now())
    }

    #[test]
    fn identical_kind_value_and_source_count_once_in_the_volume() {
        let repeated = [
            dated("saas_provider", "auth0", SourceName::DnsCname, 0),
            dated("saas_provider", " Auth0 ", SourceName::DnsCname, 0),
            dated("saas_provider", "auth0", SourceName::DnsCname, 0),
        ];
        let evidence = evidence_of(&repeated, 0.0);
        assert_eq!(evidence.signal_count, 3);
        assert_eq!(evidence.independent_signals, 1);
        assert!((evidence.decayed_signals - 1.0).abs() < 1e-12);
        assert!((evidence.components.volume - (1.0 - (-1.0f64 / 3.0).exp())).abs() < 1e-12);
    }

    #[test]
    fn another_value_kind_or_source_is_independent() {
        let signals = [
            dated("saas_provider", "auth0", SourceName::DnsCname, 0),
            dated("saas_provider", "okta", SourceName::DnsCname, 0),
            dated("product_detected", "auth0", SourceName::DnsCname, 0),
            dated("saas_provider", "auth0", SourceName::HttpProbe, 0),
        ];
        assert_eq!(evidence_of(&signals, 0.0).independent_signals, 4);
    }

    #[test]
    fn the_freshest_duplicate_sets_the_age_of_an_independent_signal() {
        let signals = [
            dated("job_offer", "devops", SourceName::Ats, 90),
            dated("job_offer", "devops", SourceName::Ats, 0),
        ];
        let evidence = evidence_of(&signals, 0.0);
        assert_eq!(evidence.independent_signals, 1);
        assert!((evidence.decayed_signals - 1.0).abs() < 1e-12);
    }

    #[test]
    fn diversity_counts_sources_and_extra_kinds() {
        assert_eq!(diversity(0, 0), 0.0);
        assert_eq!(diversity(1, 1), 0.5);
        assert_eq!(diversity(1, 2), 0.75);
        assert_eq!(diversity(1, 3), 1.0);
        assert_eq!(diversity(2, 1), 1.0);
        assert_eq!(diversity(3, 5), 1.0);
        let signals = [
            dated("saas_provider", "auth0", SourceName::DnsCname, 0),
            dated("product_detected", "keycloak", SourceName::DnsCname, 0),
        ];
        let evidence = evidence_of(&signals, 0.0);
        assert_eq!(evidence.source_count, 1);
        assert_eq!(evidence.kind_count, 2);
        assert_eq!(evidence.components.diversity, 0.75);
    }

    #[test]
    fn a_signal_of_today_does_not_decay() {
        let evidence = evidence_of(&[dated("job_offer", "a", SourceName::Ats, 0)], 25.0);
        assert!((evidence.decayed_signals - 1.0).abs() < 1e-12);
        assert!((evidence.components.strength - 0.5).abs() < 1e-12);
    }

    #[test]
    fn a_signal_of_one_half_life_counts_for_half() {
        let evidence = evidence_of(&[dated("job_offer", "a", SourceName::Ats, 90)], 25.0);
        assert!((evidence.decayed_signals - 0.5).abs() < 1e-9);
        assert!((evidence.components.volume - (1.0 - (-0.5f64 / 3.0).exp())).abs() < 1e-9);
        assert!((evidence.components.strength - 0.25).abs() < 1e-9);
    }

    #[test]
    fn the_half_life_depends_on_the_signal_kind() {
        let at = |kind: &str, days: i64| {
            evidence_of(&[dated(kind, "a", SourceName::Ats, days)], 0.0).decayed_signals
        };
        assert!((at("job_offer", 90) - 0.5).abs() < 1e-9);
        assert!((at("public_tender", 180) - 0.5).abs() < 1e-9);
        assert!((at("awarded_contract", 365) - 0.5).abs() < 1e-9);
        assert!((at("saas_provider", 365) - 0.5).abs() < 1e-9);
        assert!((at("product_detected", 365) - 0.5).abs() < 1e-9);
        assert!((at("debt_marker", 365) - 0.5).abs() < 1e-9);
        assert!((at("contact", 365) - 0.5).abs() < 1e-9);
        assert!((at("something_new", 180) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn the_profile_half_lives_override_the_defaults() {
        let mut custom = settings();
        custom.half_life_days.insert("job_offer".to_owned(), 30);
        custom.half_life_days.insert("default".to_owned(), 60);
        let signals = [
            dated("job_offer", "a", SourceName::Ats, 30),
            dated("something_new", "b", SourceName::Ats, 60),
        ];
        let facts = LeadFacts::compute(&lead(), &signals, &[]);
        let evidence = EvidenceStrength::compute(&facts, 0.0, &custom, now());
        assert!((evidence.decayed_signals - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_signal_from_the_future_does_not_grow_the_volume() {
        let evidence = evidence_of(&[dated("job_offer", "a", SourceName::Ats, -10)], 0.0);
        assert!((evidence.decayed_signals - 1.0).abs() < 1e-12);
    }

    #[test]
    fn stored_evidence_without_the_newer_fields_still_reads() {
        let stored = json!({
            "score": 0.5,
            "components": {"volume": 0.5, "diversity": 0.5, "environment": 0.5, "strength": 0.5},
            "signal_count": 2,
            "signal_score": 10.0,
            "only_non_production": false
        });
        let evidence: EvidenceStrength = serde_json::from_value(stored).unwrap();
        assert_eq!(evidence.independent_signals, 0);
        assert_eq!(evidence.signal_count, 2);
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
