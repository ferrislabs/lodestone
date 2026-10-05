use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use sqlx::Row;

use crate::{
    domain::llm::provider::{LlmProviderRecord, LlmProviderRepository, ProviderId, Tier},
    infrastructure::{
        llm_provider::postgres::model::{LlmProviderRow, pricing_cents, timeout_ms},
        postgres::{SharedTx, error::map_sqlx_error},
    },
};

#[repository(domain = LlmProvider, backend = Postgres)]
pub struct PgLlmProviderRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgLlmProviderRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

struct Params<'a> {
    timeout_ms: i32,
    max_retries: i16,
    input: i32,
    output: i32,
    nonce: Option<&'a [u8]>,
    ciphertext: Option<&'a [u8]>,
}

impl<'a> Params<'a> {
    fn of(record: &'a LlmProviderRecord) -> Result<Self, CoreError> {
        Ok(Self {
            timeout_ms: timeout_ms(record.timeout)?,
            max_retries: i16::from(record.max_retries),
            input: pricing_cents(record.pricing.input_cents_per_mtok)?,
            output: pricing_cents(record.pricing.output_cents_per_mtok)?,
            nonce: record.api_key.as_ref().map(|key| key.nonce.as_slice()),
            ciphertext: record.api_key.as_ref().map(|key| key.ciphertext.as_slice()),
        })
    }
}

impl<'tx> LlmProviderRepository for PgLlmProviderRepository<'tx> {
    #[tracing::instrument(skip(self, record), fields(db.system = "postgresql", db.operation = "insert", db.table = "llm_providers", provider.id = %record.id), err)]
    async fn insert(&mut self, record: &LlmProviderRecord) -> Result<(), CoreError> {
        let params = Params::of(record)?;
        let mut tx = self.tx.lock().await;
        sqlx::query!(
            r#"
            INSERT INTO llm_providers (
                id, name, kind, base_url, model, timeout_ms, max_retries,
                input_cents_per_mtok, output_cents_per_mtok,
                api_key_nonce, api_key_ciphertext, enabled, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            "#,
            record.id.0,
            record.name,
            record.kind.as_str(),
            record.base_url.as_str(),
            record.model,
            params.timeout_ms,
            params.max_retries,
            params.input,
            params.output,
            params.nonce,
            params.ciphertext,
            record.enabled,
            record.created_at,
            record.updated_at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    #[tracing::instrument(skip(self, record), fields(db.system = "postgresql", db.operation = "update", db.table = "llm_providers", provider.id = %record.id), err)]
    async fn update(&mut self, record: &LlmProviderRecord) -> Result<bool, CoreError> {
        let params = Params::of(record)?;
        let mut tx = self.tx.lock().await;
        let result = sqlx::query!(
            r#"
            UPDATE llm_providers
            SET name = $2, kind = $3, base_url = $4, model = $5, timeout_ms = $6,
                max_retries = $7, input_cents_per_mtok = $8, output_cents_per_mtok = $9,
                api_key_nonce = $10, api_key_ciphertext = $11, enabled = $12, updated_at = $13
            WHERE id = $1
            "#,
            record.id.0,
            record.name,
            record.kind.as_str(),
            record.base_url.as_str(),
            record.model,
            params.timeout_ms,
            params.max_retries,
            params.input,
            params.output,
            params.nonce,
            params.ciphertext,
            record.enabled,
            record.updated_at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "llm_providers", provider.id = %id), err)]
    async fn find(&mut self, id: ProviderId) -> Result<Option<LlmProviderRecord>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            LlmProviderRow,
            r#"
            SELECT id, name, kind, base_url, model, timeout_ms, max_retries,
                   input_cents_per_mtok, output_cents_per_mtok,
                   api_key_nonce, api_key_ciphertext, enabled, created_at, updated_at
            FROM llm_providers
            WHERE id = $1
            "#,
            id.0,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(LlmProviderRecord::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "llm_providers"), err)]
    async fn list(&mut self) -> Result<Vec<LlmProviderRecord>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            LlmProviderRow,
            r#"
            SELECT id, name, kind, base_url, model, timeout_ms, max_retries,
                   input_cents_per_mtok, output_cents_per_mtok,
                   api_key_nonce, api_key_ciphertext, enabled, created_at, updated_at
            FROM llm_providers
            ORDER BY name
            "#,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(LlmProviderRecord::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "upsert", db.table = "llm_tiers", provider.id = %provider), err)]
    async fn assign_tier(
        &mut self,
        tier: Tier,
        provider: ProviderId,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut tx = self.tx.lock().await;
        sqlx::query!(
            r#"
            INSERT INTO llm_tiers (tier, provider_id, updated_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (tier) DO UPDATE
            SET provider_id = EXCLUDED.provider_id, updated_at = EXCLUDED.updated_at
            "#,
            tier.as_str(),
            provider.0,
            at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "llm_tiers"), err)]
    async fn tier_provider(&mut self, tier: Tier) -> Result<Option<LlmProviderRecord>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            LlmProviderRow,
            r#"
            SELECT p.id, p.name, p.kind, p.base_url, p.model, p.timeout_ms, p.max_retries,
                   p.input_cents_per_mtok, p.output_cents_per_mtok,
                   p.api_key_nonce, p.api_key_ciphertext, p.enabled, p.created_at, p.updated_at
            FROM llm_tiers t
            JOIN llm_providers p ON p.id = t.provider_id
            WHERE t.tier = $1
            "#,
            tier.as_str(),
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(LlmProviderRecord::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "delete", db.table = "llm_providers", provider.id = %id), err)]
    async fn delete(&mut self, id: ProviderId) -> Result<bool, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = sqlx::query("DELETE FROM llm_providers WHERE id = $1")
            .bind(id.0)
            .execute(&mut ***tx)
            .await
            .map_err(|error| match &error {
                sqlx::Error::Database(db) if db.is_foreign_key_violation() => {
                    CoreError::Conflict("provider is assigned to a tier".to_owned())
                }
                _ => map_sqlx_error(error),
            })?;

        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "llm_tiers"), err)]
    async fn list_tiers(&mut self) -> Result<Vec<(Tier, ProviderId)>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query("SELECT tier, provider_id FROM llm_tiers ORDER BY tier")
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;

        rows.iter()
            .map(|row| {
                let tier: String = row.try_get("tier").map_err(map_sqlx_error)?;
                let provider: uuid::Uuid = row.try_get("provider_id").map_err(map_sqlx_error)?;
                let tier = Tier::parse(&tier)
                    .ok_or_else(|| CoreError::Internal("stored llm tier is invalid".to_owned()))?;
                Ok((tier, ProviderId(provider)))
            })
            .collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "delete", db.table = "llm_tiers"), err)]
    async fn unassign_tier(&mut self, tier: Tier) -> Result<bool, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = sqlx::query("DELETE FROM llm_tiers WHERE tier = $1")
            .bind(tier.as_str())
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;

        Ok(result.rows_affected() > 0)
    }
}
