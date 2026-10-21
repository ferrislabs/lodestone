use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use uuid::Uuid;

use crate::{
    LeadId, ProfileId, RevisionNumber, SourceName,
    definition::AgentId,
    domain::agent_work::{
        CancelReason, WorkKind, WorkTarget,
        ports::{AgentStop, AgentWorkRepository, KeptAgents},
    },
    infrastructure::postgres::{SharedTx, error::map_sqlx_error},
    sources::SOURCE_JOB_KIND,
};

#[repository(domain = AgentWork, backend = Postgres)]
pub struct PgAgentWorkRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgAgentWorkRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

fn revision_number(revision: RevisionNumber) -> Result<i32, CoreError> {
    i32::try_from(revision.get())
        .map_err(|_| CoreError::Internal("profile revision number out of range".to_owned()))
}

#[derive(sqlx::FromRow)]
struct StopRow {
    lead_id: Uuid,
    kind: String,
    agent_id: String,
    profile_revision: i32,
    reason: String,
    stopped_at: DateTime<Utc>,
}

impl TryFrom<StopRow> for AgentStop {
    type Error = CoreError;

    fn try_from(row: StopRow) -> Result<Self, CoreError> {
        let kind = WorkKind::parse(&row.kind)
            .ok_or_else(|| CoreError::Internal("stored agent stop kind is invalid".to_owned()))?;
        let reason = CancelReason::parse(&row.reason)
            .ok_or_else(|| CoreError::Internal("stored agent stop reason is invalid".to_owned()))?;
        let revision = u32::try_from(row.profile_revision)
            .ok()
            .and_then(RevisionNumber::new)
            .ok_or_else(|| {
                CoreError::Internal("profile revision number out of range".to_owned())
            })?;
        Ok(Self {
            lead_id: LeadId(row.lead_id),
            target: WorkTarget::new(kind, &AgentId::new(row.agent_id)),
            revision,
            reason,
            stopped_at: row.stopped_at,
        })
    }
}

const PROFILE_LEADS: &str = "SELECT id FROM leads WHERE profile_id = $1";
const CARRIED: &str = "SELECT state.lead_id FROM lead_qualification_state state JOIN leads ON leads.id = state.lead_id WHERE leads.profile_id = $1 AND state.profile_revision = $2";

impl<'tx> AgentWorkRepository for PgAgentWorkRepository<'tx> {
    #[tracing::instrument(skip(self, kept), fields(db.system = "postgresql", db.operation = "delete", db.table = "lead_agent_retries", profile.id = %profile), err)]
    async fn clear_retries(
        &mut self,
        profile: ProfileId,
        kept: &KeptAgents,
    ) -> Result<u64, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = sqlx::query(&format!(
            r#"
            DELETE FROM lead_agent_retries retry
            WHERE retry.lead_id IN ({PROFILE_LEADS})
              AND NOT (
                  (retry.kind = 'qualification' AND retry.agent_id = ANY ($2))
                  OR (retry.kind = 'scoring' AND retry.agent_id = ANY ($3))
              )
            "#
        ))
        .bind(profile.0)
        .bind(&kept.qualification)
        .bind(&kept.scoring)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(skip(self, kept), fields(db.system = "postgresql", db.operation = "update", db.table = "lead_qualification_state", profile.id = %profile), err)]
    async fn carry_forward(
        &mut self,
        profile: ProfileId,
        from: RevisionNumber,
        to: RevisionNumber,
        kept: &KeptAgents,
    ) -> Result<u64, CoreError> {
        let from = revision_number(from)?;
        let to = revision_number(to)?;
        let mut tx = self.tx.lock().await;
        for (table, agents) in [
            ("qualification_results", &kept.qualification),
            ("scoring_results", &kept.scoring),
        ] {
            sqlx::query(&format!(
                r#"
                DELETE FROM {table} result
                WHERE result.lead_id IN ({CARRIED})
                  AND result.profile_revision = $2
                  AND NOT (result.agent_id = ANY ($3))
                "#
            ))
            .bind(profile.0)
            .bind(from)
            .bind(agents)
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
            sqlx::query(&format!(
                r#"
                UPDATE {table} result
                SET profile_revision = $3
                WHERE result.lead_id IN ({CARRIED})
                  AND result.profile_revision = $2
                  AND NOT EXISTS (
                      SELECT 1 FROM {table} newer
                      WHERE newer.lead_id = result.lead_id
                        AND newer.profile_revision = $3
                        AND newer.agent_id = result.agent_id
                  )
                "#
            ))
            .bind(profile.0)
            .bind(from)
            .bind(to)
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        }
        sqlx::query(&format!(
            r#"
            UPDATE lead_agent_retries retry
            SET profile_revision = $3
            WHERE retry.lead_id IN ({CARRIED}) AND retry.profile_revision = $2
            "#
        ))
        .bind(profile.0)
        .bind(from)
        .bind(to)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let moved = sqlx::query(
            r#"
            UPDATE lead_qualification_state state
            SET profile_revision = $3,
                scoring_revision = CASE WHEN state.scoring_revision = $2 THEN $3 ELSE state.scoring_revision END,
                escalated_revision = CASE WHEN state.escalated_revision = $2 THEN $3 ELSE state.escalated_revision END,
                scored_at = NULL
            FROM leads
            WHERE leads.id = state.lead_id
              AND leads.profile_id = $1
              AND state.profile_revision = $2
            "#,
        )
        .bind(profile.0)
        .bind(from)
        .bind(to)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(moved.rows_affected())
    }

    #[tracing::instrument(skip(self, stop), fields(db.system = "postgresql", db.operation = "upsert", db.table = "lead_agent_stops", lead.id = %stop.lead_id), err)]
    async fn record_stop(&mut self, stop: &AgentStop) -> Result<(), CoreError> {
        let revision = revision_number(stop.revision)?;
        let mut tx = self.tx.lock().await;
        sqlx::query(
            r#"
            INSERT INTO lead_agent_stops (lead_id, kind, agent_id, profile_revision, reason, stopped_at)
            SELECT $1, $2, $3, $4, $5, $6
            WHERE EXISTS (SELECT 1 FROM leads WHERE id = $1)
            ON CONFLICT (lead_id, kind, agent_id) DO UPDATE SET
                profile_revision = EXCLUDED.profile_revision,
                reason           = EXCLUDED.reason,
                stopped_at       = EXCLUDED.stopped_at
            "#,
        )
        .bind(stop.lead_id.0)
        .bind(stop.target.kind.as_str())
        .bind(stop.target.agent.as_str())
        .bind(revision)
        .bind(stop.reason.as_str())
        .bind(stop.stopped_at)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self, kinds), fields(db.system = "postgresql", db.operation = "delete", db.table = "lead_agent_stops", lead.id = %lead), err)]
    async fn clear_stops(&mut self, lead: LeadId, kinds: &[WorkKind]) -> Result<(), CoreError> {
        let kinds: Vec<&str> = kinds.iter().map(|kind| kind.as_str()).collect();
        let mut tx = self.tx.lock().await;
        sqlx::query("DELETE FROM lead_agent_stops WHERE lead_id = $1 AND kind = ANY ($2)")
            .bind(lead.0)
            .bind(&kinds)
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "lead_agent_stops", lead.id = %lead), err)]
    async fn stops(&mut self, lead: LeadId) -> Result<Vec<AgentStop>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, StopRow>(
            r#"
            SELECT lead_id, kind, agent_id, profile_revision, reason, stopped_at
            FROM lead_agent_stops
            WHERE lead_id = $1
            ORDER BY stopped_at DESC, kind ASC, agent_id ASC
            "#,
        )
        .bind(lead.0)
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter().map(AgentStop::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "jobs", source = source.as_str()), err)]
    async fn cancel_source_jobs(
        &mut self,
        source: SourceName,
        reason: CancelReason,
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'cancelled', last_error = $3, updated_at = $4
            WHERE kind = $1 AND dedup_key = $2 AND status = 'pending'
            "#,
        )
        .bind(SOURCE_JOB_KIND)
        .bind(format!("source:{}", source.as_str()))
        .bind(reason.as_str())
        .bind(now)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "jobs"), err)]
    async fn cancelled_sources(&mut self) -> Result<Vec<SourceName>, CoreError> {
        let mut tx = self.tx.lock().await;
        let sources: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT cancelled.payload->>'source'
            FROM jobs cancelled
            WHERE cancelled.kind = $1
              AND cancelled.status = 'cancelled'
              AND cancelled.payload->>'source' IS NOT NULL
              AND NOT EXISTS (
                  SELECT 1 FROM jobs live
                  WHERE live.dedup_key = cancelled.dedup_key
                    AND live.status IN ('pending', 'running')
              )
            "#,
        )
        .bind(SOURCE_JOB_KIND)
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(sources
            .iter()
            .filter_map(|source| source.parse::<SourceName>().ok())
            .collect())
    }
}
