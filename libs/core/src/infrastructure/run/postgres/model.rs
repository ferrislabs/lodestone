use chrono::{DateTime, Utc};
use common::CoreError;
use uuid::Uuid;

use crate::{CancelReason, ProfileId, Run, RunId, RunStatus, SourceName, definition::AgentId};

#[derive(Debug, Clone)]
pub struct RunRow {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub source: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub volume: i64,
    pub signals: i64,
    pub error: Option<String>,
    pub agent_id: Option<String>,
    pub cancel_reason: Option<String>,
}

pub fn to_i64(value: u64) -> Result<i64, CoreError> {
    i64::try_from(value).map_err(|_| CoreError::Internal("run counter out of range".to_owned()))
}

fn to_u64(value: i64) -> Result<u64, CoreError> {
    u64::try_from(value)
        .map_err(|_| CoreError::Internal("stored run counter is out of range".to_owned()))
}

impl TryFrom<RunRow> for Run {
    type Error = CoreError;

    fn try_from(row: RunRow) -> Result<Self, Self::Error> {
        let source = row
            .source
            .parse::<SourceName>()
            .map_err(|_| CoreError::Internal("stored run source is invalid".to_owned()))?;
        let status = RunStatus::parse(&row.status)
            .ok_or_else(|| CoreError::Internal("stored run status is invalid".to_owned()))?;
        Ok(Self {
            id: RunId(row.id),
            profile_id: ProfileId(row.profile_id),
            source,
            status,
            started_at: row.started_at,
            finished_at: row.finished_at,
            volume: to_u64(row.volume)?,
            signals: to_u64(row.signals)?,
            error: row.error,
            agent: row.agent_id.map(AgentId::new).filter(AgentId::is_slug),
            cancel_reason: row.cancel_reason.as_deref().and_then(CancelReason::parse),
        })
    }
}
