use chrono::{DateTime, Utc};
use common::CoreError;

use crate::{
    JobId, NewJob,
    domain::output::ports::JobOutbox,
    infrastructure::postgres::{SharedTx, error::map_sqlx_error},
};

pub struct PgJobOutbox<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgJobOutbox<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> JobOutbox for PgJobOutbox<'tx> {
    #[tracing::instrument(skip(self, job), fields(db.system = "postgresql", db.operation = "insert", db.table = "jobs", job.kind = job.kind.as_str()), err)]
    async fn enqueue(
        &mut self,
        job: &NewJob,
        now: DateTime<Utc>,
    ) -> Result<Option<JobId>, CoreError> {
        let id = common::generate_uuid_v7();
        let mut tx = self.tx.lock().await;
        let inserted = sqlx::query_scalar::<_, uuid::Uuid>(
            r#"
            INSERT INTO jobs (id, kind, payload, status, attempts, max_attempts, run_at, dedup_key, created_at, updated_at)
            VALUES ($1, $2, $3, 'pending', 0, $4, $5, $6, $7, $7)
            ON CONFLICT (dedup_key) WHERE dedup_key IS NOT NULL AND status IN ('pending', 'running')
            DO NOTHING
            RETURNING id
            "#,
        )
        .bind(id)
        .bind(job.kind.as_str())
        .bind(&job.payload)
        .bind(i32::from(job.max_attempts.get()))
        .bind(job.run_at)
        .bind(job.dedup_key.as_deref())
        .bind(now)
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(inserted.map(JobId))
    }
}
