use crate::domain::{
    lead::{Assessment, ContactEmail, Lead, LeadStatus, Signal},
    profile::{Profile, ProfileId, ProfileName, RevisionNumber},
    values::SignalKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeadSort {
    #[default]
    ScoreDesc,
    ScoreAsc,
    LastSeenDesc,
    FirstSeenDesc,
    DomainAsc,
}

impl LeadSort {
    pub const ALL: [Self; 5] = [
        Self::ScoreDesc,
        Self::ScoreAsc,
        Self::LastSeenDesc,
        Self::FirstSeenDesc,
        Self::DomainAsc,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ScoreDesc => "score_desc",
            Self::ScoreAsc => "score_asc",
            Self::LastSeenDesc => "last_seen_desc",
            Self::FirstSeenDesc => "first_seen_desc",
            Self::DomainAsc => "domain_asc",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|sort| sort.as_str() == raw)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LeadQuery {
    pub status: Option<LeadStatus>,
    pub min_score: Option<f64>,
    pub domain_contains: Option<String>,
    pub signal_kind: Option<SignalKind>,
    pub sort: LeadSort,
    pub limit: u32,
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeadPage {
    pub items: Vec<Lead>,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeadListing {
    pub profile: Profile,
    pub page: LeadPage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeadDetail {
    pub lead: Lead,
    pub profile: Profile,
    pub signals: Vec<Signal>,
    pub contacts: Vec<ContactEmail>,
    pub assessments: Vec<Assessment>,
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
        assert_eq!(LeadSort::default(), LeadSort::ScoreDesc);
    }
}
