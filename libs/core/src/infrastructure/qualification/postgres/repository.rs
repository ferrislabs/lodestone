use std::collections::HashMap;

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;

use crate::{
    Lead, LeadId, ProfileId, RevisionNumber,
    domain::qualification::{
        Completion, EvidenceStrength, Exclusion, Judgement, LeadScoring, QualificationResult,
        Rescoring, SCORING_VERSION, SUMMARY_ERROR_MAX_CHARS, SummaryState, VerdictReason,
        ports::QualificationRepository, summary_retry_delay,
    },
    infrastructure::{
        lead::postgres::{model::LeadRow, repository::NOT_SUPPRESSED},
        postgres::{SharedTx, error::map_sqlx_error},
        qualification::postgres::model::{ResultRow, ScoringRow},
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

fn evidence_json(evidence: &EvidenceStrength) -> Result<serde_json::Value, CoreError> {
    serde_json::to_value(evidence).map_err(|error| CoreError::Internal(error.to_string()))
}

fn exclusions_json(exclusions: &[Exclusion]) -> Result<serde_json::Value, CoreError> {
    serde_json::to_value(exclusions).map_err(|error| CoreError::Internal(error.to_string()))
}

fn reason_text(judgement: &Judgement) -> Option<&'static str> {
    judgement.reason().map(VerdictReason::as_str)
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
          OR EXISTS (
              SELECT 1 FROM company_dossiers dossier
              WHERE dossier.domain = leads.domain
                AND dossier.content_at IS NOT NULL
                AND (state.dossier_at IS NULL OR state.dossier_at < dossier.content_at)
          )
      )
      AND (
          $3::timestamptz IS NULL
          OR leads.created_at <= $3
          OR EXISTS (
              SELECT 1 FROM company_dossiers dossier
              WHERE dossier.domain = leads.domain AND dossier.state <> 'pending'
          )
      )
"#;

const SUMMARY_DUE: &str = r#"
    FROM leads
    JOIN lead_qualification_state state ON state.lead_id = leads.id
    WHERE leads.profile_id = $1
      AND state.profile_revision = $2
      AND state.summarised_at IS NULL
      AND leads.status IN ('new', 'qualified')
"#;

const STALE: &str = r#"
    FROM leads
    JOIN lead_qualification_state state ON state.lead_id = leads.id
    WHERE leads.profile_id = $1
      AND state.profile_revision = $2
      AND (
          state.scoring_version <> $3
          OR state.evidence IS NULL
          OR abs((state.evidence->>'signal_score')::float8 - leads.score) > 1e-9
          OR (state.evidence->>'signal_count')::bigint
              <> (SELECT COUNT(*) FROM signals WHERE signals.lead_id = leads.id)
      )
"#;

const SUMMARY_READY: &str =
    " AND (state.summary_next_attempt_at IS NULL OR state.summary_next_attempt_at <= $4)";

impl<'tx> QualificationRepository for PgQualificationRepository<'tx> {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id), err)]
    async fn due_leads(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
        limit: u32,
        dossier_gate: Option<DateTime<Utc>>,
    ) -> Result<Vec<Lead>, CoreError> {
        let revision = revision_number(revision)?;
        let query = format!(
            "SELECT leads.id, leads.profile_id, leads.domain, leads.status, leads.score, leads.score_breakdown, leads.confidence, leads.verdict, leads.qualified_at, leads.first_seen_at, leads.last_seen_at, leads.updated_at {DUE}{NOT_SUPPRESSED} ORDER BY leads.first_seen_at ASC, leads.id ASC LIMIT $4"
        );
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, LeadRow>(&query)
            .bind(profile_id.0)
            .bind(revision)
            .bind(dossier_gate)
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
        dossier_gate: Option<DateTime<Utc>>,
    ) -> Result<u64, CoreError> {
        let revision = revision_number(revision)?;
        let query = format!("SELECT COUNT(*) {DUE}{NOT_SUPPRESSED}");
        let mut tx = self.tx.lock().await;
        let count: i64 = sqlx::query_scalar(&query)
            .bind(profile_id.0)
            .bind(revision)
            .bind(dossier_gate)
            .fetch_one(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id), err)]
    async fn due_summaries(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
        limit: u32,
        now: DateTime<Utc>,
    ) -> Result<Vec<Lead>, CoreError> {
        let revision = revision_number(revision)?;
        let query = format!(
            "SELECT leads.id, leads.profile_id, leads.domain, leads.status, leads.score, leads.score_breakdown, leads.confidence, leads.verdict, leads.qualified_at, leads.first_seen_at, leads.last_seen_at, leads.updated_at {SUMMARY_DUE}{SUMMARY_READY}{NOT_SUPPRESSED} ORDER BY leads.first_seen_at ASC, leads.id ASC LIMIT $3"
        );
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, LeadRow>(&query)
            .bind(profile_id.0)
            .bind(revision)
            .bind(i64::from(limit))
            .bind(now)
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        rows.into_iter().map(Lead::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id), err)]
    async fn count_due_summaries(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
    ) -> Result<u64, CoreError> {
        let revision = revision_number(revision)?;
        let query = format!("SELECT COUNT(*) {SUMMARY_DUE}{NOT_SUPPRESSED}");
        let mut tx = self.tx.lock().await;
        let count: i64 = sqlx::query_scalar(&query)
            .bind(profile_id.0)
            .bind(revision)
            .fetch_one(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "lead_qualification_state", lead.id = %lead_id), err)]
    async fn mark_summarised(
        &mut self,
        lead_id: LeadId,
        qualified_at: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = sqlx::query(
            "UPDATE lead_qualification_state SET summarised_at = $3, summary_attempts = 0, summary_next_attempt_at = NULL, summary_error = NULL WHERE lead_id = $1 AND completed_at = $2 AND summarised_at IS NULL",
        )
        .bind(lead_id.0)
        .bind(qualified_at)
        .bind(at)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() == 1)
    }

    #[tracing::instrument(skip(self, reason), fields(db.system = "postgresql", db.operation = "update", db.table = "lead_qualification_state", lead.id = %lead_id), err)]
    async fn record_summary_failure(
        &mut self,
        lead_id: LeadId,
        qualified_at: DateTime<Utc>,
        reason: &str,
        at: DateTime<Utc>,
    ) -> Result<Option<u32>, CoreError> {
        let reason: String = reason.chars().take(SUMMARY_ERROR_MAX_CHARS).collect();
        let mut tx = self.tx.lock().await;
        let attempts: Option<i32> = sqlx::query_scalar(
            "UPDATE lead_qualification_state SET summary_attempts = summary_attempts + 1, summary_error = $3 WHERE lead_id = $1 AND completed_at = $2 AND summarised_at IS NULL RETURNING summary_attempts",
        )
        .bind(lead_id.0)
        .bind(qualified_at)
        .bind(&reason)
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let Some(attempts) = attempts.and_then(|value| u32::try_from(value).ok()) else {
            return Ok(None);
        };
        sqlx::query(
            "UPDATE lead_qualification_state SET summary_next_attempt_at = $2 WHERE lead_id = $1",
        )
        .bind(lead_id.0)
        .bind(at + summary_retry_delay(attempts))
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(Some(attempts))
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "lead_qualification_state", lead.id = %lead_id), err)]
    async fn reset_summary(&mut self, lead_id: LeadId) -> Result<bool, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = sqlx::query(
            "UPDATE lead_qualification_state SET summary_attempts = 0, summary_next_attempt_at = NULL, summary_error = NULL WHERE lead_id = $1",
        )
        .bind(lead_id.0)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() == 1)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "lead_qualification_state", lead.id = %lead_id), err)]
    async fn summary_state(&mut self, lead_id: LeadId) -> Result<Option<SummaryState>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row: Option<(i32, Option<String>, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT summary_attempts, summary_error, summary_next_attempt_at FROM lead_qualification_state WHERE lead_id = $1",
        )
        .bind(lead_id.0)
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.map(|(attempts, error, next_attempt_at)| SummaryState {
            attempts: u32::try_from(attempts).unwrap_or_default(),
            error,
            next_attempt_at,
        }))
    }

    #[tracing::instrument(skip(self, result), fields(db.system = "postgresql", db.operation = "upsert", db.table = "qualification_results", lead.id = %result.lead_id, agent.id = result.agent_id.as_str()), err)]
    async fn store_result(&mut self, result: &QualificationResult) -> Result<(), CoreError> {
        let revision = revision_number(result.profile_revision)?;
        let generation = serde_json::to_value(result.provenance.generation)
            .map_err(|error| CoreError::Internal(error.to_string()))?;
        let samples = i32::try_from(result.provenance.samples)
            .map_err(|_| CoreError::Internal("result samples out of range".to_owned()))?;
        let mut tx = self.tx.lock().await;
        sqlx::query(
            r#"
            INSERT INTO qualification_results (
                id, lead_id, profile_id, profile_revision, agent_id, provider_id, model,
                prompt_version, status, summary, answers, agent_score, error, created_at,
                provider_name, generation, samples, escalated, raw_samples
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
            ON CONFLICT (lead_id, profile_revision, agent_id) DO UPDATE SET
                provider_id    = EXCLUDED.provider_id,
                model          = EXCLUDED.model,
                prompt_version = EXCLUDED.prompt_version,
                provider_name  = EXCLUDED.provider_name,
                generation     = EXCLUDED.generation,
                samples        = EXCLUDED.samples,
                escalated      = EXCLUDED.escalated,
                raw_samples    = EXCLUDED.raw_samples,
                status      = EXCLUDED.status,
                summary     = EXCLUDED.summary,
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
        .bind(result.provenance.provider_id.map(|provider| provider.0))
        .bind(&result.provenance.model)
        .bind(&result.provenance.prompt_version)
        .bind(result.status.as_str())
        .bind(result.summary.as_deref())
        .bind(&result.answers)
        .bind(result.agent_score)
        .bind(result.error.as_deref())
        .bind(result.created_at)
        .bind(&result.provenance.provider_name)
        .bind(&generation)
        .bind(samples)
        .bind(result.escalated)
        .bind(&result.raw_samples)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self, completion), fields(db.system = "postgresql", db.operation = "upsert", db.table = "lead_qualification_state", lead.id = %completion.lead_id), err)]
    async fn complete(&mut self, completion: &Completion) -> Result<(), CoreError> {
        let revision = revision_number(completion.profile_revision)?;
        let escalated_revision = completion
            .escalated_revision
            .map(revision_number)
            .transpose()?;
        let mut tx = self.tx.lock().await;
        sqlx::query(
            r#"
            INSERT INTO lead_qualification_state (
                lead_id, profile_revision, sources, completed_at,
                scoring_version, opinion, evidence, verdict_reason, dossier_at, exclusions,
                escalated_revision
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (lead_id) DO UPDATE SET
                profile_revision = EXCLUDED.profile_revision,
                sources          = EXCLUDED.sources,
                completed_at     = EXCLUDED.completed_at,
                scoring_version  = EXCLUDED.scoring_version,
                opinion          = EXCLUDED.opinion,
                evidence         = EXCLUDED.evidence,
                verdict_reason   = EXCLUDED.verdict_reason,
                dossier_at       = EXCLUDED.dossier_at,
                exclusions       = EXCLUDED.exclusions,
                escalated_revision = EXCLUDED.escalated_revision,
                summarised_at    = NULL,
                summary_attempts = 0,
                summary_next_attempt_at = NULL,
                summary_error    = NULL
            "#,
        )
        .bind(completion.lead_id.0)
        .bind(revision)
        .bind(&completion.sources)
        .bind(completion.at)
        .bind(completion.scoring_version)
        .bind(completion.judgement.opinion)
        .bind(evidence_json(&completion.judgement.evidence)?)
        .bind(reason_text(&completion.judgement))
        .bind(completion.dossier_at)
        .bind(exclusions_json(&completion.judgement.exclusions)?)
        .bind(escalated_revision)
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
                   r.provider_name, r.model, r.prompt_version, r.generation, r.samples,
                   r.status, r.summary, r.answers, r.agent_score, r.error, r.escalated,
                   r.raw_samples, r.created_at
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

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id), err)]
    async fn stale_scorings(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
        limit: u32,
    ) -> Result<Vec<Lead>, CoreError> {
        let revision = revision_number(revision)?;
        let query = format!(
            "SELECT leads.id, leads.profile_id, leads.domain, leads.status, leads.score, leads.score_breakdown, leads.confidence, leads.verdict, leads.qualified_at, leads.first_seen_at, leads.last_seen_at, leads.updated_at {STALE}{NOT_SUPPRESSED} ORDER BY leads.first_seen_at ASC, leads.id ASC LIMIT $4"
        );
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, LeadRow>(&query)
            .bind(profile_id.0)
            .bind(revision)
            .bind(SCORING_VERSION)
            .bind(i64::from(limit))
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        rows.into_iter().map(Lead::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads", profile.id = %profile_id), err)]
    async fn count_stale_scorings(
        &mut self,
        profile_id: ProfileId,
        revision: RevisionNumber,
    ) -> Result<u64, CoreError> {
        let revision = revision_number(revision)?;
        let query = format!("SELECT COUNT(*) {STALE}{NOT_SUPPRESSED}");
        let mut tx = self.tx.lock().await;
        let count: i64 = sqlx::query_scalar(&query)
            .bind(profile_id.0)
            .bind(revision)
            .bind(SCORING_VERSION)
            .fetch_one(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    #[tracing::instrument(skip(self, rescoring), fields(db.system = "postgresql", db.operation = "update", db.table = "lead_qualification_state", lead.id = %rescoring.lead_id), err)]
    async fn rescore(&mut self, rescoring: &Rescoring) -> Result<(), CoreError> {
        let evidence = evidence_json(&rescoring.judgement.evidence)?;
        let exclusions = exclusions_json(&rescoring.judgement.exclusions)?;
        let mut tx = self.tx.lock().await;
        sqlx::query(
            "UPDATE lead_qualification_state SET scoring_version = $2, opinion = $3, evidence = $4, verdict_reason = $5, exclusions = $6 WHERE lead_id = $1",
        )
        .bind(rescoring.lead_id.0)
        .bind(rescoring.scoring_version)
        .bind(rescoring.judgement.opinion)
        .bind(evidence)
        .bind(reason_text(&rescoring.judgement))
        .bind(exclusions)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        sqlx::query(
            "UPDATE lead_qualification_state SET summarised_at = NULL, summary_attempts = 0, summary_next_attempt_at = NULL, summary_error = NULL WHERE lead_id = $1 AND summarised_at IS NOT NULL AND (SELECT verdict IS DISTINCT FROM $2 FROM leads WHERE id = $1)",
        )
        .bind(rescoring.lead_id.0)
        .bind(rescoring.judgement.verdict.map(|verdict| verdict.as_str()))
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        sqlx::query("UPDATE leads SET confidence = $2, verdict = $3 WHERE id = $1")
            .bind(rescoring.lead_id.0)
            .bind(rescoring.judgement.confidence)
            .bind(rescoring.judgement.verdict.map(|verdict| verdict.as_str()))
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        for (result_id, score) in &rescoring.agent_scores {
            sqlx::query("UPDATE qualification_results SET agent_score = $2 WHERE id = $1")
                .bind(result_id.0)
                .bind(score)
                .execute(&mut ***tx)
                .await
                .map_err(map_sqlx_error)?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, lead_ids), fields(db.system = "postgresql", db.operation = "select", db.table = "lead_qualification_state"), err)]
    async fn scorings(
        &mut self,
        lead_ids: &[LeadId],
    ) -> Result<HashMap<LeadId, LeadScoring>, CoreError> {
        if lead_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<uuid::Uuid> = lead_ids.iter().map(|id| id.0).collect();
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, ScoringRow>(
            "SELECT lead_id, opinion, evidence, verdict_reason, exclusions FROM lead_qualification_state WHERE lead_id = ANY($1) AND evidence IS NOT NULL",
        )
        .bind(&ids)
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter()
            .map(|row| {
                let id = LeadId(row.lead_id);
                LeadScoring::try_from(row).map(|scoring| (id, scoring))
            })
            .collect()
    }
}
