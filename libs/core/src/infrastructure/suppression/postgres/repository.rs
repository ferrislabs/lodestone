use common::CoreError;
use lodestone_macros::repository;

use crate::{
    EmailAddress, NormalizedDomain, SuppressionEntry, SuppressionId,
    domain::suppression::ports::SuppressionRepository,
    infrastructure::{
        postgres::{SharedTx, error::map_sqlx_error},
        suppression::postgres::model::{SuppressionRow, target_columns},
    },
};

#[repository(domain = Suppression, backend = Postgres)]
pub struct PgSuppressionRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgSuppressionRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> SuppressionRepository for PgSuppressionRepository<'tx> {
    #[tracing::instrument(skip(self, entry), fields(db.system = "postgresql", db.operation = "insert", db.table = "suppression_list", suppression.id = %entry.id), err)]
    async fn insert(&mut self, entry: &SuppressionEntry) -> Result<(), CoreError> {
        let (kind, value) = target_columns(&entry.target);
        let mut tx = self.tx.lock().await;
        sqlx::query!(
            r#"
            INSERT INTO suppression_list (id, kind, value, reason, created_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            entry.id.0,
            kind,
            value,
            entry.reason.as_deref(),
            entry.created_by.map(|id| id.0),
            entry.created_at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "delete", db.table = "suppression_list", suppression.id = %id), err)]
    async fn remove(&mut self, id: SuppressionId) -> Result<bool, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = sqlx::query!(r#"DELETE FROM suppression_list WHERE id = $1"#, id.0,)
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;

        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "suppression_list"), err)]
    async fn list(&mut self) -> Result<Vec<SuppressionEntry>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            SuppressionRow,
            r#"
            SELECT id, kind, value, reason, created_by, created_at
            FROM suppression_list
            ORDER BY created_at DESC, id DESC
            "#,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(SuppressionEntry::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "suppression_list", domain = %domain), err)]
    async fn is_domain_suppressed(&mut self, domain: &NormalizedDomain) -> Result<bool, CoreError> {
        let mut tx = self.tx.lock().await;
        let found = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM suppression_list WHERE kind = 'domain' AND value = $1
            ) AS "found!"
            "#,
            domain.as_str(),
        )
        .fetch_one(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(found)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "suppression_list"), err)]
    async fn is_email_suppressed(&mut self, email: &EmailAddress) -> Result<bool, CoreError> {
        let registrable = NormalizedDomain::from_host(email.domain()).ok();
        let mut tx = self.tx.lock().await;
        let found = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM suppression_list
                WHERE (kind = 'email' AND value = $1)
                   OR (kind = 'domain' AND value = $2)
            ) AS "found!"
            "#,
            email.as_str(),
            registrable.as_ref().map(NormalizedDomain::as_str),
        )
        .fetch_one(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(found)
    }
}
