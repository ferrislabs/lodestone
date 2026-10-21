use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};

use crate::domain::{
    lead::LeadId,
    profile::{ProfileId, definition::VerdictThresholds},
    qualification::Verdict,
    quality::{DecisionKind, DecisionLabel, RejectionReason},
};

pub const MIN_LABELLED_LEADS: u64 = 20;
pub const BUCKET_COUNT: usize = 10;
const BUCKET_WIDTH: f64 = 10.0;
const THRESHOLD_STEP: u8 = 5;
const MAX_THRESHOLD: u8 = 100;
const MAX_VALIDATED_BELOW_DISCARD: u64 = 20;

fn tally(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}

fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let total: f64 = values.iter().sum();
    Some(total / f64::from(u32::try_from(values.len()).unwrap_or(u32::MAX)))
}

fn ratio(numerator: u64, denominator: u64) -> Option<f64> {
    if denominator == 0 {
        return None;
    }
    let numerator = f64::from(u32::try_from(numerator).unwrap_or(u32::MAX));
    let denominator = f64::from(u32::try_from(denominator).unwrap_or(u32::MAX));
    Some(numerator / denominator)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VerdictCounts {
    pub reliable: u64,
    pub to_check: u64,
    pub discarded: u64,
}

impl VerdictCounts {
    pub fn total(&self) -> u64 {
        self.reliable + self.to_check + self.discarded
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileInfo {
    pub id: ProfileId,
    pub name: String,
    pub thresholds: VerdictThresholds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecisionCounts {
    pub validated: u64,
    pub rejected: u64,
    pub drafts_approved: u64,
    pub drafts_rejected: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerdictPrecision {
    pub verdict: Option<Verdict>,
    pub validated: u64,
    pub rejected: u64,
    pub precision: Option<f64>,
}

impl VerdictPrecision {
    pub fn decided(&self) -> u64 {
        self.validated + self.rejected
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiscardedShare {
    pub discarded: u64,
    pub total: u64,
    pub share: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfidenceBucket {
    pub from: u8,
    pub to: u8,
    pub validated: u64,
    pub rejected: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentSeparation {
    pub agent_id: String,
    pub name: String,
    pub mean_validated: Option<f64>,
    pub mean_rejected: Option<f64>,
    pub separation: Option<f64>,
    pub validated: u64,
    pub rejected: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VersionPrecision {
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub model: String,
    pub prompt_version: String,
    pub escalated: bool,
    pub validated: u64,
    pub rejected: u64,
    pub precision: Option<f64>,
}

impl VersionPrecision {
    pub fn decided(&self) -> u64 {
        self.validated + self.rejected
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReasonCount {
    pub reason: RejectionReason,
    pub count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suggestion {
    Insufficient {
        labelled: u64,
        required: u64,
    },
    Ready {
        labelled: u64,
        reliable_from: Option<u8>,
        discard_below: Option<u8>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileQuality {
    pub profile_id: ProfileId,
    pub name: String,
    pub current: VerdictThresholds,
    pub validated: u64,
    pub rejected: u64,
    pub precision: Option<f64>,
    pub suggestion: Suggestion,
    pub reasons: Vec<ReasonCount>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExclusionCount {
    pub agent_id: String,
    pub agent_name: String,
    pub question_id: String,
    pub option: String,
    pub label: String,
    pub count: u64,
}

pub const EXCLUSION_REASONS: [RejectionReason; 4] = [
    RejectionReason::AlreadyCustomer,
    RejectionReason::Competitor,
    RejectionReason::VendorOrIntegrator,
    RejectionReason::HostOrAgency,
];

pub const MAX_EXCLUSION_CANDIDATES: usize = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExclusionTarget {
    pub lead_id: LeadId,
    pub reason: RejectionReason,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExclusionCandidate {
    pub lead_id: LeadId,
    pub domain: String,
    pub legal_name: Option<String>,
    pub reason: RejectionReason,
    pub decided_at: DateTime<Utc>,
}

pub fn exclusion_targets(labels: &[DecisionLabel]) -> Vec<ExclusionTarget> {
    let mut targets: Vec<ExclusionTarget> = latest_lead_decisions(labels)
        .into_iter()
        .filter(|label| !is_validated(label))
        .filter_map(|label| {
            let reason = label.rejection.as_ref()?.reason();
            EXCLUSION_REASONS
                .contains(&reason)
                .then_some(ExclusionTarget {
                    lead_id: label.lead_id,
                    reason,
                    decided_at: label.decided_at,
                })
        })
        .collect();
    targets.sort_by_key(|target| std::cmp::Reverse(target.decided_at));
    targets.truncate(MAX_EXCLUSION_CANDIDATES);
    targets
}

#[derive(Debug, Clone, PartialEq)]
pub struct QualityReport {
    pub decisions: DecisionCounts,
    pub verdicts: Vec<VerdictPrecision>,
    pub discarded: DiscardedShare,
    pub confidence: Vec<ConfidenceBucket>,
    pub agents: Vec<AgentSeparation>,
    pub versions: Vec<VersionPrecision>,
    pub profiles: Vec<ProfileQuality>,
    pub exclusions: Vec<ExclusionCount>,
    pub exclusion_candidates: Vec<ExclusionCandidate>,
}

fn latest_lead_decisions(labels: &[DecisionLabel]) -> Vec<&DecisionLabel> {
    let mut latest: HashMap<_, &DecisionLabel> = HashMap::new();
    for label in labels.iter().filter(|label| label.kind.is_lead()) {
        match latest.get(&label.lead_id) {
            Some(kept) if (kept.decided_at, kept.id) >= (label.decided_at, label.id) => {}
            _ => {
                latest.insert(label.lead_id, label);
            }
        }
    }
    let mut decisions: Vec<&DecisionLabel> = latest.into_values().collect();
    decisions.sort_by_key(|label| (label.decided_at, label.id));
    decisions
}

fn is_validated(label: &DecisionLabel) -> bool {
    label.kind == DecisionKind::LeadValidated
}

fn verdict_of(label: &DecisionLabel) -> Option<Verdict> {
    label.snapshot.verdict.as_deref().and_then(Verdict::parse)
}

fn verdict_precision(decisions: &[&DecisionLabel]) -> Vec<VerdictPrecision> {
    [
        Some(Verdict::Reliable),
        Some(Verdict::ToCheck),
        Some(Verdict::Discarded),
        None,
    ]
    .into_iter()
    .filter_map(|verdict| {
        let matching = decisions
            .iter()
            .filter(|label| verdict_of(label) == verdict);
        let (validated, rejected) = matching.fold((0, 0), |(validated, rejected), label| {
            if is_validated(label) {
                (validated + 1, rejected)
            } else {
                (validated, rejected + 1)
            }
        });
        let row = VerdictPrecision {
            verdict,
            validated,
            rejected,
            precision: ratio(validated, validated + rejected),
        };
        (verdict.is_some() || row.decided() > 0).then_some(row)
    })
    .collect()
}

fn confidence_histogram(decisions: &[&DecisionLabel]) -> Vec<ConfidenceBucket> {
    let mut buckets: Vec<ConfidenceBucket> = (0..BUCKET_COUNT)
        .map(|index| {
            let from = u8::try_from(index * 10).unwrap_or(u8::MAX);
            ConfidenceBucket {
                from,
                to: from.saturating_add(10),
                validated: 0,
                rejected: 0,
            }
        })
        .collect();
    for label in decisions {
        let Some(confidence) = label.snapshot.confidence.filter(|value| value.is_finite()) else {
            continue;
        };
        let index = bucket_index(confidence);
        if is_validated(label) {
            buckets[index].validated += 1;
        } else {
            buckets[index].rejected += 1;
        }
    }
    buckets
}

fn bucket_index(confidence: f64) -> usize {
    (1..BUCKET_COUNT)
        .filter(|boundary| {
            confidence >= BUCKET_WIDTH * f64::from(u8::try_from(*boundary).unwrap_or(u8::MAX))
        })
        .count()
}

fn agent_name(agent_names: &BTreeMap<String, String>, agent_id: &str) -> String {
    agent_names
        .get(agent_id)
        .cloned()
        .unwrap_or_else(|| agent_id.to_owned())
}

fn version_precision(
    decisions: &[&DecisionLabel],
    agent_names: &BTreeMap<String, String>,
) -> Vec<VersionPrecision> {
    type Key<'a> = (Option<&'a str>, &'a str, &'a str, bool);
    let mut tallies: BTreeMap<Key<'_>, (u64, u64)> = BTreeMap::new();
    for label in decisions {
        let validated = is_validated(label);
        let agents = label
            .snapshot
            .agents
            .iter()
            .filter(|(_, snapshot)| snapshot.status == "valid")
            .filter_map(|(agent, snapshot)| {
                snapshot
                    .provenance
                    .as_ref()
                    .map(|provenance| (Some(agent.as_str()), provenance, snapshot.escalated))
            });
        let aggregator = label
            .snapshot
            .aggregator
            .as_ref()
            .map(|provenance| (None, provenance, false));
        for (agent, provenance, escalated) in agents.chain(aggregator) {
            let entry = tallies
                .entry((
                    agent,
                    provenance.model.as_str(),
                    provenance.prompt_version.as_str(),
                    escalated,
                ))
                .or_default();
            if validated {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }
        }
    }
    tallies
        .into_iter()
        .map(
            |((agent, model, prompt_version, escalated), (validated, rejected))| VersionPrecision {
                agent_id: agent.map(str::to_owned),
                agent_name: agent.map(|agent| agent_name(agent_names, agent)),
                model: model.to_owned(),
                prompt_version: prompt_version.to_owned(),
                escalated,
                validated,
                rejected,
                precision: ratio(validated, validated + rejected),
            },
        )
        .collect()
}

fn rejection_reasons(decisions: &[&DecisionLabel]) -> Vec<ReasonCount> {
    RejectionReason::ALL
        .into_iter()
        .filter_map(|reason| {
            let count = tally(
                decisions
                    .iter()
                    .filter(|label| {
                        !is_validated(label)
                            && label
                                .rejection
                                .as_ref()
                                .is_some_and(|rejection| rejection.reason() == reason)
                    })
                    .count(),
            );
            (count > 0).then_some(ReasonCount { reason, count })
        })
        .collect()
}

fn agent_separation(
    decisions: &[&DecisionLabel],
    agent_names: &BTreeMap<String, String>,
) -> Vec<AgentSeparation> {
    let mut scores: BTreeMap<&str, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
    for label in decisions {
        for (agent, snapshot) in &label.snapshot.agents {
            let Some(score) = snapshot
                .score
                .filter(|_| snapshot.status == "valid")
                .filter(|score| score.is_finite())
            else {
                continue;
            };
            let entry = scores.entry(agent.as_str()).or_default();
            if is_validated(label) {
                entry.0.push(score);
            } else {
                entry.1.push(score);
            }
        }
    }
    scores
        .into_iter()
        .map(|(agent, (validated, rejected))| {
            let mean_validated = mean(&validated);
            let mean_rejected = mean(&rejected);
            AgentSeparation {
                agent_id: agent.to_owned(),
                name: agent_name(agent_names, agent),
                mean_validated,
                mean_rejected,
                separation: mean_validated.zip(mean_rejected).map(|(v, r)| v - r),
                validated: tally(validated.len()),
                rejected: tally(rejected.len()),
            }
        })
        .collect()
}

fn thresholds_grid() -> std::iter::StepBy<std::ops::RangeInclusive<u8>> {
    (0..=MAX_THRESHOLD).step_by(usize::from(THRESHOLD_STEP))
}

fn best_reliable_from(points: &[(f64, bool)]) -> Option<u8> {
    let validated = tally(points.iter().filter(|(_, validated)| *validated).count());
    if validated == 0 {
        return None;
    }
    let mut best: Option<(u8, u64, u64)> = None;
    for threshold in thresholds_grid() {
        let cut = f64::from(threshold);
        let mut true_positive = 0;
        let mut false_positive = 0;
        for (confidence, is_validated) in points {
            if *confidence >= cut {
                if *is_validated {
                    true_positive += 1;
                } else {
                    false_positive += 1;
                }
            }
        }
        let false_negative = validated - true_positive;
        let numerator = 2 * true_positive;
        let denominator = 2 * true_positive + false_positive + false_negative;
        let better = match best {
            None => true,
            Some((_, best_num, best_den)) => {
                u128::from(numerator) * u128::from(best_den)
                    >= u128::from(best_num) * u128::from(denominator)
            }
        };
        if better {
            best = Some((threshold, numerator, denominator));
        }
    }
    best.map(|(threshold, _, _)| threshold)
}

fn best_discard_below(points: &[(f64, bool)]) -> Option<u8> {
    thresholds_grid().rev().find(|threshold| {
        let cut = f64::from(*threshold);
        let below = points.iter().filter(|(confidence, _)| *confidence < cut);
        let (count, validated) =
            below.fold((0u64, 0u64), |(count, validated), (_, is_validated)| {
                (count + 1, validated + u64::from(*is_validated))
            });
        count > 0 && validated * MAX_VALIDATED_BELOW_DISCARD <= count
    })
}

impl Suggestion {
    pub fn compute(decisions: &[&DecisionLabel]) -> Self {
        let points: Vec<(f64, bool)> = decisions
            .iter()
            .filter_map(|label| {
                label
                    .snapshot
                    .confidence
                    .filter(|value| value.is_finite())
                    .map(|confidence| (confidence, is_validated(label)))
            })
            .collect();
        let labelled = tally(points.len());
        if labelled < MIN_LABELLED_LEADS {
            return Self::Insufficient {
                labelled,
                required: MIN_LABELLED_LEADS,
            };
        }
        let reliable_from = best_reliable_from(&points);
        let discard_below = best_discard_below(&points).map(|value| match reliable_from {
            Some(reliable) => value.min(reliable.saturating_sub(THRESHOLD_STEP)),
            None => value,
        });
        Self::Ready {
            labelled,
            reliable_from,
            discard_below,
        }
    }
}

fn profile_quality(
    info: &ProfileInfo,
    decisions: &[&DecisionLabel],
    selected: Option<ProfileId>,
) -> Option<ProfileQuality> {
    let own: Vec<&DecisionLabel> = decisions
        .iter()
        .copied()
        .filter(|label| label.profile_id == info.id)
        .collect();
    if own.is_empty() && selected != Some(info.id) {
        return None;
    }
    let validated = tally(own.iter().filter(|label| is_validated(label)).count());
    let rejected = tally(own.len()) - validated;
    Some(ProfileQuality {
        profile_id: info.id,
        name: info.name.clone(),
        current: info.thresholds,
        validated,
        rejected,
        precision: ratio(validated, validated + rejected),
        suggestion: Suggestion::compute(&own),
        reasons: rejection_reasons(&own),
    })
}

impl QualityReport {
    pub fn compute(
        labels: &[DecisionLabel],
        verdicts: VerdictCounts,
        profiles: &[ProfileInfo],
        selected: Option<ProfileId>,
        agent_names: &BTreeMap<String, String>,
    ) -> Self {
        let decisions = latest_lead_decisions(labels);
        let validated = tally(decisions.iter().filter(|label| is_validated(label)).count());
        let drafts = |kind| tally(labels.iter().filter(|label| label.kind == kind).count());
        Self {
            decisions: DecisionCounts {
                validated,
                rejected: tally(decisions.len()) - validated,
                drafts_approved: drafts(DecisionKind::DraftApproved),
                drafts_rejected: drafts(DecisionKind::DraftRejected),
            },
            verdicts: verdict_precision(&decisions),
            discarded: DiscardedShare {
                discarded: verdicts.discarded,
                total: verdicts.total(),
                share: ratio(verdicts.discarded, verdicts.total()),
            },
            confidence: confidence_histogram(&decisions),
            agents: agent_separation(&decisions, agent_names),
            versions: version_precision(&decisions, agent_names),
            profiles: profiles
                .iter()
                .filter_map(|info| profile_quality(info, &decisions, selected))
                .collect(),
            exclusions: Vec::new(),
            exclusion_candidates: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeZone, Utc};
    use uuid::Uuid;

    use super::*;
    use crate::domain::{
        lead::LeadId,
        llm::RunProvenance,
        profile::RevisionNumber,
        quality::{AgentSnapshot, DecisionSnapshot, Rejection},
    };

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
    }

    fn profile(n: u128) -> ProfileId {
        ProfileId(Uuid::from_u128(n))
    }

    fn provenance(model: &str, version: &str) -> RunProvenance {
        RunProvenance::legacy(model.to_owned(), version.to_owned())
    }

    struct Builder {
        label: DecisionLabel,
    }

    fn label(kind: DecisionKind, confidence: Option<f64>, verdict: Option<&str>) -> Builder {
        Builder {
            label: DecisionLabel {
                id: Uuid::now_v7(),
                lead_id: LeadId(Uuid::now_v7()),
                profile_id: profile(1),
                profile_revision: RevisionNumber::FIRST,
                kind,
                decided_by: None,
                decided_at: at(0),
                snapshot: DecisionSnapshot {
                    confidence,
                    opinion: None,
                    evidence: None,
                    verdict: verdict.map(str::to_owned),
                    verdict_reason: None,
                    agents: BTreeMap::new(),
                    aggregator: None,
                },
                rejection: None,
            },
        }
    }

    impl Builder {
        fn agent(mut self, id: &str, status: &str, score: Option<f64>) -> Self {
            self.label.snapshot.agents.insert(
                id.to_owned(),
                AgentSnapshot {
                    status: status.to_owned(),
                    score,
                    provenance: None,
                    escalated: false,
                },
            );
            self
        }

        fn versioned(mut self, id: &str, model: &str, version: &str) -> Self {
            self.label.snapshot.agents.insert(
                id.to_owned(),
                AgentSnapshot {
                    status: "valid".to_owned(),
                    score: Some(0.5),
                    provenance: Some(provenance(model, version)),
                    escalated: false,
                },
            );
            self
        }

        fn aggregated(mut self, model: &str, version: &str) -> Self {
            self.label.snapshot.aggregator = Some(provenance(model, version));
            self
        }

        fn because(mut self, reason: RejectionReason) -> Self {
            self.label.rejection = Some(Rejection::new(reason, Some("note")).unwrap());
            self
        }

        fn lead(mut self, lead: LeadId, seconds: i64) -> Self {
            self.label.lead_id = lead;
            self.label.decided_at = at(seconds);
            self
        }

        fn profile(mut self, id: ProfileId) -> Self {
            self.label.profile_id = id;
            self
        }

        fn done(self) -> DecisionLabel {
            self.label
        }
    }

    fn validated(confidence: f64, verdict: &str) -> Builder {
        label(DecisionKind::LeadValidated, Some(confidence), Some(verdict))
    }

    fn rejected(confidence: f64, verdict: &str) -> Builder {
        label(DecisionKind::LeadRejected, Some(confidence), Some(verdict))
    }

    fn info(thresholds: VerdictThresholds) -> ProfileInfo {
        ProfileInfo {
            id: profile(1),
            name: "alpha".to_owned(),
            thresholds,
        }
    }

    fn report(labels: &[DecisionLabel]) -> QualityReport {
        QualityReport::compute(
            labels,
            VerdictCounts::default(),
            &[info(VerdictThresholds::default())],
            None,
            &BTreeMap::new(),
        )
    }

    #[test]
    fn counts_and_precision_follow_lead_labels_and_draft_labels_stay_apart() {
        let labels = vec![
            validated(80.0, "fiable").done(),
            validated(75.0, "fiable").done(),
            rejected(72.0, "fiable").done(),
            validated(50.0, "a_verifier").done(),
            rejected(45.0, "a_verifier").done(),
            rejected(40.0, "a_verifier").done(),
            rejected(10.0, "ecarte").done(),
            label(DecisionKind::DraftApproved, Some(80.0), Some("fiable")).done(),
            label(DecisionKind::DraftApproved, Some(80.0), Some("fiable")).done(),
            label(DecisionKind::DraftRejected, Some(80.0), Some("fiable")).done(),
        ];
        let report = report(&labels);
        assert_eq!(
            report.decisions,
            DecisionCounts {
                validated: 3,
                rejected: 4,
                drafts_approved: 2,
                drafts_rejected: 1,
            }
        );
        let precision: Vec<(Option<Verdict>, u64, Option<f64>)> = report
            .verdicts
            .iter()
            .map(|row| (row.verdict, row.decided(), row.precision))
            .collect();
        assert_eq!(precision.len(), 3);
        assert_eq!(precision[0].0, Some(Verdict::Reliable));
        assert_eq!(precision[0].1, 3);
        assert!((precision[0].2.unwrap() - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(precision[1].1, 3);
        assert!((precision[1].2.unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(precision[2], (Some(Verdict::Discarded), 1, Some(0.0)));
    }

    #[test]
    fn a_verdict_without_decisions_has_no_precision_and_pending_appears_only_when_used() {
        let none = report(&[]);
        assert_eq!(none.verdicts.len(), 3);
        assert!(none.verdicts.iter().all(|row| row.precision.is_none()));
        let pending = report(&[label(DecisionKind::LeadValidated, None, None).done()]);
        assert_eq!(pending.verdicts.len(), 4);
        assert_eq!(pending.verdicts[3].verdict, None);
        assert_eq!(pending.verdicts[3].validated, 1);
    }

    #[test]
    fn only_the_latest_decision_of_a_lead_counts() {
        let lead = LeadId(Uuid::now_v7());
        let labels = vec![
            validated(80.0, "fiable").lead(lead, 10).done(),
            rejected(80.0, "fiable").lead(lead, 20).done(),
        ];
        let report = report(&labels);
        assert_eq!(report.decisions.validated, 0);
        assert_eq!(report.decisions.rejected, 1);
    }

    #[test]
    fn the_share_of_discarded_leads_uses_the_verdict_totals() {
        let counts = VerdictCounts {
            reliable: 2,
            to_check: 3,
            discarded: 5,
        };
        let report = QualityReport::compute(&[], counts, &[], None, &BTreeMap::new());
        assert_eq!(report.discarded.discarded, 5);
        assert_eq!(report.discarded.total, 10);
        assert_eq!(report.discarded.share, Some(0.5));
        let empty =
            QualityReport::compute(&[], VerdictCounts::default(), &[], None, &BTreeMap::new());
        assert_eq!(empty.discarded.share, None);
    }

    #[test]
    fn agents_are_separated_by_the_mean_score_of_validated_versus_rejected() {
        let labels = vec![
            validated(80.0, "fiable")
                .agent("fit", "valid", Some(0.9))
                .agent("market", "valid", Some(0.5))
                .done(),
            validated(80.0, "fiable")
                .agent("fit", "valid", Some(0.7))
                .agent("market", "invalid", None)
                .done(),
            rejected(30.0, "a_verifier")
                .agent("fit", "valid", Some(0.2))
                .agent("market", "valid", Some(0.4))
                .done(),
        ];
        let report = report(&labels);
        let fit = &report.agents[0];
        assert_eq!(fit.agent_id, "fit");
        assert!((fit.mean_validated.unwrap() - 0.8).abs() < 1e-9);
        assert!((fit.mean_rejected.unwrap() - 0.2).abs() < 1e-9);
        assert!((fit.separation.unwrap() - 0.6).abs() < 1e-9);
        assert_eq!((fit.validated, fit.rejected), (2, 1));
        let market = &report.agents[1];
        assert_eq!((market.validated, market.rejected), (1, 1));
        assert!((market.separation.unwrap() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn an_agent_seen_on_one_side_only_has_no_separation() {
        let labels = vec![
            validated(80.0, "fiable")
                .agent("fit", "valid", Some(0.9))
                .done(),
        ];
        let agent = &report(&labels).agents[0];
        assert_eq!(agent.mean_rejected, None);
        assert_eq!(agent.separation, None);
        assert_eq!((agent.validated, agent.rejected), (1, 0));
    }

    #[test]
    fn the_histogram_has_ten_buckets_and_puts_one_hundred_in_the_last() {
        let labels = vec![
            validated(0.0, "ecarte").done(),
            validated(9.9, "ecarte").done(),
            validated(10.0, "ecarte").done(),
            rejected(55.0, "a_verifier").done(),
            rejected(99.9, "fiable").done(),
            validated(100.0, "fiable").done(),
            label(DecisionKind::LeadRejected, None, None).done(),
        ];
        let buckets = report(&labels).confidence;
        assert_eq!(buckets.len(), 10);
        assert_eq!((buckets[0].from, buckets[0].to), (0, 10));
        assert_eq!((buckets[9].from, buckets[9].to), (90, 100));
        assert_eq!(buckets[0].validated, 2);
        assert_eq!(buckets[1].validated, 1);
        assert_eq!(buckets[5].rejected, 1);
        assert_eq!(buckets[9].rejected, 1);
        assert_eq!(buckets[9].validated, 1);
        let total: u64 = buckets.iter().map(|b| b.validated + b.rejected).sum();
        assert_eq!(total, 6);
    }

    fn separable(count: usize) -> Vec<DecisionLabel> {
        let mut labels = Vec::new();
        for index in 0..count {
            let offset = f64::from(u32::try_from(index).unwrap());
            labels.push(validated(70.0 + offset, "fiable").done());
            labels.push(rejected(20.0 + offset, "a_verifier").done());
        }
        labels
    }

    #[test]
    fn suggestions_need_twenty_labelled_leads() {
        let report = report(&separable(9));
        assert_eq!(
            report.profiles[0].suggestion,
            Suggestion::Insufficient {
                labelled: 18,
                required: 20
            }
        );
        let enough = super::QualityReport::compute(
            &separable(10),
            VerdictCounts::default(),
            &[info(VerdictThresholds::default())],
            None,
            &BTreeMap::new(),
        );
        assert!(matches!(
            enough.profiles[0].suggestion,
            Suggestion::Ready { labelled: 20, .. }
        ));
    }

    #[test]
    fn unlabelled_confidences_do_not_count_toward_the_minimum() {
        let mut labels = separable(9);
        labels.push(label(DecisionKind::LeadValidated, None, None).done());
        labels.push(label(DecisionKind::LeadRejected, None, None).done());
        assert_eq!(
            report(&labels).profiles[0].suggestion,
            Suggestion::Insufficient {
                labelled: 18,
                required: 20
            }
        );
    }

    #[test]
    fn the_reliable_threshold_maximises_f1_and_the_discard_threshold_spares_validated_leads() {
        let report = report(&separable(10));
        let Suggestion::Ready {
            reliable_from,
            discard_below,
            ..
        } = report.profiles[0].suggestion
        else {
            panic!("expected a suggestion");
        };
        assert_eq!(reliable_from, Some(70));
        assert_eq!(discard_below, Some(65));
    }

    #[test]
    fn a_noisy_set_balances_precision_and_recall_in_the_reliable_threshold() {
        let mut labels = Vec::new();
        for confidence in [95.0, 90.0, 85.0, 80.0, 75.0, 66.0, 64.0, 62.0] {
            labels.push(validated(confidence, "fiable").done());
        }
        for confidence in [
            68.0, 40.0, 38.0, 35.0, 30.0, 25.0, 20.0, 15.0, 10.0, 5.0, 2.0, 1.0,
        ] {
            labels.push(rejected(confidence, "a_verifier").done());
        }
        let Suggestion::Ready { reliable_from, .. } = report(&labels).profiles[0].suggestion else {
            panic!("expected a suggestion");
        };
        assert_eq!(reliable_from, Some(60));
    }

    #[test]
    fn a_set_without_validated_leads_suggests_no_reliable_threshold() {
        let labels: Vec<DecisionLabel> = (0..20)
            .map(|index| rejected(f64::from(index) * 4.0, "a_verifier").done())
            .collect();
        let Suggestion::Ready {
            reliable_from,
            discard_below,
            ..
        } = report(&labels).profiles[0].suggestion
        else {
            panic!("expected a suggestion");
        };
        assert_eq!(reliable_from, None);
        assert_eq!(discard_below, Some(100));
    }

    #[test]
    fn the_discard_threshold_never_reaches_the_reliable_one() {
        let mut labels = Vec::new();
        for index in 0..20 {
            labels.push(rejected(f64::from(index), "ecarte").done());
        }
        for index in 0..5 {
            labels.push(validated(60.0 + f64::from(index), "fiable").done());
        }
        let Suggestion::Ready {
            reliable_from,
            discard_below,
            ..
        } = report(&labels).profiles[0].suggestion
        else {
            panic!("expected a suggestion");
        };
        assert_eq!(reliable_from, Some(60));
        assert_eq!(discard_below, Some(55));
    }

    #[test]
    fn profiles_without_decisions_appear_only_when_selected() {
        let other = ProfileInfo {
            id: profile(2),
            name: "beta".to_owned(),
            thresholds: VerdictThresholds::default(),
        };
        let labels = vec![validated(80.0, "fiable").profile(profile(2)).done()];
        let profiles = [info(VerdictThresholds::default()), other];
        let unselected = QualityReport::compute(
            &labels,
            VerdictCounts::default(),
            &profiles,
            None,
            &BTreeMap::new(),
        );
        assert_eq!(unselected.profiles.len(), 1);
        assert_eq!(unselected.profiles[0].name, "beta");
        assert_eq!(unselected.profiles[0].precision, Some(1.0));
        let selected = QualityReport::compute(
            &[],
            VerdictCounts::default(),
            &profiles[..1],
            Some(profile(1)),
            &BTreeMap::new(),
        );
        assert_eq!(selected.profiles.len(), 1);
        assert_eq!(
            selected.profiles[0].suggestion,
            Suggestion::Insufficient {
                labelled: 0,
                required: 20
            }
        );
    }

    #[test]
    fn precision_is_split_by_agent_model_and_prompt_version() {
        let labels = vec![
            validated(80.0, "fiable")
                .versioned("fit", "qwen3", "qualification-agent@a")
                .aggregated("qwen3", "aggregator@x")
                .done(),
            validated(80.0, "fiable")
                .versioned("fit", "qwen3", "qualification-agent@a")
                .aggregated("mistral", "aggregator@x")
                .done(),
            rejected(30.0, "a_verifier")
                .versioned("fit", "qwen3", "qualification-agent@a")
                .aggregated("mistral", "aggregator@x")
                .done(),
            rejected(30.0, "a_verifier")
                .versioned("fit", "qwen3", "qualification-agent@b")
                .done(),
            validated(80.0, "fiable").done(),
        ];
        let names = BTreeMap::from([("fit".to_owned(), "Adéquation".to_owned())]);
        let report = QualityReport::compute(
            &labels,
            VerdictCounts::default(),
            &[info(VerdictThresholds::default())],
            None,
            &names,
        );
        let rows: Vec<(Option<&str>, &str, &str, u64, u64)> = report
            .versions
            .iter()
            .map(|row| {
                (
                    row.agent_id.as_deref(),
                    row.model.as_str(),
                    row.prompt_version.as_str(),
                    row.validated,
                    row.rejected,
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                (None, "mistral", "aggregator@x", 1, 1),
                (None, "qwen3", "aggregator@x", 1, 0),
                (Some("fit"), "qwen3", "qualification-agent@a", 2, 1),
                (Some("fit"), "qwen3", "qualification-agent@b", 0, 1),
            ]
        );
        let fit = &report.versions[2];
        assert_eq!(fit.agent_name.as_deref(), Some("Adéquation"));
        assert!((fit.precision.unwrap() - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(fit.decided(), 3);
        assert_eq!(report.versions[0].agent_name, None);
    }

    #[test]
    fn escalated_runs_are_counted_apart_from_the_original_ones() {
        let mut escalated =
            rejected(30.0, "a_verifier").versioned("fit", "qwen3", "qualification-agent@a");
        escalated
            .label
            .snapshot
            .agents
            .get_mut("fit")
            .unwrap()
            .escalated = true;
        let labels = vec![
            validated(80.0, "fiable")
                .versioned("fit", "qwen3", "qualification-agent@a")
                .done(),
            escalated.done(),
        ];
        let report = QualityReport::compute(
            &labels,
            VerdictCounts::default(),
            &[info(VerdictThresholds::default())],
            None,
            &BTreeMap::new(),
        );
        let rows: Vec<(bool, u64, u64)> = report
            .versions
            .iter()
            .map(|row| (row.escalated, row.validated, row.rejected))
            .collect();
        assert_eq!(rows, vec![(false, 1, 0), (true, 0, 1)]);
    }

    #[test]
    fn invalid_agent_results_and_legacy_labels_carry_no_version() {
        let mut invalid = validated(80.0, "fiable");
        invalid.label.snapshot.agents.insert(
            "fit".to_owned(),
            AgentSnapshot {
                status: "invalid".to_owned(),
                score: None,
                provenance: Some(provenance("qwen3", "qualification-agent@a")),
                escalated: false,
            },
        );
        let labels = vec![
            invalid.done(),
            validated(80.0, "fiable")
                .agent("fit", "valid", Some(0.5))
                .done(),
        ];
        assert!(report(&labels).versions.is_empty());
    }

    #[test]
    fn only_rejections_for_an_exclusion_reason_are_candidates_for_the_lists() {
        let revised = LeadId(Uuid::now_v7());
        let labels = vec![
            rejected(30.0, "a_verifier")
                .because(RejectionReason::Competitor)
                .done(),
            rejected(30.0, "a_verifier")
                .because(RejectionReason::OffTarget)
                .done(),
            rejected(30.0, "a_verifier").done(),
            rejected(30.0, "a_verifier")
                .because(RejectionReason::AlreadyCustomer)
                .lead(revised, 10)
                .done(),
            validated(80.0, "fiable").lead(revised, 20).done(),
            rejected(30.0, "a_verifier")
                .because(RejectionReason::HostOrAgency)
                .done(),
        ];
        let reasons: Vec<RejectionReason> = exclusion_targets(&labels)
            .iter()
            .map(|target| target.reason)
            .collect();
        assert_eq!(reasons.len(), 2);
        assert!(reasons.contains(&RejectionReason::Competitor));
        assert!(reasons.contains(&RejectionReason::HostOrAgency));
        assert!(!reasons.contains(&RejectionReason::AlreadyCustomer));
    }

    #[test]
    fn the_candidates_are_the_most_recent_and_bounded() {
        let labels: Vec<DecisionLabel> = (0..40)
            .map(|index| {
                rejected(30.0, "a_verifier")
                    .because(RejectionReason::Competitor)
                    .lead(LeadId(Uuid::now_v7()), index)
                    .done()
            })
            .collect();
        let targets = exclusion_targets(&labels);
        assert_eq!(targets.len(), MAX_EXCLUSION_CANDIDATES);
        assert!(targets[0].decided_at > targets[1].decided_at);
        assert_eq!(targets[0].decided_at, at(39));
    }

    #[test]
    fn agent_names_fall_back_to_the_id() {
        let labels = vec![
            validated(80.0, "fiable")
                .agent("fit", "valid", Some(0.9))
                .agent("market", "valid", Some(0.5))
                .done(),
        ];
        let names = BTreeMap::from([("fit".to_owned(), "Adéquation".to_owned())]);
        let report = QualityReport::compute(
            &labels,
            VerdictCounts::default(),
            &[info(VerdictThresholds::default())],
            None,
            &names,
        );
        assert_eq!(report.agents[0].name, "Adéquation");
        assert_eq!(report.agents[1].name, "market");
    }

    #[test]
    fn rejection_reasons_are_counted_per_profile_for_rejected_leads_only() {
        let other = ProfileInfo {
            id: profile(2),
            name: "beta".to_owned(),
            thresholds: VerdictThresholds::default(),
        };
        let lead = LeadId(Uuid::now_v7());
        let labels = vec![
            rejected(30.0, "a_verifier")
                .because(RejectionReason::OffTarget)
                .done(),
            rejected(30.0, "a_verifier")
                .because(RejectionReason::OffTarget)
                .done(),
            rejected(30.0, "a_verifier")
                .because(RejectionReason::Competitor)
                .done(),
            rejected(30.0, "a_verifier").done(),
            rejected(30.0, "a_verifier")
                .because(RejectionReason::BadData)
                .profile(profile(2))
                .done(),
            rejected(30.0, "a_verifier")
                .because(RejectionReason::Other)
                .lead(lead, 10)
                .done(),
            validated(30.0, "a_verifier").lead(lead, 20).done(),
        ];
        let report = QualityReport::compute(
            &labels,
            VerdictCounts::default(),
            &[info(VerdictThresholds::default()), other],
            None,
            &BTreeMap::new(),
        );
        assert_eq!(
            report.profiles[0].reasons,
            vec![
                ReasonCount {
                    reason: RejectionReason::OffTarget,
                    count: 2
                },
                ReasonCount {
                    reason: RejectionReason::Competitor,
                    count: 1
                },
            ]
        );
        assert_eq!(
            report.profiles[1].reasons,
            vec![ReasonCount {
                reason: RejectionReason::BadData,
                count: 1
            }]
        );
    }
}
