use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;

use crate::{
    NormalizedDomain,
    domain::company::{CompanyDossier, ports::CompanyRepository},
    infrastructure::{
        company::postgres::model::DossierRow,
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

impl<'tx> CompanyRepository for PgCompanyRepository<'tx> {
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
            "SELECT domain, state, siren, official, pages, content_hash, content_at, built_at, attempts, next_attempt_at, error FROM company_dossiers WHERE domain = $1",
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
        let mut tx = self.tx.lock().await;
        sqlx::query(
            r#"
            INSERT INTO company_dossiers (
                domain, state, siren, official, pages, content_hash, content_at,
                built_at, attempts, next_attempt_at, error
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
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
                error           = EXCLUDED.error
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
}
