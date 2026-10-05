use chrono::{DateTime, Utc};

use crate::domain::{
    lead::{str_enum, uuid_id},
    profile::{ProfileId, definition::AgentId},
    values::SourceName,
};

pub mod ports;

uuid_id!(RunId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunStatus {
    Running,
    Succeeded,
    Failed,
}

str_enum!(RunStatus {
    Running => "running",
    Succeeded => "succeeded",
    Failed => "failed",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Succeeded {
        volume: u64,
        signals: u64,
    },
    Failed {
        volume: u64,
        signals: u64,
        error: String,
    },
}

impl RunOutcome {
    pub fn status(&self) -> RunStatus {
        match self {
            Self::Succeeded { .. } => RunStatus::Succeeded,
            Self::Failed { .. } => RunStatus::Failed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub id: RunId,
    pub profile_id: ProfileId,
    pub source: SourceName,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub volume: u64,
    pub signals: u64,
    pub error: Option<String>,
    pub agent: Option<AgentId>,
}

impl Run {
    pub fn start(
        id: RunId,
        profile_id: ProfileId,
        source: SourceName,
        agent: Option<AgentId>,
        at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            profile_id,
            source,
            agent,
            status: RunStatus::Running,
            started_at: at,
            finished_at: None,
            volume: 0,
            signals: 0,
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_maps_to_status() {
        assert_eq!(
            RunOutcome::Succeeded {
                volume: 1,
                signals: 0
            }
            .status(),
            RunStatus::Succeeded
        );
        assert_eq!(
            RunOutcome::Failed {
                volume: 0,
                signals: 0,
                error: "timeout".into()
            }
            .status(),
            RunStatus::Failed
        );
    }

    #[test]
    fn statuses_round_trip_through_text() {
        for status in [RunStatus::Running, RunStatus::Succeeded, RunStatus::Failed] {
            assert_eq!(RunStatus::parse(status.as_str()), Some(status));
        }
    }
}
