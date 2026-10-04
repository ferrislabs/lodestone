use common::CoreError;
use sqlx::PgPool;

use crate::{
    domain::llm::cache::{CacheScope, CachedOutput, LlmCache},
    infrastructure::postgres::error::map_sqlx_error,
};

#[derive(Clone)]
pub struct PgLlmCache {
    pool: PgPool,
}

impl PgLlmCache {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl LlmCache for PgLlmCache {
    #[tracing::instrument(skip_all, fields(db.system = "postgresql", db.operation = "select", db.table = "llm_cache"), err)]
    async fn get(&self, key: &[u8]) -> Result<Option<CachedOutput>, CoreError> {
        let row = sqlx::query!(
            r#"SELECT output, model, created_at FROM llm_cache WHERE cache_key = $1"#,
            key,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.map(|row| CachedOutput {
            output: row.output,
            model: row.model,
            created_at: row.created_at,
        }))
    }

    #[tracing::instrument(skip_all, fields(db.system = "postgresql", db.operation = "upsert", db.table = "llm_cache"), err)]
    async fn put(
        &self,
        key: &[u8],
        scope: &CacheScope,
        entry: &CachedOutput,
    ) -> Result<(), CoreError> {
        let revision = i32::try_from(scope.revision.get())
            .map_err(|_| CoreError::Internal("profile revision out of range".to_owned()))?;
        sqlx::query!(
            r#"
            INSERT INTO llm_cache (cache_key, profile_id, profile_revision, prompt, domain, output, model, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (cache_key) DO UPDATE SET
                output = EXCLUDED.output, model = EXCLUDED.model, created_at = EXCLUDED.created_at
            "#,
            key,
            scope.profile_id.0,
            revision,
            scope.prompt,
            scope.domain.as_str(),
            entry.output,
            entry.model,
            entry.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }
}
