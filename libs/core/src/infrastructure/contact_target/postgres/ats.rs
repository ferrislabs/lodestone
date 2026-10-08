use chrono::{DateTime, Utc};
use common::CoreError;
use sqlx::{PgPool, query};

use crate::{
    NormalizedDomain, ProfileId, SourceName,
    domain::{ats_target::AtsTargets, enrichment::EnrichmentStep},
    infrastructure::postgres::error::map_sqlx_error,
};

#[derive(Clone)]
pub struct PgAtsTargets {
    pool: PgPool,
}

impl PgAtsTargets {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl AtsTargets for PgAtsTargets {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads"), err)]
    async fn list_due(
        &self,
        profiles: &[ProfileId],
        checked_before: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<NormalizedDomain>, CoreError> {
        if profiles.is_empty() {
            return Ok(Vec::new());
        }
        let profiles: Vec<uuid::Uuid> = profiles.iter().map(|profile| profile.0).collect();
        let rows = sqlx::query_scalar!(
            r#"
            SELECT l.domain AS "domain!"
            FROM (
                SELECT DISTINCT leads.domain
                FROM leads
                WHERE leads.profile_id = ANY($3::uuid[])
            ) l
            LEFT JOIN lead_domain_checks c ON c.domain = l.domain AND c.source = $1
            WHERE (
                c.checked_at IS NULL
                OR c.checked_at < $2
                OR EXISTS (
                    SELECT 1 FROM leads fresh
                    WHERE fresh.domain = l.domain
                      AND fresh.profile_id = ANY($3::uuid[])
                      AND NOT EXISTS (
                          SELECT 1 FROM lead_enrichments e
                          WHERE e.lead_id = fresh.id AND e.step = $5 AND e.ran_at IS NOT NULL
                      )
                )
            )
              AND NOT EXISTS (
                  SELECT 1 FROM suppression_list s WHERE s.kind = 'domain' AND s.value = l.domain
              )
            ORDER BY c.checked_at NULLS FIRST, l.domain
            LIMIT $4
            "#,
            SourceName::Ats.as_str(),
            checked_before,
            &profiles,
            i64::from(limit),
            EnrichmentStep::CompanyJobs.as_str(),
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
            SourceName::Ats.as_str(),
            at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        query!(
            r#"
            INSERT INTO lead_enrichments (lead_id, step, ran_at)
            SELECT leads.id, $2, $3 FROM leads WHERE leads.domain = ANY($1::text[])
            ON CONFLICT (lead_id, step) DO UPDATE SET ran_at = EXCLUDED.ran_at
            "#,
            &domains,
            EnrichmentStep::CompanyJobs.as_str(),
            at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }
}
