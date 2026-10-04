use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;

use crate::{
    Lead, LeadId, LeadStatus, NormalizedDomain, ProfileId, RevisionNumber, ScoreBreakdown,
    domain::lead::ports::LeadRepository,
    infrastructure::{
        lead::postgres::model::LeadRow,
        postgres::{SharedTx, error::map_sqlx_error},
    },
};

#[repository(domain = Lead, backend = Postgres)]
pub struct PgLeadRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgLeadRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> LeadRepository for PgLeadRepository<'tx> {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "upsert", db.table = "leads", profile.id = %profile_id, lead.domain = %domain), err)]
    async fn record_seen(
        &mut self,
        profile_id: ProfileId,
        domain: &NormalizedDomain,
        seen_at: DateTime<Utc>,
    ) -> Result<Lead, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            LeadRow,
            r#"
            INSERT INTO leads (id, profile_id, domain, status, score, first_seen_at, last_seen_at, updated_at)
            VALUES ($1, $2, $3, 'new', 0, $4, $4, $4)
            ON CONFLICT (profile_id, domain) DO UPDATE SET
                last_seen_at  = GREATEST(leads.last_seen_at, EXCLUDED.last_seen_at),
                first_seen_at = LEAST(leads.first_seen_at, EXCLUDED.first_seen_at),
                updated_at    = EXCLUDED.updated_at
            RETURNING id, profile_id, domain, status, score, score_breakdown, first_seen_at, last_seen_at, updated_at
            "#,
            common::generate_uuid_v7(),
            profile_id.0,
            domain.as_str(),
            seen_at,
        )
        .fetch_one(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Lead::try_from(row)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", lead.id = %id), err)]
    async fn find(&mut self, id: LeadId) -> Result<Option<Lead>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            LeadRow,
            r#"
            SELECT id, profile_id, domain, status, score, score_breakdown, first_seen_at, last_seen_at, updated_at
            FROM leads
            WHERE id = $1
            "#,
            id.0,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(Lead::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id, lead.domain = %domain), err)]
    async fn find_by_domain(
        &mut self,
        profile_id: ProfileId,
        domain: &NormalizedDomain,
    ) -> Result<Option<Lead>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            LeadRow,
            r#"
            SELECT id, profile_id, domain, status, score, score_breakdown, first_seen_at, last_seen_at, updated_at
            FROM leads
            WHERE profile_id = $1 AND domain = $2
            "#,
            profile_id.0,
            domain.as_str(),
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(Lead::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads"), err)]
    async fn list_by_domain(&mut self, domain: &NormalizedDomain) -> Result<Vec<Lead>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            LeadRow,
            r#"
            SELECT id, profile_id, domain, status, score, score_breakdown, first_seen_at, last_seen_at, updated_at
            FROM leads
            WHERE domain = $1
            ORDER BY profile_id
            "#,
            domain.as_str(),
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter().map(Lead::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "leads", lead.id = %id), err)]
    async fn update_status(
        &mut self,
        id: LeadId,
        from: LeadStatus,
        to: LeadStatus,
        at: DateTime<Utc>,
    ) -> Result<Option<Lead>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            LeadRow,
            r#"
            UPDATE leads
            SET status = $3, updated_at = $4
            WHERE id = $1 AND status = $2
            RETURNING id, profile_id, domain, status, score, score_breakdown, first_seen_at, last_seen_at, updated_at
            "#,
            id.0,
            from.as_str(),
            to.as_str(),
            at,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(Lead::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "leads", lead.id = %id), err)]
    async fn update_score(
        &mut self,
        id: LeadId,
        breakdown: &ScoreBreakdown,
        at: DateTime<Utc>,
    ) -> Result<Option<Lead>, CoreError> {
        let contributions = serde_json::to_value(&breakdown.contributions)
            .map_err(|_| CoreError::Internal("score breakdown is not serializable".to_owned()))?;
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            LeadRow,
            r#"
            UPDATE leads
            SET score = $2, score_breakdown = $4, updated_at = $3
            WHERE id = $1
            RETURNING id, profile_id, domain, status, score, score_breakdown, first_seen_at, last_seen_at, updated_at
            "#,
            id.0,
            breakdown.total.get(),
            at,
            contributions,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(Lead::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id), err)]
    async fn list_by_profile(&mut self, profile_id: ProfileId) -> Result<Vec<Lead>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            LeadRow,
            r#"
            SELECT id, profile_id, domain, status, score, score_breakdown, first_seen_at, last_seen_at, updated_at
            FROM leads
            WHERE profile_id = $1
            ORDER BY score DESC, domain ASC
            "#,
            profile_id.0,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(Lead::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id), err)]
    async fn list_to_qualify(
        &mut self,
        profile_id: ProfileId,
        threshold: f64,
        revision: RevisionNumber,
        prompt: &str,
        limit: u32,
    ) -> Result<Vec<Lead>, CoreError> {
        let revision = i32::try_from(revision.get())
            .map_err(|_| CoreError::Internal("profile revision number out of range".to_owned()))?;
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            LeadRow,
            r#"
            SELECT id, profile_id, domain, status, score, score_breakdown, first_seen_at, last_seen_at, updated_at
            FROM leads
            WHERE profile_id = $1
              AND score >= $2
              AND status = 'new'
              AND NOT EXISTS (
                  SELECT 1 FROM assessments
                  WHERE assessments.lead_id = leads.id
                    AND assessments.profile_revision = $3
                    AND assessments.prompt_version = $4
              )
            ORDER BY score DESC, domain ASC
            LIMIT $5
            "#,
            profile_id.0,
            threshold,
            revision,
            prompt,
            i64::from(limit),
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(Lead::try_from).collect()
    }
}
