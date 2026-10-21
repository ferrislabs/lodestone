use common::CoreError;
use lodestone_macros::repository;

use crate::{
    Lead, LeadId, ProfileId, RevisionNumber,
    domain::qualification::{Completion, QualificationResult, ports::QualificationRepository},
    infrastructure::{
        lead::postgres::{model::LeadRow, repository::NOT_SUPPRESSED},
        postgres::{SharedTx, error::map_sqlx_error},
        qualification::postgres::model::ResultRow,
    },
};

#[repository(domain = Qualification, backend = Postgres)]
pub struct PgQualificationRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgQualificationRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

fn revision_number(revision: RevisionNumber) -> Result<i32, CoreError> {
    i32::try_from(revision.get())
        .map_err(|_| CoreError::Internal("profile revision number out of range".to_owned()))
}

const DUE: &str = r#"
    FROM leads
    LEFT JOIN lead_qualification_state state ON state.lead_id = leads.id
    WHERE leads.profile_id = $1
      AND EXISTS (SELECT 1 FROM signals WHERE signals.lead_id = leads.id)
      AND (
          state.lead_id IS NULL
          OR state.profile_revision <> $2
          OR EXISTS (
              SELECT 1 FROM signals
              WHERE signals.lead_id = leads.id AND NOT (signals.source = ANY (state.sources))
          )
      )
"#;

impl<'tx> QualificationRepository for PgQualificationRepository<'tx> {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id), err)]
    async fn due_leads(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
        limit: u32,
    ) -> Result<Vec<Lead>, CoreError> {
        let revision = revision_number(revision)?;
        let query = format!(
            "SELECT leads.id, leads.profile_id, leads.domain, leads.status, leads.score, leads.score_breakdown, leads.confidence, leads.verdict, leads.qualified_at, leads.first_seen_at, leads.last_seen_at, leads.updated_at {DUE}{NOT_SUPPRESSED} ORDER BY leads.first_seen_at ASC, leads.id ASC LIMIT $3"
        );
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, LeadRow>(&query)
            .bind(profile_id.0)
            .bind(revision)
            .bind(i64::from(limit))
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        rows.into_iter().map(Lead::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id), err)]
    async fn count_due(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
    ) -> Result<u64, CoreError> {
        let revision = revision_number(revision)?;
        let query = format!("SELECT COUNT(*) {DUE}{NOT_SUPPRESSED}");
        let mut tx = self.tx.lock().await;
        let count: i64 = sqlx::query_scalar(&query)
            .bind(profile_id.0)
            .bind(revision)
            .fetch_one(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    #[tracing::instrument(skip(self, result), fields(db.system = "postgresql", db.operation = "upsert", db.table = "qualification_results", lead.id = %result.lead_id, agent.id = result.agent_id.as_str()), err)]
    async fn store_result(&mut self, result: &QualificationResult) -> Result<(), CoreError> {
        let revision = revision_number(result.profile_revision)?;
        let mut tx = self.tx.lock().await;
        sqlx::query(
            r#"
            INSERT INTO qualification_results (
                id, lead_id, profile_id, profile_revision, agent_id, provider_id, model,
                prompt_hash, status, answers, agent_score, error, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (lead_id, profile_revision, agent_id) DO UPDATE SET
                provider_id = EXCLUDED.provider_id,
                model       = EXCLUDED.model,
                prompt_hash = EXCLUDED.prompt_hash,
                status      = EXCLUDED.status,
                answers     = EXCLUDED.answers,
                agent_score = EXCLUDED.agent_score,
                error       = EXCLUDED.error,
                created_at  = EXCLUDED.created_at
            "#,
        )
        .bind(result.id.0)
        .bind(result.lead_id.0)
        .bind(result.profile_id.0)
        .bind(revision)
        .bind(result.agent_id.as_str())
        .bind(result.provider_id.map(|provider| provider.0))
        .bind(&result.model)
        .bind(&result.prompt_hash)
        .bind(result.status.as_str())
        .bind(&result.answers)
        .bind(result.agent_score)
        .bind(result.error.as_deref())
        .bind(result.created_at)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self, completion), fields(db.system = "postgresql", db.operation = "upsert", db.table = "lead_qualification_state", lead.id = %completion.lead_id), err)]
    async fn complete(&mut self, completion: &Completion) -> Result<(), CoreError> {
        let revision = revision_number(completion.profile_revision)?;
        let mut tx = self.tx.lock().await;
        sqlx::query(
            r#"
            INSERT INTO lead_qualification_state (lead_id, profile_revision, sources, completed_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (lead_id) DO UPDATE SET
                profile_revision = EXCLUDED.profile_revision,
                sources          = EXCLUDED.sources,
                completed_at     = EXCLUDED.completed_at
            "#,
        )
        .bind(completion.lead_id.0)
        .bind(revision)
        .bind(&completion.sources)
        .bind(completion.at)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        sqlx::query(
            "UPDATE leads SET confidence = $2, verdict = $3, qualified_at = $4 WHERE id = $1",
        )
        .bind(completion.lead_id.0)
        .bind(completion.judgement.confidence)
        .bind(completion.judgement.verdict.map(|verdict| verdict.as_str()))
        .bind(completion.at)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "qualification_results", lead.id = %lead_id), err)]
    async fn current_results(
        &mut self,
        lead_id: LeadId,
    ) -> Result<Vec<QualificationResult>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, ResultRow>(
            r#"
            SELECT r.id, r.lead_id, r.profile_id, r.profile_revision, r.agent_id, r.provider_id,
                   r.model, r.prompt_hash, r.status, r.answers, r.agent_score, r.error, r.created_at
            FROM qualification_results r
            JOIN lead_qualification_state s
              ON s.lead_id = r.lead_id AND s.profile_revision = r.profile_revision
            WHERE r.lead_id = $1
            ORDER BY r.agent_id ASC
            "#,
        )
        .bind(lead_id.0)
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter()
            .map(QualificationResult::try_from)
            .collect()
    }
}
