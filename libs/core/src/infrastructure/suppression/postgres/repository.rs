use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use sqlx::{query, query_scalar};
use uuid::Uuid;

use crate::{
    EmailAddress, LeadId, NormalizedDomain, ProfileId, SuppressionEntry, SuppressionId,
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

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", lead.id = %lead_id), err)]
    async fn lead_exclusion(&mut self, lead_id: LeadId) -> Result<Option<String>, CoreError> {
        let mut tx = self.tx.lock().await;
        let reason = query_scalar!("SELECT excluded_reason FROM leads WHERE id = $1", lead_id.0,)
            .fetch_optional(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(reason.flatten())
    }

    #[tracing::instrument(skip(self, lead_ids), fields(db.system = "postgresql", db.operation = "update", db.table = "leads", leads = lead_ids.len()), err)]
    async fn exclude_leads(
        &mut self,
        lead_ids: &[LeadId],
        reason: &str,
        at: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        if lead_ids.is_empty() {
            return Ok(0);
        }
        let ids: Vec<Uuid> = lead_ids.iter().map(|id| id.0).collect();
        let mut tx = self.tx.lock().await;
        let result = query!(
            "UPDATE leads SET excluded_reason = $2, excluded_at = $3 WHERE id = ANY($1) AND excluded_reason IS NULL",
            &ids,
            reason,
            at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "leads", lead.id = %lead_id), err)]
    async fn lift_exclusion(&mut self, lead_id: LeadId, reason: &str) -> Result<bool, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = query!(
            "UPDATE leads SET excluded_reason = NULL, excluded_at = NULL WHERE id = $1 AND excluded_reason = $2",
            lead_id.0,
            reason,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() == 1)
    }

    #[tracing::instrument(skip(self, profiles, suffixes), fields(db.system = "postgresql", db.operation = "update", db.table = "leads"), err)]
    async fn exclude_leads_under(
        &mut self,
        profiles: &[ProfileId],
        suffixes: &[&'static str],
        reason: &str,
        at: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        if profiles.is_empty() || suffixes.is_empty() {
            return Ok(0);
        }
        let ids: Vec<Uuid> = profiles.iter().map(|id| id.0).collect();
        let mut tx = self.tx.lock().await;
        let result = query!(
            r#"
            UPDATE leads SET excluded_reason = $3, excluded_at = $4
            WHERE profile_id = ANY($1)
              AND excluded_reason IS NULL
              AND EXISTS (
                  SELECT 1 FROM UNNEST($2::text[]) AS tenant(suffix)
                  WHERE leads.domain = tenant.suffix
                     OR right(leads.domain, length(tenant.suffix) + 1) = '.' || tenant.suffix
              )
            "#,
            &ids,
            suffixes as &[&str],
            reason,
            at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected())
    }
}
