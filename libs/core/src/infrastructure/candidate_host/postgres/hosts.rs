use chrono::{DateTime, Utc};
use common::CoreError;
use sqlx::PgPool;

use crate::{
    CandidateHost, CandidateHosts, Hostname, SourceName,
    infrastructure::{
        candidate_host::postgres::model::CandidateHostRow, postgres::error::map_sqlx_error,
    },
};

#[derive(Clone)]
pub struct PgCandidateHosts {
    pool: PgPool,
}

impl PgCandidateHosts {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl CandidateHosts for PgCandidateHosts {
    #[tracing::instrument(skip(self, hosts), fields(db.system = "postgresql", db.operation = "upsert", db.table = "candidate_hosts", batch = hosts.len()), err)]
    async fn upsert(&self, hosts: &[CandidateHost]) -> Result<u64, CoreError> {
        if hosts.is_empty() {
            return Ok(0);
        }
        let names: Vec<&str> = hosts.iter().map(|h| h.host.as_str()).collect();
        let domains: Vec<&str> = hosts.iter().map(|h| h.domain.as_str()).collect();
        let origins: Vec<&str> = hosts.iter().map(|h| h.origin.as_str()).collect();
        let firsts: Vec<DateTime<Utc>> = hosts.iter().map(|h| h.first_seen_at).collect();
        let lasts: Vec<DateTime<Utc>> = hosts.iter().map(|h| h.last_seen_at).collect();
        let result = sqlx::query!(
            r#"
            INSERT INTO candidate_hosts (host, domain, origin, first_seen_at, last_seen_at)
            SELECT * FROM UNNEST($1::text[], $2::text[], $3::text[], $4::timestamptz[], $5::timestamptz[])
                AS incoming(host, domain, origin, first_seen_at, last_seen_at)
            WHERE NOT EXISTS (
                SELECT 1 FROM suppression_list s WHERE s.kind = 'domain' AND s.value = incoming.domain
            )
            ON CONFLICT (host) DO UPDATE SET
                last_seen_at = GREATEST(candidate_hosts.last_seen_at, EXCLUDED.last_seen_at),
                first_seen_at = LEAST(candidate_hosts.first_seen_at, EXCLUDED.first_seen_at)
            "#,
            &names as &[&str],
            &domains as &[&str],
            &origins as &[&str],
            &firsts,
            &lasts,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "candidate_hosts"), err)]
    async fn list_due(
        &self,
        source: SourceName,
        checked_before: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<CandidateHost>, CoreError> {
        let rows = sqlx::query_as!(
            CandidateHostRow,
            r#"
            SELECT h.host, h.domain, h.origin, h.first_seen_at, h.last_seen_at
            FROM candidate_hosts h
            LEFT JOIN host_source_checks c ON c.host = h.host AND c.source = $1
            WHERE (c.checked_at IS NULL OR c.checked_at < $2)
              AND NOT EXISTS (
                  SELECT 1 FROM suppression_list s WHERE s.kind = 'domain' AND s.value = h.domain
              )
            ORDER BY c.checked_at ASC NULLS FIRST, h.host ASC
            LIMIT $3
            "#,
            source.as_str(),
            checked_before,
            i64::from(limit),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter().map(CandidateHost::try_from).collect()
    }

    #[tracing::instrument(skip(self, hosts), fields(db.system = "postgresql", db.operation = "upsert", db.table = "host_source_checks", batch = hosts.len()), err)]
    async fn mark_checked(
        &self,
        hosts: &[Hostname],
        source: SourceName,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        if hosts.is_empty() {
            return Ok(());
        }
        let names: Vec<&str> = hosts.iter().map(Hostname::as_str).collect();
        sqlx::query!(
            r#"
            INSERT INTO host_source_checks (host, source, checked_at)
            SELECT host, $2, $3 FROM UNNEST($1::text[]) AS t(host)
            ON CONFLICT (host, source) DO UPDATE SET checked_at = EXCLUDED.checked_at
            "#,
            &names as &[&str],
            source.as_str(),
            at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }
}
