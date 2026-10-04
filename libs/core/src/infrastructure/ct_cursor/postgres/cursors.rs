use chrono::{DateTime, Utc};
use common::CoreError;
use sqlx::PgPool;

use crate::{domain::host::ports::CtCursors, infrastructure::postgres::error::map_sqlx_error};

#[derive(Clone)]
pub struct PgCtCursors {
    pool: PgPool,
}

impl PgCtCursors {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn to_i64(value: u64) -> Result<i64, CoreError> {
    i64::try_from(value).map_err(|_| CoreError::Internal("ct cursor out of range".to_owned()))
}

impl CtCursors for PgCtCursors {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "ct_log_cursors"), err)]
    async fn next_index(&self, log_id: &str) -> Result<Option<u64>, CoreError> {
        let stored = sqlx::query_scalar!(
            r#"SELECT next_index FROM ct_log_cursors WHERE log_id = $1"#,
            log_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        stored
            .map(|value| {
                u64::try_from(value)
                    .map_err(|_| CoreError::Internal("ct cursor out of range".to_owned()))
            })
            .transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "upsert", db.table = "ct_log_cursors"), err)]
    async fn advance(
        &self,
        log_id: &str,
        next_index: u64,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        sqlx::query!(
            r#"
            INSERT INTO ct_log_cursors (log_id, next_index, updated_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (log_id) DO UPDATE
            SET next_index = GREATEST(ct_log_cursors.next_index, EXCLUDED.next_index),
                updated_at = EXCLUDED.updated_at
            "#,
            log_id,
            to_i64(next_index)?,
            at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }
}
