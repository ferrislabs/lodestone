use chrono::{DateTime, Utc};

use crate::domain::{
    agent_work::CancelReason,
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
    Cancelled,
}

str_enum!(RunStatus {
    Running => "running",
    Succeeded => "succeeded",
    Failed => "failed",
    Cancelled => "cancelled",
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
    Cancelled {
        volume: u64,
        signals: u64,
        reason: CancelReason,
    },
}

impl RunOutcome {
    pub fn status(&self) -> RunStatus {
        match self {
            Self::Succeeded { .. } => RunStatus::Succeeded,
            Self::Failed { .. } => RunStatus::Failed,
            Self::Cancelled { .. } => RunStatus::Cancelled,
        }
    }

    pub fn cancel_reason(&self) -> Option<CancelReason> {
        match self {
            Self::Cancelled { reason, .. } => Some(*reason),
            Self::Succeeded { .. } | Self::Failed { .. } => None,
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
    pub cancel_reason: Option<CancelReason>,
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
            cancel_reason: None,
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
        let cancelled = RunOutcome::Cancelled {
            volume: 2,
            signals: 1,
            reason: CancelReason::AgentRemoved,
        };
        assert_eq!(cancelled.status(), RunStatus::Cancelled);
        assert_eq!(cancelled.cancel_reason(), Some(CancelReason::AgentRemoved));
    }

    #[test]
    fn statuses_round_trip_through_text() {
        for status in [
            RunStatus::Running,
            RunStatus::Succeeded,
            RunStatus::Failed,
            RunStatus::Cancelled,
        ] {
            assert_eq!(RunStatus::parse(status.as_str()), Some(status));
        }
    }
}
