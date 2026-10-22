use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    UserId,
    domain::{
        lead::{Assessment, Lead, LeadId, LeadStatus, str_enum},
        llm::RunProvenance,
        profile::{ProfileId, RevisionNumber},
        qualification::{EvidenceStrength, LeadScoring, QualificationResult},
        scoring_agent::ScoringResult,
    },
};

pub mod metrics;
pub mod ports;
pub mod rejection;
pub mod sourcing;

pub use metrics::{
    AgentSeparation, ConfidenceBucket, DecisionCounts, DiscardedShare, EXCLUSION_REASONS,
    ExclusionCandidate, ExclusionCount, ExclusionTarget, MAX_EXCLUSION_CANDIDATES,
    MIN_LABELLED_LEADS, ProfileInfo, ProfileQuality, QualityReport, ReasonCount, Suggestion,
    VerdictCounts, VerdictPrecision, VersionKind, VersionPrecision, exclusion_targets,
};
pub use rejection::{MAX_NOTE_CHARS, Rejection, RejectionProblem, RejectionReason};
pub use sourcing::{
    AgentRules, AgentYield, EditProblem, KeywordSuggestion, KeywordYield,
    MAX_EXCLUSION_SUGGESTIONS_PER_AGENT, MIN_DECIDED_PER_KEYWORD, MIN_EXCLUSION_WORD_CHARS,
    MIN_REJECTED_WITH_WORD, SourcingRow, SourcingYield, SuggestionAction, SuggestionEdit,
    YieldVerdicts,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecisionKind {
    LeadValidated,
    LeadRejected,
    DraftApproved,
    DraftRejected,
}

str_enum!(DecisionKind {
    LeadValidated => "lead_validated",
    LeadRejected => "lead_rejected",
    DraftApproved => "draft_approved",
    DraftRejected => "draft_rejected",
});

impl DecisionKind {
    pub fn for_lead(status: LeadStatus) -> Option<Self> {
        match status {
            LeadStatus::Validated => Some(Self::LeadValidated),
            LeadStatus::Rejected => Some(Self::LeadRejected),
            _ => None,
        }
    }

    pub fn is_approval(self) -> bool {
        matches!(self, Self::LeadValidated | Self::DraftApproved)
    }

    pub fn is_lead(self) -> bool {
        matches!(self, Self::LeadValidated | Self::LeadRejected)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub status: String,
    pub score: Option<f64>,
    #[serde(default)]
    pub provenance: Option<RunProvenance>,
    #[serde(default)]
    pub escalated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoringSnapshot {
    pub status: String,
    pub score: Option<u8>,
    pub verdict: Option<String>,
    pub confidence: Option<f64>,
    #[serde(default)]
    pub provenance: Option<RunProvenance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionSnapshot {
    pub confidence: Option<f64>,
    #[serde(default)]
    pub indicator_confidence: Option<f64>,
    #[serde(default)]
    pub llm_score: Option<u8>,
    #[serde(default)]
    pub verdict_source: Option<String>,
    pub opinion: Option<f64>,
    pub evidence: Option<EvidenceStrength>,
    pub verdict: Option<String>,
    pub verdict_reason: Option<String>,
    pub agents: BTreeMap<String, AgentSnapshot>,
    #[serde(default)]
    pub scoring_agents: BTreeMap<String, ScoringSnapshot>,
    #[serde(default)]
    pub aggregator: Option<RunProvenance>,
}

impl DecisionSnapshot {
    pub fn capture(
        lead: &Lead,
        scoring: Option<&LeadScoring>,
        results: &[QualificationResult],
        scoring_results: &[ScoringResult],
        assessment: Option<&Assessment>,
    ) -> Self {
        Self {
            confidence: lead.confidence,
            indicator_confidence: scoring.and_then(|scoring| scoring.indicator_confidence),
            llm_score: lead.llm_score,
            verdict_source: scoring.map(|scoring| scoring.verdict_source.as_str().to_owned()),
            opinion: scoring.and_then(|scoring| scoring.opinion),
            evidence: scoring.map(|scoring| scoring.evidence),
            verdict: lead.verdict.map(|verdict| verdict.as_str().to_owned()),
            verdict_reason: scoring
                .and_then(|scoring| scoring.reason)
                .map(|reason| reason.as_str().to_owned()),
            agents: results
                .iter()
                .map(|result| {
                    (
                        result.agent_id.as_str().to_owned(),
                        AgentSnapshot {
                            status: result.status.as_str().to_owned(),
                            score: result.agent_score,
                            provenance: Some(result.provenance.clone()),
                            escalated: result.escalated,
                        },
                    )
                })
                .collect(),
            scoring_agents: scoring_results
                .iter()
                .map(|result| {
                    (
                        result.agent_id.as_str().to_owned(),
                        ScoringSnapshot {
                            status: result.status.as_str().to_owned(),
                            score: result.score,
                            verdict: result
                                .verdict
                                .as_ref()
                                .map(|verdict| verdict.choice.as_str().to_owned()),
                            confidence: result.verdict.as_ref().map(|verdict| verdict.confidence),
                            provenance: Some(result.provenance.clone()),
                        },
                    )
                })
                .collect(),
            aggregator: assessment.map(|assessment| assessment.provenance.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionLabel {
    pub id: Uuid,
    pub lead_id: LeadId,
    pub profile_id: ProfileId,
    pub profile_revision: RevisionNumber,
    pub kind: DecisionKind,
    pub decided_by: Option<UserId>,
    pub decided_at: DateTime<Utc>,
    pub snapshot: DecisionSnapshot,
    pub rejection: Option<Rejection>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LabelFilter {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub profile_id: Option<ProfileId>,
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::domain::{
        NormalizedDomain,
        lead::Score,
        profile::definition::AgentId,
        qualification::{
            EvidenceComponents, ResultId, ResultStatus, Verdict, VerdictCap, VerdictReason,
            VerdictSource,
        },
        scoring_agent::VerdictProbabilities,
    };

    fn lead() -> Lead {
        let at = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        Lead {
            id: LeadId(Uuid::nil()),
            profile_id: ProfileId(Uuid::nil()),
            domain: NormalizedDomain::from_host("example.com").unwrap(),
            status: LeadStatus::Qualified,
            score: Score::new(1.0).unwrap(),
            confidence: Some(72.5),
            verdict: Some(Verdict::ToCheck),
            llm_score: Some(68),
            qualified_at: Some(at),
            first_seen_at: at,
            last_seen_at: at,
            updated_at: at,
        }
    }

    fn evidence() -> EvidenceStrength {
        EvidenceStrength {
            score: 0.4,
            components: EvidenceComponents {
                volume: 0.5,
                diversity: 0.5,
                environment: 0.5,
                strength: 0.2,
            },
            signal_count: 2,
            signal_score: 4.0,
            only_non_production: false,
            independent_signals: 2,
            decayed_signals: 2.0,
            source_count: 1,
            kind_count: 1,
        }
    }

    fn result(agent: &str, status: ResultStatus, score: Option<f64>) -> QualificationResult {
        QualificationResult {
            id: ResultId(Uuid::nil()),
            lead_id: LeadId(Uuid::nil()),
            profile_id: ProfileId(Uuid::nil()),
            profile_revision: RevisionNumber::FIRST,
            agent_id: AgentId::new(agent),
            provenance: RunProvenance::legacy("m".to_owned(), "h".to_owned()),
            status,
            summary: None,
            answers: serde_json::Value::Null,
            agent_score: score,
            error: None,
            escalated: false,
            raw_samples: serde_json::json!([]),
            created_at: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
        }
    }

    #[test]
    fn only_validation_and_rejection_are_decisions() {
        assert_eq!(
            DecisionKind::for_lead(LeadStatus::Validated),
            Some(DecisionKind::LeadValidated)
        );
        assert_eq!(
            DecisionKind::for_lead(LeadStatus::Rejected),
            Some(DecisionKind::LeadRejected)
        );
        for status in [
            LeadStatus::New,
            LeadStatus::Qualified,
            LeadStatus::Contacted,
            LeadStatus::Uncertain,
        ] {
            assert_eq!(DecisionKind::for_lead(status), None);
        }
        assert!(DecisionKind::LeadRejected.is_lead());
        assert!(!DecisionKind::DraftApproved.is_lead());
        assert_eq!(
            DecisionKind::parse("draft_rejected"),
            Some(DecisionKind::DraftRejected)
        );
    }

    #[test]
    fn the_snapshot_keeps_the_state_at_decision_time() {
        let scoring = LeadScoring {
            opinion: Some(0.8),
            evidence: evidence(),
            indicator_confidence: Some(41.0),
            cap: Some(VerdictCap::WeakEvidence),
            reason: Some(VerdictReason::Capped(VerdictCap::WeakEvidence)),
            exclusions: Vec::new(),
            verdict_source: VerdictSource::Indicators,
            probabilities: None,
        };
        let results = [
            result("fit", ResultStatus::Valid, Some(0.9)),
            result("market", ResultStatus::Invalid, None),
        ];
        let snapshot = DecisionSnapshot::capture(&lead(), Some(&scoring), &results, &[], None);
        assert_eq!(snapshot.confidence, Some(72.5));
        assert_eq!(snapshot.opinion, Some(0.8));
        assert_eq!(snapshot.evidence, Some(evidence()));
        assert_eq!(snapshot.verdict.as_deref(), Some("a_verifier"));
        assert_eq!(snapshot.verdict_reason.as_deref(), Some("weak_evidence"));
        assert_eq!(snapshot.agents["fit"].status, "valid");
        assert_eq!(snapshot.agents["fit"].score, Some(0.9));
        assert_eq!(snapshot.agents["market"].status, "invalid");
        assert_eq!(snapshot.agents["market"].score, None);
    }

    #[test]
    fn a_lead_without_qualification_has_an_empty_snapshot() {
        let mut unqualified = lead();
        unqualified.confidence = None;
        unqualified.verdict = None;
        let snapshot = DecisionSnapshot::capture(&unqualified, None, &[], &[], None);
        assert_eq!(snapshot.confidence, None);
        assert_eq!(snapshot.evidence, None);
        assert_eq!(snapshot.verdict, None);
        assert!(snapshot.agents.is_empty());
    }

    #[test]
    fn the_snapshot_survives_a_json_round_trip() {
        let scoring = LeadScoring {
            opinion: Some(0.8),
            evidence: evidence(),
            indicator_confidence: None,
            cap: None,
            reason: None,
            exclusions: Vec::new(),
            verdict_source: VerdictSource::Scoring,
            probabilities: Some(VerdictProbabilities::new(0.2, 0.7, 0.1)),
        };
        let snapshot = DecisionSnapshot::capture(&lead(), Some(&scoring), &[], &[], None);
        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(
            serde_json::from_value::<DecisionSnapshot>(json).unwrap(),
            snapshot
        );
    }

    fn assessment() -> Assessment {
        Assessment {
            id: crate::domain::lead::AssessmentId(Uuid::nil()),
            lead_id: LeadId(Uuid::nil()),
            profile_revision: RevisionNumber::FIRST,
            summary: String::new(),
            verdict_narrative: String::new(),
            approach_angle: String::new(),
            evidence: Vec::new(),
            evidence_verified: true,
            quality_warnings: Vec::new(),
            provenance: RunProvenance::legacy("big".to_owned(), "aggregator@z".to_owned()),
            draft: None,
            draft_review: None,
            created_at: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
        }
    }

    #[test]
    fn the_snapshot_keeps_the_provenance_of_each_agent_and_of_the_aggregator() {
        let results = [result("fit", ResultStatus::Valid, Some(0.9))];
        let snapshot = DecisionSnapshot::capture(&lead(), None, &results, &[], Some(&assessment()));
        assert_eq!(
            snapshot.agents["fit"].provenance,
            Some(RunProvenance::legacy("m".to_owned(), "h".to_owned()))
        );
        assert_eq!(
            snapshot.aggregator.as_ref().map(|p| p.model.as_str()),
            Some("big")
        );
        let without = DecisionSnapshot::capture(&lead(), None, &results, &[], None);
        assert_eq!(without.aggregator, None);
    }

    #[test]
    fn a_snapshot_stored_before_provenance_still_reads() {
        let json = serde_json::json!({
            "confidence": 50.0,
            "opinion": null,
            "evidence": null,
            "verdict": "fiable",
            "verdict_reason": null,
            "agents": {"fit": {"status": "valid", "score": 0.5}},
        });
        let snapshot: DecisionSnapshot = serde_json::from_value(json).unwrap();
        assert_eq!(snapshot.agents["fit"].provenance, None);
        assert_eq!(snapshot.aggregator, None);
    }
}
