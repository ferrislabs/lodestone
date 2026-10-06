use chrono::{DateTime, Utc};
use common::CoreError;
use sqlx::PgPool;

use crate::{
    JobKind, JobSchedule, JobStatus, ProfileId, RunStatus, ScheduledJob, SourceName,
    SourceRunRecord, infrastructure::postgres::error::map_sqlx_error,
};

#[derive(Clone)]
pub struct PgJobSchedule {
    pool: PgPool,
}

impl PgJobSchedule {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct ScheduledRow {
    dedup_key: String,
    kind: String,
    status: String,
    run_at: DateTime<Utc>,
    attempts: i32,
    lease_until: Option<DateTime<Utc>>,
    last_error: Option<String>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<ScheduledRow> for ScheduledJob {
    type Error = CoreError;

    fn try_from(row: ScheduledRow) -> Result<Self, Self::Error> {
        Ok(Self {
            dedup_key: row.dedup_key,
            kind: JobKind::parse(&row.kind)
                .map_err(|_| CoreError::Internal("stored job kind is invalid".to_owned()))?,
            status: JobStatus::parse(&row.status)
                .ok_or_else(|| CoreError::Internal("stored job status is invalid".to_owned()))?,
            run_at: row.run_at,
            attempts: u16::try_from(row.attempts)
                .map_err(|_| CoreError::Internal("stored job attempts is invalid".to_owned()))?,
            lease_until: row.lease_until,
            last_error: row.last_error,
            updated_at: row.updated_at,
        })
    }
}

fn counter(value: i64) -> Result<u64, CoreError> {
    u64::try_from(value)
        .map_err(|_| CoreError::Internal("stored run counter is out of range".to_owned()))
}

impl JobSchedule for PgJobSchedule {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "jobs"), err)]
    async fn list(&self) -> Result<Vec<ScheduledJob>, CoreError> {
        let rows = sqlx::query_as!(
            ScheduledRow,
            r#"
            SELECT DISTINCT ON (dedup_key)
                dedup_key AS "dedup_key!", kind, status, run_at, attempts,
                lease_until, last_error, updated_at
            FROM jobs
            WHERE kind = 'source.run' AND dedup_key IS NOT NULL
            ORDER BY dedup_key, (status IN ('pending', 'running')) DESC, updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter().map(ScheduledJob::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "jobs"), err)]
    async fn find(&self, dedup_key: &str) -> Result<Option<ScheduledJob>, CoreError> {
        let row = sqlx::query_as!(
            ScheduledRow,
            r#"
            SELECT dedup_key AS "dedup_key!", kind, status, run_at, attempts,
                lease_until, last_error, updated_at
            FROM jobs
            WHERE dedup_key = $1 AND kind = 'source.run'
            ORDER BY (status IN ('pending', 'running')) DESC, updated_at DESC
            LIMIT 1
            "#,
            dedup_key,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        row.map(ScheduledJob::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "jobs"), err)]
    async fn run_now(
        &self,
        dedup_key: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<ScheduledJob>, CoreError> {
        let row = sqlx::query_as!(
            ScheduledRow,
            r#"
            UPDATE jobs
            SET run_at = LEAST(run_at, $2), updated_at = $2
            WHERE dedup_key = $1 AND status = 'pending' AND kind = 'source.run'
            RETURNING dedup_key AS "dedup_key!", kind, status, run_at, attempts,
                lease_until, last_error, updated_at
            "#,
            dedup_key,
            now,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        row.map(ScheduledJob::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "runs"), err)]
    async fn latest_runs(&self, per_source: u32) -> Result<Vec<SourceRunRecord>, CoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT source AS "source!", profile_id AS "profile_id!", profile_name AS "profile_name!",
                agent_id, status AS "status!", volume AS "volume!", signals AS "signals!",
                error, started_at AS "started_at!", finished_at
            FROM (
                SELECT r.source, r.profile_id, p.name AS profile_name, r.agent_id, r.status,
                    r.volume, r.signals, r.error, r.started_at, r.finished_at, r.id,
                    ROW_NUMBER() OVER (PARTITION BY r.source ORDER BY r.started_at DESC, r.id DESC) AS position
                FROM runs r
                JOIN profiles p ON p.id = r.profile_id
            ) ranked
            WHERE position <= $1
            ORDER BY source, started_at DESC, id DESC
            "#,
            i64::from(per_source),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(SourceRunRecord {
                    source: row.source.parse::<SourceName>().map_err(|_| {
                        CoreError::Internal("stored run source is invalid".to_owned())
                    })?,
                    profile_id: ProfileId(row.profile_id),
                    profile_name: row.profile_name,
                    agent_id: row.agent_id,
                    status: RunStatus::parse(&row.status).ok_or_else(|| {
                        CoreError::Internal("stored run status is invalid".to_owned())
                    })?,
                    volume: counter(row.volume)?,
                    signals: counter(row.signals)?,
                    error: row.error,
                    started_at: row.started_at,
                    finished_at: row.finished_at,
                })
            })
            .collect()
    }
}
