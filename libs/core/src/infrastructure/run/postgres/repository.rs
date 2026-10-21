use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;

use crate::{
    CancelReason, Run, RunId, RunOutcome,
    definition::AgentId,
    domain::run::ports::RunRepository,
    infrastructure::{
        postgres::{SharedTx, error::map_sqlx_error},
        run::postgres::model::{RunRow, to_i64},
    },
};

#[repository(domain = Run, backend = Postgres)]
pub struct PgRunRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgRunRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> RunRepository for PgRunRepository<'tx> {
    #[tracing::instrument(skip(self, run), fields(db.system = "postgresql", db.operation = "insert", db.table = "runs", run.id = %run.id), err)]
    async fn insert(&mut self, run: &Run) -> Result<(), CoreError> {
        let volume = to_i64(run.volume)?;
        let signals = to_i64(run.signals)?;
        let mut tx = self.tx.lock().await;
        sqlx::query!(
            r#"
            INSERT INTO runs (id, profile_id, source, status, started_at, finished_at, volume, signals, error, agent_id, cancel_reason)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            run.id.0,
            run.profile_id.0,
            run.source.as_str(),
            run.status.as_str(),
            run.started_at,
            run.finished_at,
            volume,
            signals,
            run.error.as_deref(),
            run.agent.as_ref().map(AgentId::as_str),
            run.cancel_reason.map(CancelReason::as_str),
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    #[tracing::instrument(skip(self, outcome), fields(db.system = "postgresql", db.operation = "update", db.table = "runs", run.id = %id), err)]
    async fn finish(
        &mut self,
        id: RunId,
        outcome: &RunOutcome,
        finished_at: DateTime<Utc>,
    ) -> Result<Option<Run>, CoreError> {
        let (volume, signals, error) = match outcome {
            RunOutcome::Succeeded { volume, signals } => (*volume, *signals, None),
            RunOutcome::Failed {
                volume,
                signals,
                error,
            } => (*volume, *signals, Some(error.as_str())),
            RunOutcome::Cancelled {
                volume, signals, ..
            } => (*volume, *signals, None),
        };
        let volume = to_i64(volume)?;
        let signals = to_i64(signals)?;
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            RunRow,
            r#"
            UPDATE runs
            SET status = $2, volume = $3, signals = $4, error = $5, finished_at = $6, cancel_reason = $7
            WHERE id = $1 AND status = 'running'
            RETURNING id, profile_id, source, status, started_at, finished_at, volume, signals, error, agent_id, cancel_reason
            "#,
            id.0,
            outcome.status().as_str(),
            volume,
            signals,
            error,
            finished_at,
            outcome.cancel_reason().map(CancelReason::as_str),
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(Run::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "runs", run.id = %id), err)]
    async fn find(&mut self, id: RunId) -> Result<Option<Run>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            RunRow,
            r#"
            SELECT id, profile_id, source, status, started_at, finished_at, volume, signals, error, agent_id, cancel_reason
            FROM runs
            WHERE id = $1
            "#,
            id.0,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(Run::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "runs"), err)]
    async fn list_recent(&mut self, limit: u32) -> Result<Vec<Run>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            RunRow,
            r#"
            SELECT id, profile_id, source, status, started_at, finished_at, volume, signals, error, agent_id, cancel_reason
            FROM runs
            ORDER BY started_at DESC, id DESC
            LIMIT $1
            "#,
            i64::from(limit),
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(Run::try_from).collect()
    }
}
