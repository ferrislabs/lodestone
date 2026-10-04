use std::time::Duration;

use chrono::{DateTime, Utc};
use common::CoreError;
use sqlx::PgPool;

use crate::{
    ClaimedJob, Job, JobFailure, JobId, JobQueue, LeaseToken, NewJob,
    infrastructure::{job::postgres::model::JobRow, postgres::error::map_sqlx_error},
    new_lease_token,
};

#[derive(Clone)]
pub struct PgJobQueue {
    pool: PgPool,
}

impl PgJobQueue {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn lease_deadline(now: DateTime<Utc>, lease: Duration) -> Result<DateTime<Utc>, CoreError> {
    chrono::Duration::from_std(lease)
        .ok()
        .and_then(|delta| now.checked_add_signed(delta))
        .ok_or_else(|| CoreError::Internal("job lease duration is out of range".to_owned()))
}

impl JobQueue for PgJobQueue {
    #[tracing::instrument(skip(self, job), fields(db.system = "postgresql", db.operation = "insert", db.table = "jobs", job.kind = job.kind.as_str()), err)]
    async fn enqueue(&self, job: &NewJob, now: DateTime<Utc>) -> Result<Option<JobId>, CoreError> {
        let id = common::generate_uuid_v7();
        let inserted = sqlx::query_scalar!(
            r#"
            INSERT INTO jobs (id, kind, payload, status, attempts, max_attempts, run_at, dedup_key, created_at, updated_at)
            VALUES ($1, $2, $3, 'pending', 0, $4, $5, $6, $7, $7)
            ON CONFLICT (dedup_key) WHERE dedup_key IS NOT NULL AND status IN ('pending', 'running')
            DO NOTHING
            RETURNING id
            "#,
            id,
            job.kind.as_str(),
            job.payload,
            i32::from(job.max_attempts.get()),
            job.run_at,
            job.dedup_key.as_deref(),
            now,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(inserted.map(JobId))
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "claim", db.table = "jobs"), err)]
    async fn claim(
        &self,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<Option<ClaimedJob>, CoreError> {
        let lease_until = lease_deadline(now, lease)?;
        let token = new_lease_token();
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;
        sqlx::query!(
            r#"
            UPDATE jobs
            SET status = 'failed', last_error = 'lease expired',
                lease_token = NULL, lease_until = NULL, updated_at = $1
            WHERE status = 'running' AND lease_until < $1 AND attempts >= max_attempts
            "#,
            now,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;
        let row = sqlx::query_as!(
            JobRow,
            r#"
            WITH next AS (
                SELECT id FROM jobs
                WHERE (status = 'pending' AND run_at <= $1)
                   OR (status = 'running' AND lease_until < $1 AND attempts < max_attempts)
                ORDER BY run_at, id
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            UPDATE jobs
            SET status = 'running', attempts = attempts + 1,
                lease_token = $2, lease_until = $3, updated_at = $1
            FROM next
            WHERE jobs.id = next.id
            RETURNING jobs.id, jobs.kind, jobs.payload, jobs.status, jobs.attempts,
                jobs.max_attempts, jobs.run_at, jobs.lease_token, jobs.lease_until,
                jobs.last_error, jobs.created_at, jobs.updated_at
            "#,
            now,
            token.0,
            lease_until,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;
        tx.commit().await.map_err(map_sqlx_error)?;
        row.map(ClaimedJob::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "jobs", job.id = %id), err)]
    async fn extend_lease(
        &self,
        id: JobId,
        token: LeaseToken,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<bool, CoreError> {
        let lease_until = lease_deadline(now, lease)?;
        let result = sqlx::query!(
            r#"
            UPDATE jobs SET lease_until = $3, updated_at = $4
            WHERE id = $1 AND status = 'running' AND lease_token = $2
            "#,
            id.0,
            token.0,
            lease_until,
            now,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() == 1)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "jobs", job.id = %id), err)]
    async fn complete(
        &self,
        id: JobId,
        token: LeaseToken,
        now: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let result = sqlx::query!(
            r#"
            UPDATE jobs
            SET status = 'succeeded', lease_token = NULL, lease_until = NULL,
                last_error = NULL, updated_at = $3
            WHERE id = $1 AND status = 'running' AND lease_token = $2
            "#,
            id.0,
            token.0,
            now,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() == 1)
    }

    #[tracing::instrument(skip(self, failure), fields(db.system = "postgresql", db.operation = "update", db.table = "jobs", job.id = %id), err)]
    async fn fail(
        &self,
        id: JobId,
        token: LeaseToken,
        failure: &JobFailure,
        now: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let result = match failure {
            JobFailure::RetryAt { at, error } => {
                sqlx::query!(
                    r#"
                    UPDATE jobs
                    SET status = 'pending', run_at = $3, lease_token = NULL,
                        lease_until = NULL, last_error = $4, updated_at = $5
                    WHERE id = $1 AND status = 'running' AND lease_token = $2
                    "#,
                    id.0,
                    token.0,
                    at,
                    error,
                    now,
                )
                .execute(&self.pool)
                .await
            }
            JobFailure::GiveUp { error } => {
                sqlx::query!(
                    r#"
                    UPDATE jobs
                    SET status = 'failed', lease_token = NULL, lease_until = NULL,
                        last_error = $3, updated_at = $4
                    WHERE id = $1 AND status = 'running' AND lease_token = $2
                    "#,
                    id.0,
                    token.0,
                    error,
                    now,
                )
                .execute(&self.pool)
                .await
            }
        }
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() == 1)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "jobs", job.id = %id), err)]
    async fn release(
        &self,
        id: JobId,
        token: LeaseToken,
        now: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let result = sqlx::query!(
            r#"
            UPDATE jobs
            SET status = 'pending', run_at = $3, attempts = GREATEST(attempts - 1, 0),
                lease_token = NULL, lease_until = NULL, updated_at = $3
            WHERE id = $1 AND status = 'running' AND lease_token = $2
            "#,
            id.0,
            token.0,
            now,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() == 1)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "jobs", job.id = %id), err)]
    async fn find(&self, id: JobId) -> Result<Option<Job>, CoreError> {
        let row = sqlx::query_as!(
            JobRow,
            r#"
            SELECT id, kind, payload, status, attempts, max_attempts, run_at,
                   lease_token, lease_until, last_error, created_at, updated_at
            FROM jobs WHERE id = $1
            "#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        row.map(Job::try_from).transpose()
    }
}
