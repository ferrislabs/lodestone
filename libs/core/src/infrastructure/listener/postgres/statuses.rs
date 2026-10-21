use chrono::{DateTime, Utc};
use common::CoreError;
use sqlx::PgPool;

use crate::{
    ListenerState, ListenerStatus, StatusUpdate, infrastructure::postgres::error::map_sqlx_error,
};

#[derive(Clone)]
pub struct PgListenerStatuses {
    pool: PgPool,
}

impl PgListenerStatuses {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn counter(value: i64) -> Result<u64, CoreError> {
    u64::try_from(value)
        .map_err(|_| CoreError::Internal("stored listener counter is out of range".to_owned()))
}

impl PgListenerStatuses {
    #[tracing::instrument(skip(self, update), fields(db.system = "postgresql", db.operation = "upsert", db.table = "listeners", listener = update.name), err)]
    pub async fn record(&self, update: &StatusUpdate<'_>) -> Result<(), CoreError> {
        let processed = to_i64(update.outcome.processed);
        sqlx::query!(
            r#"
            INSERT INTO listeners (
                name, holder, state, last_activity_at, last_error, last_error_at,
                items_total, signals_total, backlog, updated_at
            )
            VALUES (
                $1, $2, $3, CASE WHEN $4::bigint > 0 THEN $8::timestamptz END, $5,
                CASE WHEN $5::text IS NOT NULL THEN $8::timestamptz END,
                $4::bigint, $6::bigint, $7, $8::timestamptz
            )
            ON CONFLICT (name) DO UPDATE SET
                holder = EXCLUDED.holder,
                state = EXCLUDED.state,
                last_activity_at = CASE
                    WHEN $4::bigint > 0 THEN $8::timestamptz
                    ELSE listeners.last_activity_at
                END,
                last_error = CASE
                    WHEN EXCLUDED.last_error IS NOT NULL THEN EXCLUDED.last_error
                    WHEN $9 THEN NULL
                    ELSE listeners.last_error
                END,
                last_error_at = CASE
                    WHEN EXCLUDED.last_error IS NOT NULL THEN EXCLUDED.last_error_at
                    WHEN $9 THEN NULL
                    ELSE listeners.last_error_at
                END,
                items_total = listeners.items_total + $4::bigint,
                signals_total = listeners.signals_total + $6::bigint,
                backlog = EXCLUDED.backlog,
                updated_at = EXCLUDED.updated_at
            "#,
            update.name,
            update.holder,
            update.state.as_str(),
            processed,
            update.error,
            to_i64(update.outcome.signals),
            update.outcome.backlog.map(to_i64),
            update.at,
            update.succeeded,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "upsert", db.table = "listeners"), err)]
    pub async fn record_started(
        &self,
        name: &str,
        holder: &str,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        sqlx::query!(
            r#"
            INSERT INTO listeners (name, holder, state, updated_at)
            VALUES ($1, $2, 'listening', $3)
            ON CONFLICT (name) DO UPDATE SET
                holder = EXCLUDED.holder,
                state = 'listening',
                updated_at = EXCLUDED.updated_at
            WHERE listeners.holder IS DISTINCT FROM EXCLUDED.holder
                OR listeners.state IN ('standby', 'stopped')
            "#,
            name,
            holder,
            at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "insert", db.table = "listeners"), err)]
    pub async fn record_standby(&self, name: &str, at: DateTime<Utc>) -> Result<(), CoreError> {
        sqlx::query!(
            r#"
            INSERT INTO listeners (name, state, updated_at)
            VALUES ($1, 'standby', $2)
            ON CONFLICT (name) DO NOTHING
            "#,
            name,
            at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "listeners"), err)]
    pub async fn touch(
        &self,
        name: &str,
        holder: &str,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        sqlx::query!(
            r#"
            UPDATE listeners SET updated_at = $3
            WHERE name = $1 AND holder = $2
            "#,
            name,
            holder,
            at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "listeners"), err)]
    pub async fn record_stopped(
        &self,
        name: &str,
        holder: &str,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        sqlx::query!(
            r#"
            UPDATE listeners SET state = 'stopped', updated_at = $3
            WHERE name = $1 AND holder = $2
            "#,
            name,
            holder,
            at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "listeners"), err)]
    pub async fn list(&self) -> Result<Vec<ListenerStatus>, CoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT name, holder, state, last_activity_at, last_error, last_error_at,
                items_total, signals_total, backlog, updated_at
            FROM listeners
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(ListenerStatus {
                    name: row.name,
                    holder: row.holder,
                    state: ListenerState::parse(&row.state).ok_or_else(|| {
                        CoreError::Internal("stored listener state is invalid".to_owned())
                    })?,
                    last_activity_at: row.last_activity_at,
                    last_error: row.last_error,
                    last_error_at: row.last_error_at,
                    items_total: counter(row.items_total)?,
                    signals_total: counter(row.signals_total)?,
                    backlog: row.backlog.map(counter).transpose()?,
                    updated_at: row.updated_at,
                })
            })
            .collect()
    }
}
