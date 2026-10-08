use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use sqlx::{query, query_as};
use tracing::instrument;
use uuid::Uuid;

use crate::{
    UserId,
    domain::settings::{StoredSetting, ports::OrganisationSettingRepository},
    infrastructure::postgres::{SharedTx, error::map_sqlx_error},
};

#[repository(domain = OrganisationSetting, backend = Postgres)]
pub struct PgOrganisationSettingRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgOrganisationSettingRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

struct SettingRow {
    value: String,
    updated_by: Option<Uuid>,
    updated_by_name: Option<String>,
    updated_at: DateTime<Utc>,
}

impl<'tx> OrganisationSettingRepository for PgOrganisationSettingRepository<'tx> {
    #[instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "organisation_settings", setting.key = %key), err)]
    async fn find(&mut self, key: &str) -> Result<Option<StoredSetting>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = query_as!(
            SettingRow,
            r#"
            SELECT s.value, s.updated_by, u.display_name AS "updated_by_name?", s.updated_at
            FROM organisation_settings s
            LEFT JOIN users u ON u.id = s.updated_by
            WHERE s.key = $1
            "#,
            key,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.map(|row| StoredSetting {
            value: row.value,
            updated_by: row.updated_by.map(UserId),
            updated_by_name: row.updated_by_name,
            updated_at: row.updated_at,
        }))
    }

    #[instrument(skip(self), fields(db.system = "postgresql", db.operation = "upsert", db.table = "organisation_settings", setting.key = %key), err)]
    async fn save(
        &mut self,
        key: &str,
        value: &str,
        updated_by: UserId,
        updated_at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut tx = self.tx.lock().await;
        query!(
            r#"
            INSERT INTO organisation_settings (key, value, updated_by, updated_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (key) DO UPDATE SET
                value      = EXCLUDED.value,
                updated_by = EXCLUDED.updated_by,
                updated_at = EXCLUDED.updated_at
            "#,
            key,
            value,
            updated_by.0,
            updated_at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }
}
