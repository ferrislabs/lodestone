use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    job::{JobKind, JobStatus},
    profile::ProfileId,
    run::RunStatus,
    values::SourceName,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledJob {
    pub dedup_key: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub run_at: DateTime<Utc>,
    pub attempts: u16,
    pub lease_until: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRunRecord {
    pub source: SourceName,
    pub profile_id: ProfileId,
    pub profile_name: String,
    pub agent_id: Option<String>,
    pub status: RunStatus,
    pub volume: u64,
    pub signals: u64,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSchedule {
    pub jobs: Vec<ScheduledJob>,
    pub runs: Vec<SourceRunRecord>,
}

pub trait JobSchedule: Send + Sync {
    fn list(&self) -> impl Future<Output = Result<Vec<ScheduledJob>, CoreError>> + Send;

    fn find(
        &self,
        dedup_key: &str,
    ) -> impl Future<Output = Result<Option<ScheduledJob>, CoreError>> + Send;

    fn run_now(
        &self,
        dedup_key: &str,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Option<ScheduledJob>, CoreError>> + Send;

    fn latest_runs(
        &self,
        per_source: u32,
    ) -> impl Future<Output = Result<Vec<SourceRunRecord>, CoreError>> + Send;
}
