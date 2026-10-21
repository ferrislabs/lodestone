use std::str::FromStr;

use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::domain::{
    NormalizedDomain,
    profile::{ProfileId, RevisionNumber, definition::AgentId},
    prompt::DraftMessage,
    values::{EmailAddress, Provenance, SignalKind, SourceName, UnitInterval},
};

pub mod draft;
pub mod ports;
pub mod query;

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(pub ::uuid::Uuid);

        impl ::std::str::FromStr for $name {
            type Err = ::uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                <::uuid::Uuid as ::std::str::FromStr>::from_str(s).map($name)
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

pub(crate) use uuid_id;

uuid_id!(LeadId);
uuid_id!(SignalId);
uuid_id!(ContactId);
uuid_id!(AssessmentId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeadStatus {
    New,
    Qualified,
    Validated,
    Rejected,
    Contacted,
    Uncertain,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("lead cannot move from {from:?} to {to:?}")]
pub struct InvalidTransition {
    pub from: LeadStatus,
    pub to: LeadStatus,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("unknown lead status `{0}`")]
pub struct UnknownLeadStatus(pub String);

impl LeadStatus {
    pub const ALL: [Self; 6] = [
        Self::New,
        Self::Qualified,
        Self::Validated,
        Self::Rejected,
        Self::Contacted,
        Self::Uncertain,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Qualified => "qualified",
            Self::Validated => "validated",
            Self::Rejected => "rejected",
            Self::Contacted => "contacted",
            Self::Uncertain => "uncertain",
        }
    }

    pub fn can_move_to(self, to: Self) -> bool {
        use LeadStatus::{Contacted, New, Qualified, Rejected, Uncertain, Validated};
        matches!(
            (self, to),
            (New, Qualified)
                | (New, Rejected)
                | (Qualified, Validated)
                | (Qualified, Rejected)
                | (Qualified, New)
                | (Validated, Contacted)
                | (Validated, Rejected)
                | (Rejected, New)
                | (New, Uncertain)
                | (Uncertain, New)
                | (Uncertain, Rejected)
        )
    }

    pub fn move_to(self, to: Self) -> Result<Self, InvalidTransition> {
        if self.can_move_to(to) {
            Ok(to)
        } else {
            Err(InvalidTransition { from: self, to })
        }
    }
}

impl FromStr for LeadStatus {
    type Err = UnknownLeadStatus;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == raw)
            .ok_or_else(|| UnknownLeadStatus(raw.to_owned()))
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("score must be a finite number")]
pub struct InvalidScore;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Score(f64);

impl Score {
    pub fn new(value: f64) -> Result<Self, InvalidScore> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(InvalidScore)
        }
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lead {
    pub id: LeadId,
    pub profile_id: ProfileId,
    pub domain: NormalizedDomain,
    pub status: LeadStatus,
    pub score: Score,
    pub score_breakdown: Vec<crate::domain::scoring::ScoreContribution>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub id: SignalId,
    pub lead_id: LeadId,
    pub kind: SignalKind,
    pub value: Option<String>,
    pub source: SourceName,
    pub raw: serde_json::Value,
    pub provenance: Provenance,
    pub strength: UnitInterval,
    pub observed_at: DateTime<Utc>,
    pub agent: Option<AgentId>,
}

const FINGERPRINT_SEPARATOR: char = '\u{1f}';

impl Signal {
    pub fn fingerprint(&self) -> String {
        let mut fingerprint = String::with_capacity(
            self.kind.as_str().len()
                + self.value.as_deref().map_or(0, str::len)
                + self.provenance.as_str().len()
                + 32,
        );
        fingerprint.push_str(self.kind.as_str());
        fingerprint.push(FINGERPRINT_SEPARATOR);
        fingerprint.push_str(self.value.as_deref().unwrap_or_default());
        fingerprint.push(FINGERPRINT_SEPARATOR);
        fingerprint.push_str(self.source.as_str());
        fingerprint.push(FINGERPRINT_SEPARATOR);
        fingerprint.push_str(self.provenance.as_str());
        fingerprint
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContactMethod {
    PublicPage,
    SecurityTxt,
    RoleGuess,
    LlmExtraction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContactKind {
    Role,
    Nominative,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContactEmail {
    pub id: ContactId,
    pub lead_id: LeadId,
    pub address: EmailAddress,
    pub provenance: Provenance,
    pub method: ContactMethod,
    pub confidence: UnitInterval,
    pub kind: ContactKind,
    pub found_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    pub quote: String,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assessment {
    pub id: AssessmentId,
    pub lead_id: LeadId,
    pub profile_revision: RevisionNumber,
    pub summary: String,
    pub approach_angle: String,
    pub evidence: Vec<Evidence>,
    pub model: String,
    pub prompt_version: String,
    pub draft: Option<DraftMessage>,
    pub draft_review: Option<draft::DraftReview>,
    pub created_at: DateTime<Utc>,
}

macro_rules! str_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        impl $name {
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            pub fn parse(raw: &str) -> Option<Self> {
                match raw {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

pub(crate) use str_enum;

str_enum!(ContactMethod {
    PublicPage => "public_page",
    SecurityTxt => "security_txt",
    RoleGuess => "role_guess",
    LlmExtraction => "llm_extraction",
});

str_enum!(ContactKind {
    Role => "role",
    Nominative => "nominative",
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_transitions() {
        use LeadStatus::*;
        let allowed = [
            (New, Qualified),
            (New, Rejected),
            (Qualified, Validated),
            (Qualified, Rejected),
            (Qualified, New),
            (Validated, Contacted),
            (Validated, Rejected),
            (Rejected, New),
            (New, Uncertain),
            (Uncertain, New),
            (Uncertain, Rejected),
        ];
        for from in LeadStatus::ALL {
            for to in LeadStatus::ALL {
                assert_eq!(
                    from.can_move_to(to),
                    allowed.contains(&(from, to)),
                    "{from:?} -> {to:?}"
                );
            }
        }
    }

    #[test]
    fn contacted_is_terminal() {
        for to in LeadStatus::ALL {
            assert!(LeadStatus::Contacted.move_to(to).is_err());
        }
    }

    #[test]
    fn a_lead_cannot_be_contacted_before_validation() {
        assert_eq!(
            LeadStatus::Qualified.move_to(LeadStatus::Contacted),
            Err(InvalidTransition {
                from: LeadStatus::Qualified,
                to: LeadStatus::Contacted
            })
        );
    }

    #[test]
    fn statuses_round_trip_through_text() {
        for status in LeadStatus::ALL {
            assert_eq!(status.as_str().parse::<LeadStatus>(), Ok(status));
        }
        assert!("archived".parse::<LeadStatus>().is_err());
    }

    #[test]
    fn score_rejects_non_finite_values() {
        assert!(Score::new(12.5).is_ok());
        assert_eq!(Score::new(f64::INFINITY), Err(InvalidScore));
    }

    #[test]
    fn contact_enums_round_trip_through_text() {
        for method in [
            ContactMethod::PublicPage,
            ContactMethod::SecurityTxt,
            ContactMethod::RoleGuess,
            ContactMethod::LlmExtraction,
        ] {
            assert_eq!(ContactMethod::parse(method.as_str()), Some(method));
        }
        for kind in [ContactKind::Role, ContactKind::Nominative] {
            assert_eq!(ContactKind::parse(kind.as_str()), Some(kind));
        }
    }
}
