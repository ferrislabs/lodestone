use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;

use crate::{
    Lead, LeadId, ProfileId, RevisionNumber,
    domain::scoring_agent::{
        Decision, ScoringResult,
        ports::{ScoringCompletion, ScoringRepository},
    },
    infrastructure::{
        lead::postgres::{model::LeadRow, repository::NOT_SUPPRESSED},
        postgres::{SharedTx, error::map_sqlx_error},
        scoring_agent::postgres::model::ScoringResultRow,
    },
};

#[repository(domain = ScoringAgent, backend = Postgres)]
pub struct PgScoringRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgScoringRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

fn revision_number(revision: RevisionNumber) -> Result<i32, CoreError> {
    i32::try_from(revision.get())
        .map_err(|_| CoreError::Internal("profile revision number out of range".to_owned()))
}

fn score_value(score: Option<u8>) -> Option<i16> {
    score.map(i16::from)
}

fn probabilities_json(decision: &Decision) -> Option<serde_json::Value> {
    decision
        .probabilities
        .as_ref()
        .map(|probabilities| probabilities.to_json())
}

const UPDATE_LEAD_DECISION: &str = "UPDATE leads SET confidence = $2, verdict = $3, llm_score = $4, reliable_probability = $5 WHERE id = $1";

const SCORING_DUE: &str = r#"
    FROM leads
    JOIN lead_qualification_state state ON state.lead_id = leads.id
    WHERE leads.profile_id = $1
      AND state.profile_revision = $2
      AND (
          state.scoring_revision IS DISTINCT FROM state.profile_revision
          OR state.scoring_for IS DISTINCT FROM state.completed_at
      )
      AND NOT EXISTS (
          SELECT 1 FROM lead_agent_retries retry
          WHERE retry.lead_id = leads.id
            AND retry.kind = 'scoring'
            AND retry.profile_revision = $2
            AND retry.next_attempt_at > $3
      )
"#;

impl<'tx> ScoringRepository for PgScoringRepository<'tx> {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id), err)]
    async fn due_leads(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
        limit: u32,
        now: DateTime<Utc>,
    ) -> Result<Vec<Lead>, CoreError> {
        let revision = revision_number(revision)?;
        let query = format!(
            "SELECT leads.id, leads.profile_id, leads.domain, leads.status, leads.score, leads.confidence, leads.verdict, leads.llm_score, leads.qualified_at, leads.first_seen_at, leads.last_seen_at, leads.updated_at {SCORING_DUE}{NOT_SUPPRESSED} ORDER BY leads.first_seen_at ASC, leads.id ASC LIMIT $4"
        );
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, LeadRow>(&query)
            .bind(profile_id.0)
            .bind(revision)
            .bind(now)
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
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let revision = revision_number(revision)?;
        let query = format!("SELECT COUNT(*) {SCORING_DUE}{NOT_SUPPRESSED}");
        let mut tx = self.tx.lock().await;
        let count: i64 = sqlx::query_scalar(&query)
            .bind(profile_id.0)
            .bind(revision)
            .bind(now)
            .fetch_one(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    #[tracing::instrument(skip(self, result), fields(db.system = "postgresql", db.operation = "upsert", db.table = "scoring_results", lead.id = %result.lead_id, agent.id = result.agent_id.as_str()), err)]
    async fn store_result(&mut self, result: &ScoringResult) -> Result<(), CoreError> {
        let revision = revision_number(result.profile_revision)?;
        let generation = serde_json::to_value(result.provenance.generation)
            .map_err(|error| CoreError::Internal(error.to_string()))?;
        let samples = i32::try_from(result.provenance.samples)
            .map_err(|_| CoreError::Internal("result samples out of range".to_owned()))?;
        let verdict = result.verdict.as_ref().map(|verdict| verdict.to_json());
        let reasons = serde_json::to_value(&result.reasons)
            .map_err(|error| CoreError::Internal(error.to_string()))?;
        let mut tx = self.tx.lock().await;
        sqlx::query(
            r#"
            INSERT INTO scoring_results (
                id, lead_id, profile_id, profile_revision, agent_id, provider_id, provider_name,
                model, prompt_version, generation, samples, status, criteria, score, verdict,
                reasons, error, raw_samples, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
            ON CONFLICT (lead_id, profile_revision, agent_id) DO UPDATE SET
                provider_id    = EXCLUDED.provider_id,
                provider_name  = EXCLUDED.provider_name,
                model          = EXCLUDED.model,
                prompt_version = EXCLUDED.prompt_version,
                generation     = EXCLUDED.generation,
                samples        = EXCLUDED.samples,
                status         = EXCLUDED.status,
                criteria       = EXCLUDED.criteria,
                score          = EXCLUDED.score,
                verdict        = EXCLUDED.verdict,
                reasons        = EXCLUDED.reasons,
                error          = EXCLUDED.error,
                raw_samples    = EXCLUDED.raw_samples,
                created_at     = EXCLUDED.created_at
            "#,
        )
        .bind(result.id.0)
        .bind(result.lead_id.0)
        .bind(result.profile_id.0)
        .bind(revision)
        .bind(result.agent_id.as_str())
        .bind(result.provenance.provider_id.map(|provider| provider.0))
        .bind(&result.provenance.provider_name)
        .bind(&result.provenance.model)
        .bind(&result.provenance.prompt_version)
        .bind(&generation)
        .bind(samples)
        .bind(result.status.as_str())
        .bind(&result.criteria)
        .bind(score_value(result.score))
        .bind(&verdict)
        .bind(&reasons)
        .bind(result.error.as_deref())
        .bind(&result.raw_samples)
        .bind(result.created_at)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self, completion), fields(db.system = "postgresql", db.operation = "update", db.table = "lead_qualification_state", lead.id = %completion.lead_id), err)]
    async fn complete(&mut self, completion: &ScoringCompletion) -> Result<bool, CoreError> {
        let revision = revision_number(completion.profile_revision)?;
        let decision = &completion.decision;
        let mut tx = self.tx.lock().await;
        let result = sqlx::query(
            "UPDATE lead_qualification_state SET scoring_revision = $2, scoring_for = $3, verdict_source = 'scoring', verdict_reason = $4, verdict_probabilities = $5, summarised_at = NULL, summary_attempts = 0, summary_next_attempt_at = NULL, summary_error = NULL WHERE lead_id = $1 AND profile_revision = $2 AND completed_at = $3",
        )
        .bind(completion.lead_id.0)
        .bind(revision)
        .bind(completion.qualified_at)
        .bind(decision.reason_text())
        .bind(probabilities_json(decision))
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        if result.rows_affected() != 1 {
            return Ok(false);
        }
        sqlx::query(UPDATE_LEAD_DECISION)
            .bind(completion.lead_id.0)
            .bind(decision.confidence)
            .bind(decision.verdict.map(|verdict| verdict.as_str()))
            .bind(score_value(decision.score))
            .bind(decision.reliable_probability())
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        sqlx::query("DELETE FROM lead_agent_retries WHERE lead_id = $1 AND kind = 'scoring'")
            .bind(completion.lead_id.0)
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(true)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "scoring_results", lead.id = %lead_id), err)]
    async fn current_results(&mut self, lead_id: LeadId) -> Result<Vec<ScoringResult>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, ScoringResultRow>(
            r#"
            SELECT r.id, r.lead_id, r.profile_id, r.profile_revision, r.agent_id, r.provider_id,
                   r.provider_name, r.model, r.prompt_version, r.generation, r.samples,
                   r.status, r.criteria, r.score, r.verdict, r.reasons, r.error,
                   r.raw_samples, r.created_at
            FROM scoring_results r
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
        rows.into_iter().map(ScoringResult::try_from).collect()
    }

    #[tracing::instrument(skip(self, decision), fields(db.system = "postgresql", db.operation = "update", db.table = "leads", lead.id = %lead_id), err)]
    async fn apply_decision(
        &mut self,
        lead_id: LeadId,
        decision: &Decision,
    ) -> Result<(), CoreError> {
        let verdict = decision.verdict.map(|verdict| verdict.as_str());
        let mut tx = self.tx.lock().await;
        sqlx::query(
            "UPDATE lead_qualification_state SET summarised_at = NULL, summary_attempts = 0, summary_next_attempt_at = NULL, summary_error = NULL WHERE lead_id = $1 AND summarised_at IS NOT NULL AND (SELECT verdict IS DISTINCT FROM $2 FROM leads WHERE id = $1)",
        )
        .bind(lead_id.0)
        .bind(verdict)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        sqlx::query(
            "UPDATE lead_qualification_state SET verdict_reason = $2, verdict_probabilities = $3 WHERE lead_id = $1",
        )
        .bind(lead_id.0)
        .bind(decision.reason_text())
        .bind(probabilities_json(decision))
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        sqlx::query(UPDATE_LEAD_DECISION)
            .bind(lead_id.0)
            .bind(decision.confidence)
            .bind(verdict)
            .bind(score_value(decision.score))
            .bind(decision.reliable_probability())
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }
}
