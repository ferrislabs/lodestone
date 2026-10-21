use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use serde_json::{Map, Value, from_value, to_value};
use sqlx::{query, query_as};

use crate::{
    UserId,
    domain::{
        llm::provider::SealedSecret,
        source_provider::{
            ConnectedAccount, OAuthState, ProviderConfiguration, ProviderStatus,
            SourceProviderRepository,
        },
    },
    infrastructure::postgres::{SharedTx, error::map_sqlx_error},
};

#[repository(domain = SourceProvider, backend = Postgres)]
pub struct PgSourceProviderRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgSourceProviderRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

struct ConfigurationRow {
    provider_key: String,
    settings: Value,
    status: String,
    last_test_at: Option<DateTime<Utc>>,
    last_test_message: Option<String>,
    connected_account: Option<Value>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ConfigurationRow {
    fn into_configuration(
        self,
        secrets: BTreeMap<String, SealedSecret>,
    ) -> Result<ProviderConfiguration, CoreError> {
        let Value::Object(settings) = self.settings else {
            return Err(CoreError::Internal(
                "stored provider settings are not an object".to_owned(),
            ));
        };
        let status = ProviderStatus::parse(&self.status)
            .filter(|status| *status != ProviderStatus::NotConfigured)
            .ok_or_else(|| CoreError::Internal("stored provider status is invalid".to_owned()))?;
        let connected_account = self
            .connected_account
            .map(from_value::<ConnectedAccount>)
            .transpose()
            .map_err(|_| {
                CoreError::Internal("stored provider connected account is invalid".to_owned())
            })?;
        Ok(ProviderConfiguration {
            key: self.provider_key,
            settings,
            secrets,
            status,
            last_test_at: self.last_test_at,
            last_test_message: self.last_test_message,
            connected_account,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

impl<'tx> SourceProviderRepository for PgSourceProviderRepository<'tx> {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "source_provider_configurations", provider.key = %key), err)]
    async fn find(&mut self, key: &str) -> Result<Option<ProviderConfiguration>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = query_as!(
            ConfigurationRow,
            r#"
            SELECT provider_key, settings, status, last_test_at, last_test_message,
                   connected_account, created_at, updated_at
            FROM source_provider_configurations
            WHERE provider_key = $1
            "#,
            key,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let secrets = query!(
            r#"SELECT name, nonce, ciphertext FROM source_provider_secrets WHERE provider_key = $1"#,
            key,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?
        .into_iter()
        .map(|secret| {
            (
                secret.name,
                SealedSecret {
                    nonce: secret.nonce,
                    ciphertext: secret.ciphertext,
                },
            )
        })
        .collect();
        row.into_configuration(secrets).map(Some)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "source_provider_configurations"), err)]
    async fn list(&mut self) -> Result<Vec<ProviderConfiguration>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = query_as!(
            ConfigurationRow,
            r#"
            SELECT provider_key, settings, status, last_test_at, last_test_message,
                   connected_account, created_at, updated_at
            FROM source_provider_configurations
            ORDER BY provider_key
            "#,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let secret_rows =
            query!(r#"SELECT provider_key, name, nonce, ciphertext FROM source_provider_secrets"#,)
                .fetch_all(&mut ***tx)
                .await
                .map_err(map_sqlx_error)?;
        let mut by_provider: BTreeMap<String, BTreeMap<String, SealedSecret>> = BTreeMap::new();
        for secret in secret_rows {
            by_provider.entry(secret.provider_key).or_default().insert(
                secret.name,
                SealedSecret {
                    nonce: secret.nonce,
                    ciphertext: secret.ciphertext,
                },
            );
        }
        rows.into_iter()
            .map(|row| {
                let secrets = by_provider.remove(&row.provider_key).unwrap_or_default();
                row.into_configuration(secrets)
            })
            .collect()
    }

    #[tracing::instrument(skip(self, configuration), fields(db.system = "postgresql", db.operation = "upsert", db.table = "source_provider_configurations", provider.key = %configuration.key), err)]
    async fn save(&mut self, configuration: &ProviderConfiguration) -> Result<(), CoreError> {
        let account = configuration
            .connected_account
            .as_ref()
            .map(to_value)
            .transpose()
            .map_err(|_| CoreError::Internal("connected account is not serialisable".to_owned()))?;
        let settings = Value::Object(Map::clone(&configuration.settings));
        let mut tx = self.tx.lock().await;
        query!(
            r#"
            INSERT INTO source_provider_configurations (
                provider_key, settings, status, last_test_at, last_test_message,
                connected_account, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (provider_key) DO UPDATE
            SET settings = EXCLUDED.settings,
                status = EXCLUDED.status,
                last_test_at = EXCLUDED.last_test_at,
                last_test_message = EXCLUDED.last_test_message,
                connected_account = EXCLUDED.connected_account,
                updated_at = EXCLUDED.updated_at
            "#,
            configuration.key,
            settings,
            configuration.status.as_str(),
            configuration.last_test_at,
            configuration.last_test_message.as_deref(),
            account,
            configuration.created_at,
            configuration.updated_at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        let names: Vec<String> = configuration.secrets.keys().cloned().collect();
        query!(
            r#"DELETE FROM source_provider_secrets WHERE provider_key = $1 AND name <> ALL($2)"#,
            configuration.key,
            &names,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        for (name, sealed) in &configuration.secrets {
            query!(
                r#"
                INSERT INTO source_provider_secrets (provider_key, name, nonce, ciphertext)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (provider_key, name) DO UPDATE
                SET nonce = EXCLUDED.nonce, ciphertext = EXCLUDED.ciphertext
                "#,
                configuration.key,
                name,
                sealed.nonce,
                sealed.ciphertext,
            )
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "delete", db.table = "source_provider_configurations", provider.key = %key), err)]
    async fn delete(&mut self, key: &str) -> Result<bool, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = query!(
            r#"DELETE FROM source_provider_configurations WHERE provider_key = $1"#,
            key,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(skip(self, state), fields(db.system = "postgresql", db.operation = "insert", db.table = "source_provider_oauth_states", provider.key = %state.provider_key), err)]
    async fn insert_oauth_state(&mut self, state: &OAuthState) -> Result<(), CoreError> {
        let mut tx = self.tx.lock().await;
        query!(
            r#"
            INSERT INTO source_provider_oauth_states (
                state, provider_key, user_id, code_verifier_nonce, code_verifier_cipher,
                redirect_uri, created_at, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            state.state,
            state.provider_key,
            state.user_id.0,
            state.code_verifier.nonce,
            state.code_verifier.ciphertext,
            state.redirect_uri,
            state.created_at,
            state.expires_at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self, state), fields(db.system = "postgresql", db.operation = "delete", db.table = "source_provider_oauth_states"), err)]
    async fn take_oauth_state(&mut self, state: &str) -> Result<Option<OAuthState>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = query!(
            r#"
            DELETE FROM source_provider_oauth_states
            WHERE state = $1
            RETURNING state, provider_key, user_id, code_verifier_nonce, code_verifier_cipher,
                      redirect_uri, created_at, expires_at
            "#,
            state,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.map(|row| OAuthState {
            state: row.state,
            provider_key: row.provider_key,
            user_id: UserId(row.user_id),
            code_verifier: SealedSecret {
                nonce: row.code_verifier_nonce,
                ciphertext: row.code_verifier_cipher,
            },
            redirect_uri: row.redirect_uri,
            created_at: row.created_at,
            expires_at: row.expires_at,
        }))
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "delete", db.table = "source_provider_oauth_states"), err)]
    async fn purge_oauth_states(&mut self, before: DateTime<Utc>) -> Result<u64, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = query!(
            r#"DELETE FROM source_provider_oauth_states WHERE expires_at <= $1"#,
            before,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected())
    }
}
