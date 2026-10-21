use std::collections::HashMap;

use crate::domain::{
    agent_work::ports::AgentStop,
    calibration::Calibrations,
    company::{CompanyDossier, CompanyEntity, SiblingLead},
    ecosystem::DomainEcosystem,
    enrichment::{EnrichmentState, EvaluationStatus},
    facts::LeadFacts,
    lead::{Assessment, ContactEmail, Lead, LeadId, LeadStatus, Signal},
    profile::{
        Profile, ProfileId, ProfileName, RevisionNumber,
        definition::{QualificationAgent, ScoringAgent},
    },
    qualification::{AgentFailure, LeadScoring, QualificationResult, SummaryState, Verdict},
    scoring_agent::ScoringResult,
    values::SignalKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeadSort {
    #[default]
    RelevanceDesc,
    ConfidenceDesc,
    ConfidenceAsc,
    ScoreDesc,
    ScoreAsc,
    LlmScoreDesc,
    LlmScoreAsc,
    LastSeenDesc,
    FirstSeenDesc,
    DomainAsc,
}

impl LeadSort {
    pub const ALL: [Self; 10] = [
        Self::RelevanceDesc,
        Self::ConfidenceDesc,
        Self::ConfidenceAsc,
        Self::ScoreDesc,
        Self::ScoreAsc,
        Self::LlmScoreDesc,
        Self::LlmScoreAsc,
        Self::LastSeenDesc,
        Self::FirstSeenDesc,
        Self::DomainAsc,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::RelevanceDesc => "relevance_desc",
            Self::ConfidenceDesc => "confidence_desc",
            Self::ConfidenceAsc => "confidence_asc",
            Self::ScoreDesc => "score_desc",
            Self::ScoreAsc => "score_asc",
            Self::LlmScoreDesc => "llm_score_desc",
            Self::LlmScoreAsc => "llm_score_asc",
            Self::LastSeenDesc => "last_seen_desc",
            Self::FirstSeenDesc => "first_seen_desc",
            Self::DomainAsc => "domain_asc",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|sort| sort.as_str() == raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerdictFilter {
    Reliable,
    ToCheck,
    Discarded,
    Pending,
}

impl VerdictFilter {
    pub const ALL: [Self; 4] = [
        Self::Reliable,
        Self::ToCheck,
        Self::Discarded,
        Self::Pending,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reliable => "fiable",
            Self::ToCheck => "a_verifier",
            Self::Discarded => "ecarte",
            Self::Pending => "en_attente",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|filter| filter.as_str() == raw)
    }

    pub fn stored(self) -> Option<Verdict> {
        match self {
            Self::Reliable => Some(Verdict::Reliable),
            Self::ToCheck => Some(Verdict::ToCheck),
            Self::Discarded => Some(Verdict::Discarded),
            Self::Pending => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LeadQuery {
    pub status: Option<LeadStatus>,
    pub verdicts: Vec<VerdictFilter>,
    pub min_confidence: Option<f64>,
    pub min_score: Option<f64>,
    pub domain_contains: Option<String>,
    pub signal_kind: Option<SignalKind>,
    pub group_by_company: bool,
    pub sort: LeadSort,
    pub limit: u32,
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeadPage {
    pub items: Vec<Lead>,
    pub total: u64,
    pub members: HashMap<LeadId, Vec<Lead>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeadListing {
    pub profile: Profile,
    pub page: LeadPage,
    pub summaries: HashMap<LeadId, String>,
    pub scorings: HashMap<LeadId, LeadScoring>,
    pub evaluations: HashMap<LeadId, EvaluationStatus>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeadDetail {
    pub lead: Lead,
    pub enrichment: EnrichmentState,
    pub evaluation: EvaluationStatus,
    pub ecosystem: DomainEcosystem,
    pub profile: Profile,
    pub signals: Vec<Signal>,
    pub facts: LeadFacts,
    pub contacts: Vec<ContactEmail>,
    pub assessments: Vec<Assessment>,
    pub qualification_results: Vec<QualificationResult>,
    pub qualification_agents: Vec<QualificationAgent>,
    pub scoring_results: Vec<ScoringResult>,
    pub scoring_agents: Vec<ScoringAgent>,
    pub calibrations: Calibrations,
    pub summary_state: Option<SummaryState>,
    pub agent_failures: Vec<AgentFailure>,
    pub agent_stops: Vec<AgentStop>,
    pub scoring: Option<LeadScoring>,
    pub company: Option<CompanyDossier>,
    pub entity: Option<CompanyEntity>,
    pub siblings: Vec<SiblingLead>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileSummary {
    pub id: ProfileId,
    pub name: ProfileName,
    pub active: bool,
    pub revision: Option<RevisionNumber>,
    pub lead_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_round_trip_through_text() {
        for sort in LeadSort::ALL {
            assert_eq!(LeadSort::parse(sort.as_str()), Some(sort));
        }
        assert_eq!(LeadSort::parse("random"), None);
        assert_eq!(LeadSort::default(), LeadSort::RelevanceDesc);
        assert_eq!(LeadSort::ALL[0], LeadSort::RelevanceDesc);
    }

    #[test]
    fn verdict_filters_round_trip_through_text() {
        for filter in VerdictFilter::ALL {
            assert_eq!(VerdictFilter::parse(filter.as_str()), Some(filter));
        }
        assert_eq!(VerdictFilter::parse("maybe"), None);
        assert_eq!(VerdictFilter::Pending.stored(), None);
        assert_eq!(VerdictFilter::Discarded.stored(), Some(Verdict::Discarded));
    }
}
