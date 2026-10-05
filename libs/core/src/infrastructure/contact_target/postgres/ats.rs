use chrono::{DateTime, Utc};
use common::CoreError;
use sqlx::PgPool;

use crate::{
    NormalizedDomain, SourceName, domain::ats_target::AtsTargets,
    infrastructure::postgres::error::map_sqlx_error,
};

const PASSIVE_SOURCES: [SourceName; 3] = [
    SourceName::CertificateTransparency,
    SourceName::HttpProbe,
    SourceName::DnsCname,
];

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
        checked_before: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<NormalizedDomain>, CoreError> {
        let passive: Vec<String> = PASSIVE_SOURCES
            .iter()
            .map(|source| source.as_str().to_owned())
            .collect();
        let rows = sqlx::query_scalar!(
            r#"
            SELECT l.domain AS "domain!"
            FROM (
                SELECT DISTINCT leads.domain
                FROM leads
                JOIN profiles ON profiles.id = leads.profile_id AND profiles.active
                JOIN LATERAL (
                    SELECT definition FROM profile_revisions
                    WHERE profile_id = profiles.id
                    ORDER BY revision DESC
                    LIMIT 1
                ) r ON jsonb_typeof(r.definition #> '{sources,ats}') = 'object'
                WHERE EXISTS (
                    SELECT 1 FROM signals s
                    WHERE s.lead_id = leads.id AND s.source = ANY($3::text[])
                )
            ) l
            LEFT JOIN lead_domain_checks c ON c.domain = l.domain AND c.source = $1
            WHERE (c.checked_at IS NULL OR c.checked_at < $2)
              AND NOT EXISTS (
                  SELECT 1 FROM suppression_list s WHERE s.kind = 'domain' AND s.value = l.domain
              )
            ORDER BY c.checked_at NULLS FIRST, l.domain
            LIMIT $4
            "#,
            SourceName::Ats.as_str(),
            checked_before,
            &passive,
            i64::from(limit),
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
        Ok(())
    }
}
