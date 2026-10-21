use chrono::{DateTime, Utc};
use common::CoreError;
use sqlx::PgPool;

use crate::{
    NormalizedDomain, SourceName, domain::contact::ContactTargets,
    infrastructure::postgres::error::map_sqlx_error,
};

#[derive(Clone)]
pub struct PgContactTargets {
    pool: PgPool,
}

impl PgContactTargets {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl PgContactTargets {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads"), err)]
    pub async fn count_due(
        &self,
        source: SourceName,
        checked_before: DateTime<Utc>,
        after: Option<SourceName>,
    ) -> Result<u64, CoreError> {
        let count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM (SELECT DISTINCT leads.domain FROM leads
                  JOIN profiles ON profiles.id = leads.profile_id AND profiles.active) l
            LEFT JOIN lead_domain_checks c ON c.domain = l.domain AND c.source = $1
            WHERE (c.checked_at IS NULL OR c.checked_at < $2)
              AND NOT EXISTS (
                  SELECT 1 FROM suppression_list s WHERE s.kind = 'domain' AND s.value = l.domain
              )
              AND ($3::text IS NULL OR EXISTS (
                  SELECT 1 FROM lead_domain_checks p WHERE p.domain = l.domain AND p.source = $3
              ))
            "#,
            source.as_str(),
            checked_before,
            after.map(SourceName::as_str),
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        u64::try_from(count)
            .map_err(|_| CoreError::Internal("due target count is out of range".to_owned()))
    }
}

impl ContactTargets for PgContactTargets {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads"), err)]
    async fn list_due(
        &self,
        source: SourceName,
        checked_before: DateTime<Utc>,
        limit: u32,
        after: Option<SourceName>,
    ) -> Result<Vec<NormalizedDomain>, CoreError> {
        let rows = sqlx::query_scalar!(
            r#"
            SELECT l.domain AS "domain!"
            FROM (SELECT DISTINCT leads.domain FROM leads
                  JOIN profiles ON profiles.id = leads.profile_id AND profiles.active) l
            LEFT JOIN lead_domain_checks c ON c.domain = l.domain AND c.source = $1
            WHERE (c.checked_at IS NULL OR c.checked_at < $2)
              AND NOT EXISTS (
                  SELECT 1 FROM suppression_list s WHERE s.kind = 'domain' AND s.value = l.domain
              )
              AND ($4::text IS NULL OR EXISTS (
                  SELECT 1 FROM lead_domain_checks p WHERE p.domain = l.domain AND p.source = $4
              ))
            ORDER BY c.checked_at NULLS FIRST, l.domain
            LIMIT $3
            "#,
            source.as_str(),
            checked_before,
            i64::from(limit),
            after.map(SourceName::as_str),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        rows.iter()
            .map(|domain| {
                NormalizedDomain::from_host(domain)
                    .map_err(|_| CoreError::Internal("stored lead domain is invalid".to_owned()))
            })
            .collect()
    }

    #[tracing::instrument(skip(self, domains), fields(db.system = "postgresql", db.operation = "upsert", db.table = "lead_domain_checks"), err)]
    async fn mark_checked(
        &self,
        domains: &[NormalizedDomain],
        source: SourceName,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let domains: Vec<String> = domains.iter().map(|d| d.as_str().to_owned()).collect();
        sqlx::query!(
            r#"
            INSERT INTO lead_domain_checks (domain, source, checked_at)
            SELECT domain, $2, $3 FROM UNNEST($1::text[]) AS t(domain)
            ON CONFLICT (domain, source) DO UPDATE SET checked_at = EXCLUDED.checked_at
            "#,
            &domains,
            source.as_str(),
            at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }
}
