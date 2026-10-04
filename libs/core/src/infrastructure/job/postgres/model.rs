use std::num::NonZeroU16;

use chrono::{DateTime, Utc};
use common::CoreError;
use uuid::Uuid;

use crate::{ClaimedJob, Job, JobId, JobKind, JobStatus, LeaseToken};

#[derive(Debug, Clone)]
pub struct JobRow {
    pub id: Uuid,
    pub kind: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub run_at: DateTime<Utc>,
    pub lease_token: Option<Uuid>,
    pub lease_until: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<JobRow> for Job {
    type Error = CoreError;

    fn try_from(row: JobRow) -> Result<Self, Self::Error> {
        let kind = JobKind::parse(&row.kind)
            .map_err(|_| CoreError::Internal("stored job kind is invalid".to_owned()))?;
        let status = JobStatus::parse(&row.status)
            .ok_or_else(|| CoreError::Internal("stored job status is invalid".to_owned()))?;
        let attempts = u16::try_from(row.attempts)
            .map_err(|_| CoreError::Internal("stored job attempts is invalid".to_owned()))?;
        let max_attempts = u16::try_from(row.max_attempts)
            .ok()
            .and_then(NonZeroU16::new)
            .ok_or_else(|| CoreError::Internal("stored job max attempts is invalid".to_owned()))?;
        Ok(Self {
            id: JobId(row.id),
            kind,
            payload: row.payload,
            status,
            attempts,
            max_attempts,
            run_at: row.run_at,
            last_error: row.last_error,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

impl TryFrom<JobRow> for ClaimedJob {
    type Error = CoreError;

    fn try_from(row: JobRow) -> Result<Self, Self::Error> {
        let (Some(token), Some(lease_until)) = (row.lease_token, row.lease_until) else {
            return Err(CoreError::Internal(
                "stored job lease is missing".to_owned(),
            ));
        };
        Ok(Self {
            job: Job::try_from(row)?,
            token: LeaseToken(token),
            lease_until,
        })
    }
}
