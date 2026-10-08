use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use serde_json::to_value;
use sqlx::query_scalar;

use crate::{
    LeadId, NormalizedDomain,
    domain::company::{
        CompanyDossier, CompanyEntity, DossierState, SiblingLead, ports::CompanyRepository,
    },
    infrastructure::{
        company::postgres::model::{DossierRow, EntityRow, SiblingRow},
        lead::postgres::repository::NOT_SUPPRESSED,
        postgres::{SharedTx, error::map_sqlx_error},
    },
};

#[repository(domain = Company, backend = Postgres)]
pub struct PgCompanyRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgCompanyRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

const DUE: &str = r#"
    FROM (
        SELECT DISTINCT leads.domain
        FROM leads
        JOIN profiles ON profiles.id = leads.profile_id AND profiles.active
        WHERE TRUE
"#;

const DUE_TAIL: &str = r#"
    ) l
    LEFT JOIN company_dossiers d ON d.domain = l.domain
    WHERE d.domain IS NULL OR d.next_attempt_at <= $1
"#;

const UNCHECKED: &str = r#"
    FROM company_dossiers d
    WHERE d.pages_checked_at IS NULL
      AND jsonb_array_length(d.pages) > 0
      AND NOT EXISTS (
          SELECT 1 FROM suppression_list sup
          WHERE sup.kind = 'domain'
            AND (sup.value = d.domain OR right(d.domain, length(sup.value) + 1) = '.' || sup.value)
      )
"#;

fn ignored_links(dossier: &CompanyDossier) -> i32 {
    i32::try_from(dossier.ignored_links).unwrap_or(i32::MAX)
}

impl<'tx> CompanyRepository for PgCompanyRepository<'tx> {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "company_dossiers"), err)]
    async fn unchecked_domains(&mut self, limit: u32) -> Result<Vec<NormalizedDomain>, CoreError> {
        let query = format!("SELECT d.domain {UNCHECKED} ORDER BY d.domain LIMIT $1");
        let mut tx = self.tx.lock().await;
        let rows: Vec<String> = sqlx::query_scalar(&query)
            .bind(i64::from(limit))
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        rows.iter()
            .map(|domain| {
                NormalizedDomain::from_host(domain)
                    .map_err(|_| CoreError::Internal("stored dossier domain is invalid".to_owned()))
            })
            .collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "company_dossiers"), err)]
    async fn domains_except(
        &mut self,
        state: DossierState,
    ) -> Result<Vec<NormalizedDomain>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows: Vec<String> =
            query_scalar("SELECT domain FROM company_dossiers WHERE state <> $1 ORDER BY domain")
                .bind(state.as_str())
                .fetch_all(&mut ***tx)
                .await
                .map_err(map_sqlx_error)?;
        Ok(rows
            .iter()
            .filter_map(|domain| NormalizedDomain::from_host(domain).ok())
            .collect())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "company_dossiers"), err)]
    async fn count_unchecked(&mut self) -> Result<u64, CoreError> {
        let query = format!("SELECT COUNT(*) {UNCHECKED}");
        let mut tx = self.tx.lock().await;
        let count: i64 = sqlx::query_scalar(&query)
            .fetch_one(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    #[tracing::instrument(skip(self, dossier, built_at), fields(db.system = "postgresql", db.operation = "update", db.table = "company_dossiers", domain = %dossier.domain.as_str()), err)]
    async fn save_review(
        &mut self,
        dossier: &CompanyDossier,
        built_at: Option<DateTime<Utc>>,
    ) -> Result<bool, CoreError> {
        let pages =
            to_value(&dossier.pages).map_err(|error| CoreError::Internal(error.to_string()))?;
        let mut tx = self.tx.lock().await;
        let result = sqlx::query(
            r#"
            UPDATE company_dossiers SET
                pages            = $2,
                content_hash     = $3,
                content_at       = $4,
                ignored_links    = $5,
                pages_checked_at = $6
            WHERE domain = $1
              AND pages_checked_at IS NULL
              AND built_at IS NOT DISTINCT FROM $7
            "#,
        )
        .bind(dossier.domain.as_str())
        .bind(pages)
        .bind(dossier.content_hash.as_deref())
        .bind(dossier.content_at)
        .bind(ignored_links(dossier))
        .bind(dossier.pages_checked_at)
        .bind(built_at)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() == 1)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "company_dossiers"), err)]
    async fn due_domains(
        &mut self,
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<NormalizedDomain>, CoreError> {
        let query = format!(
            "SELECT l.domain {DUE}{NOT_SUPPRESSED}{DUE_TAIL} ORDER BY d.next_attempt_at NULLS FIRST, l.domain LIMIT $2"
        );
        let mut tx = self.tx.lock().await;
        let rows: Vec<String> = sqlx::query_scalar(&query)
            .bind(now)
            .bind(i64::from(limit))
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        rows.iter()
            .map(|domain| {
                NormalizedDomain::from_host(domain)
                    .map_err(|_| CoreError::Internal("stored lead domain is invalid".to_owned()))
            })
            .collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "company_dossiers"), err)]
    async fn count_due(&mut self, now: DateTime<Utc>) -> Result<u64, CoreError> {
        let query = format!("SELECT COUNT(*) {DUE}{NOT_SUPPRESSED}{DUE_TAIL}");
        let mut tx = self.tx.lock().await;
        let count: i64 = sqlx::query_scalar(&query)
            .bind(now)
            .fetch_one(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "company_dossiers", domain = %domain.as_str()), err)]
    async fn find(
        &mut self,
        domain: &NormalizedDomain,
    ) -> Result<Option<CompanyDossier>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as::<_, DossierRow>(
            "SELECT domain, state, siren, official, pages, content_hash, content_at, built_at, attempts, next_attempt_at, error, ignored_links, pages_checked_at FROM company_dossiers WHERE domain = $1",
        )
        .bind(domain.as_str())
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        row.map(CompanyDossier::try_from).transpose()
    }

    #[tracing::instrument(skip(self, dossier), fields(db.system = "postgresql", db.operation = "upsert", db.table = "company_dossiers", domain = %dossier.domain.as_str()), err)]
    async fn save(&mut self, dossier: &CompanyDossier) -> Result<(), CoreError> {
        let official = dossier
            .official
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| CoreError::Internal(error.to_string()))?;
        let pages = serde_json::to_value(&dossier.pages)
            .map_err(|error| CoreError::Internal(error.to_string()))?;
        let attempts = i32::try_from(dossier.attempts).unwrap_or(i32::MAX);
        let company = dossier
            .official
            .as_ref()
            .map(|official| (official.siren.as_str(), official.legal_name.as_str()));
        let mut tx = self.tx.lock().await;
        if let Some((siren, legal_name)) = company {
            sqlx::query(
                r#"
                INSERT INTO companies (siren, legal_name)
                VALUES ($1, $2)
                ON CONFLICT (siren) DO UPDATE SET
                    legal_name = EXCLUDED.legal_name,
                    updated_at = now()
                "#,
            )
            .bind(siren)
            .bind(legal_name)
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        }
        sqlx::query(
            r#"
            INSERT INTO company_dossiers (
                domain, state, siren, official, pages, content_hash, content_at,
                built_at, attempts, next_attempt_at, error, company_siren,
                ignored_links, pages_checked_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            ON CONFLICT (domain) DO UPDATE SET
                state           = EXCLUDED.state,
                siren           = EXCLUDED.siren,
                official        = EXCLUDED.official,
                pages           = EXCLUDED.pages,
                content_hash    = EXCLUDED.content_hash,
                content_at      = EXCLUDED.content_at,
                built_at        = EXCLUDED.built_at,
                attempts        = EXCLUDED.attempts,
                next_attempt_at = EXCLUDED.next_attempt_at,
                error           = EXCLUDED.error,
                company_siren   = EXCLUDED.company_siren,
                ignored_links   = EXCLUDED.ignored_links,
                pages_checked_at = EXCLUDED.pages_checked_at
            "#,
        )
        .bind(dossier.domain.as_str())
        .bind(dossier.state.as_str())
        .bind(dossier.siren.as_ref().map(|siren| siren.as_str()))
        .bind(official)
        .bind(pages)
        .bind(dossier.content_hash.as_deref())
        .bind(dossier.content_at)
        .bind(dossier.built_at)
        .bind(attempts)
        .bind(dossier.next_attempt_at)
        .bind(dossier.error.as_deref())
        .bind(company.map(|(siren, _)| siren))
        .bind(ignored_links(dossier))
        .bind(dossier.pages_checked_at)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "upsert", db.table = "company_dossiers", domain = %domain.as_str()), err)]
    async fn request_refresh(
        &mut self,
        domain: &NormalizedDomain,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut tx = self.tx.lock().await;
        sqlx::query(
            r#"
            INSERT INTO company_dossiers (domain, state, pages, next_attempt_at, created_at)
            VALUES ($1, 'pending', '[]'::jsonb, $2, $2)
            ON CONFLICT (domain) DO UPDATE SET
                next_attempt_at = LEAST(company_dossiers.next_attempt_at, EXCLUDED.next_attempt_at),
                attempts        = 0
            "#,
        )
        .bind(domain.as_str())
        .bind(now)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "companies", domain = %domain.as_str()), err)]
    async fn entity_of(
        &mut self,
        domain: &NormalizedDomain,
    ) -> Result<Option<CompanyEntity>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as::<_, EntityRow>(
            "SELECT companies.siren, companies.legal_name FROM company_dossiers d JOIN companies ON companies.siren = d.company_siren WHERE d.domain = $1",
        )
        .bind(domain.as_str())
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        row.map(CompanyEntity::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", lead.id = %lead_id), err)]
    async fn siblings(&mut self, lead_id: LeadId) -> Result<Vec<SiblingLead>, CoreError> {
        let query = format!(
            "SELECT leads.id, leads.domain, leads.profile_id, profiles.name AS profile_name, leads.verdict, leads.confidence \
             FROM leads origin \
             JOIN company_dossiers origin_dossier ON origin_dossier.domain = origin.domain \
             JOIN company_dossiers sibling_dossier ON sibling_dossier.company_siren = origin_dossier.company_siren \
             JOIN leads ON leads.domain = sibling_dossier.domain AND leads.id <> origin.id \
             JOIN profiles ON profiles.id = leads.profile_id \
             WHERE origin.id = $1 AND origin_dossier.company_siren IS NOT NULL{NOT_SUPPRESSED} \
             ORDER BY (leads.profile_id = origin.profile_id) DESC, leads.confidence DESC NULLS LAST, leads.domain ASC, leads.id ASC"
        );
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, SiblingRow>(&query)
            .bind(lead_id.0)
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        rows.into_iter().map(SiblingLead::try_from).collect()
    }
}
