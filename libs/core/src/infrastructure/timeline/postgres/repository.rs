use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::repository;
use serde_json::{Value, from_value, to_value};
use sqlx::{query, query_as, query_scalar};
use uuid::Uuid;

use crate::{
    LeadId, NormalizedDomain, ProfileId,
    domain::timeline::{
        EventActor, EventStatus, TimelineEntry, TimelineEvent, TimelineStep, micros,
        ports::TimelineRepository,
    },
    infrastructure::postgres::{SharedTx, error::map_sqlx_error},
};

#[repository(domain = Timeline, backend = Postgres)]
pub struct PgTimelineRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgTimelineRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

struct EventRow {
    id: Uuid,
    lead_id: Uuid,
    profile_id: Uuid,
    round: i32,
    step: String,
    kind: String,
    status: String,
    actor: Value,
    detail: Value,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    error: Option<String>,
    dedupe_key: String,
}

impl TryFrom<EventRow> for TimelineEvent {
    type Error = CoreError;

    fn try_from(row: EventRow) -> Result<Self, CoreError> {
        let step = TimelineStep::parse(&row.step)
            .ok_or_else(|| CoreError::Internal("stored timeline step is invalid".to_owned()))?;
        let status = EventStatus::parse(&row.status)
            .ok_or_else(|| CoreError::Internal("stored timeline status is invalid".to_owned()))?;
        let actor: EventActor = from_value(row.actor)
            .map_err(|_| CoreError::Internal("stored timeline actor is invalid".to_owned()))?;
        Ok(Self {
            id: row.id,
            lead_id: LeadId(row.lead_id),
            profile_id: ProfileId(row.profile_id),
            round: u32::try_from(row.round).unwrap_or_default(),
            step,
            kind: row.kind,
            status,
            actor,
            detail: row.detail,
            started_at: row.started_at,
            finished_at: row.finished_at,
            error: row.error,
            dedupe_key: row.dedupe_key,
        })
    }
}

fn actor_json(entry: &TimelineEntry) -> Result<Value, CoreError> {
    to_value(&entry.actor).map_err(|error| CoreError::Internal(error.to_string()))
}

impl<'tx> TimelineRepository for PgTimelineRepository<'tx> {
    #[tracing::instrument(skip(self, entry), fields(db.system = "postgresql", db.operation = "insert", db.table = "lead_events", lead.id = %lead, step = entry.step.as_str()), err)]
    async fn record(&mut self, lead: LeadId, entry: &TimelineEntry) -> Result<bool, CoreError> {
        let actor = actor_json(entry)?;
        let mut tx = self.tx.lock().await;
        let result = query!(
            r#"
            INSERT INTO lead_events (
                id, lead_id, profile_id, round, step, kind, status, actor, detail,
                started_at, finished_at, error, dedupe_key
            )
            SELECT
                $1, leads.id, leads.profile_id,
                COALESCE((
                    SELECT MAX(events.round) FROM lead_events events
                    WHERE events.lead_id = leads.id AND events.kind = 'round_started'
                ), 0) + $3,
                $4, $5, $6, $7, $8, $9, $10, $11, $12
            FROM leads
            WHERE leads.id = $2
            ON CONFLICT (dedupe_key) DO NOTHING
            "#,
            generate_uuid_v7(),
            lead.0,
            entry.slot.offset(),
            entry.step.as_str(),
            entry.kind,
            entry.status.as_str(),
            actor,
            &entry.detail,
            entry.started_at,
            entry.finished_at,
            entry.error.as_deref(),
            entry.dedupe_key(lead),
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() == 1)
    }

    #[tracing::instrument(skip(self, entry), fields(db.system = "postgresql", db.operation = "insert", db.table = "lead_events", domain = %domain, step = entry.step.as_str()), err)]
    async fn record_for_domain(
        &mut self,
        domain: &NormalizedDomain,
        entry: &TimelineEntry,
    ) -> Result<u64, CoreError> {
        let actor = actor_json(entry)?;
        let mut tx = self.tx.lock().await;
        let result = query!(
            r#"
            INSERT INTO lead_events (
                id, lead_id, profile_id, round, step, kind, status, actor, detail,
                started_at, finished_at, error, dedupe_key
            )
            SELECT
                gen_random_uuid(), leads.id, leads.profile_id,
                COALESCE((
                    SELECT MAX(events.round) FROM lead_events events
                    WHERE events.lead_id = leads.id AND events.kind = 'round_started'
                ), 0) + $2,
                $3, $4, $5, $6, $7, $8, $9, $10, leads.id::text || ':' || $11
            FROM leads
            WHERE leads.domain = $1 AND leads.excluded_reason IS NULL
            ORDER BY leads.id
            ON CONFLICT (dedupe_key) DO NOTHING
            "#,
            domain.as_str(),
            entry.slot.offset(),
            entry.step.as_str(),
            entry.kind,
            entry.status.as_str(),
            actor,
            &entry.detail,
            entry.started_at,
            entry.finished_at,
            entry.error.as_deref(),
            &entry.key,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "insert", db.table = "lead_events"), err)]
    async fn record_exclusions(
        &mut self,
        reason: &str,
        at: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = query!(
            r#"
            INSERT INTO lead_events (
                id, lead_id, profile_id, round, step, kind, status, actor, detail,
                started_at, finished_at, error, dedupe_key
            )
            SELECT
                gen_random_uuid(), leads.id, leads.profile_id,
                COALESCE((
                    SELECT MAX(events.round) FROM lead_events events
                    WHERE events.lead_id = leads.id AND events.kind = 'round_started'
                ), 0) + 1,
                'exclusion', 'excluded', 'succeeded', '{}'::jsonb,
                jsonb_build_object('reason', $1::text), $2, $2, NULL,
                leads.id::text || ':exclusion:' || $3
            FROM leads
            WHERE leads.excluded_reason = $1 AND leads.excluded_at = $2
            ORDER BY leads.id
            ON CONFLICT (dedupe_key) DO NOTHING
            "#,
            reason,
            at,
            micros(at),
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "lead_events", lead.id = %lead), err)]
    async fn list(&mut self, lead: LeadId) -> Result<Vec<TimelineEvent>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = query_as!(
            EventRow,
            r#"
            SELECT id, lead_id, profile_id, round, step, kind, status, actor, detail,
                   started_at, finished_at, error, dedupe_key
            FROM lead_events
            WHERE lead_id = $1
            ORDER BY started_at DESC, id DESC
            "#,
            lead.0,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter().map(TimelineEvent::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "delete", db.table = "lead_events"), err)]
    async fn purge(&mut self, before: DateTime<Utc>) -> Result<u64, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = query!(
            r#"
            DELETE FROM lead_events event
            WHERE event.started_at < $1
              AND NOT (
                  event.kind = 'round_started'
                  AND event.round = (
                      SELECT MAX(latest.round) FROM lead_events latest
                      WHERE latest.lead_id = event.lead_id AND latest.kind = 'round_started'
                  )
              )
            "#,
            before,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "insert", db.table = "lead_events"), err)]
    async fn backfill(&mut self) -> Result<u64, CoreError> {
        let mut tx = self.tx.lock().await;
        let inserted = query_scalar!(
            r#"
            WITH targets AS (
                SELECT leads.id, leads.profile_id, leads.domain, leads.excluded_reason,
                       leads.excluded_at,
                       state.completed_at, state.scored_at, leads.verdict, leads.llm_score,
                       leads.confidence
                FROM leads
                LEFT JOIN lead_qualification_state state ON state.lead_id = leads.id
                WHERE NOT EXISTS (SELECT 1 FROM lead_events events WHERE events.lead_id = leads.id)
            ),
            detection AS (
                INSERT INTO lead_events (id, lead_id, profile_id, round, step, kind, status, actor, detail, started_at, finished_at, error, dedupe_key)
                SELECT gen_random_uuid(), signals.lead_id, targets.profile_id, 1, 'detection', 'signal', 'succeeded',
                       jsonb_strip_nulls(jsonb_build_object('source', signals.source, 'agent_id', signals.agent_id)),
                       jsonb_strip_nulls(jsonb_build_object('reconstructed', true, 'signal_kind', signals.kind, 'value', signals.value, 'provenance', signals.provenance)),
                       signals.observed_at, signals.observed_at, NULL,
                       signals.lead_id::text || ':backfill:signal:' || signals.id::text
                FROM signals JOIN targets ON targets.id = signals.lead_id
                ON CONFLICT (dedupe_key) DO NOTHING
                RETURNING 1
            ),
            exclusion AS (
                INSERT INTO lead_events (id, lead_id, profile_id, round, step, kind, status, actor, detail, started_at, finished_at, error, dedupe_key)
                SELECT gen_random_uuid(), targets.id, targets.profile_id, 1, 'exclusion', 'excluded', 'succeeded', '{}'::jsonb,
                       jsonb_build_object('reconstructed', true, 'reason', targets.excluded_reason),
                       targets.excluded_at, targets.excluded_at, NULL,
                       targets.id::text || ':backfill:exclusion'
                FROM targets
                WHERE targets.excluded_reason IS NOT NULL AND targets.excluded_at IS NOT NULL
                ON CONFLICT (dedupe_key) DO NOTHING
                RETURNING 1
            ),
            inspection AS (
                INSERT INTO lead_events (id, lead_id, profile_id, round, step, kind, status, actor, detail, started_at, finished_at, error, dedupe_key)
                SELECT gen_random_uuid(), targets.id, targets.profile_id, 1, 'inspection', 'module',
                       CASE modules.status
                           WHEN 'done' THEN 'succeeded'
                           WHEN 'failed' THEN 'failed'
                           WHEN 'not_applicable' THEN 'skipped'
                           ELSE 'waiting'
                       END,
                       jsonb_build_object('module', modules.module),
                       jsonb_build_object('reconstructed', true, 'module_status', modules.status),
                       modules.last_run, modules.last_run, modules.error,
                       targets.id::text || ':backfill:module:' || modules.module
                FROM lead_ecosystem_modules modules
                JOIN targets ON targets.domain = modules.domain
                WHERE modules.last_run IS NOT NULL
                ON CONFLICT (dedupe_key) DO NOTHING
                RETURNING 1
            ),
            rounds AS (
                INSERT INTO lead_events (id, lead_id, profile_id, round, step, kind, status, actor, detail, started_at, finished_at, error, dedupe_key)
                SELECT gen_random_uuid(), targets.id, targets.profile_id, 1, 'qualification', 'round_started', 'started', '{}'::jsonb,
                       jsonb_build_object('reconstructed', true, 'trigger', 'first'),
                       targets.completed_at, NULL, NULL,
                       targets.id::text || ':backfill:round'
                FROM targets
                WHERE targets.completed_at IS NOT NULL
                ON CONFLICT (dedupe_key) DO NOTHING
                RETURNING 1
            ),
            qualification AS (
                INSERT INTO lead_events (id, lead_id, profile_id, round, step, kind, status, actor, detail, started_at, finished_at, error, dedupe_key)
                SELECT gen_random_uuid(), results.lead_id, targets.profile_id, 1,
                       CASE WHEN results.escalated THEN 'escalation' ELSE 'qualification' END,
                       'agent',
                       CASE results.status WHEN 'valid' THEN 'succeeded' ELSE 'failed' END,
                       jsonb_strip_nulls(jsonb_build_object('agent_id', results.agent_id, 'provider', NULLIF(results.provider_name, ''), 'model', NULLIF(results.model, ''))),
                       jsonb_strip_nulls(jsonb_build_object('reconstructed', true, 'agent_score', results.agent_score, 'summary', results.summary, 'samples', results.samples)),
                       results.created_at, results.created_at, results.error,
                       results.lead_id::text || ':backfill:qualification:' || results.id::text
                FROM qualification_results results
                JOIN targets ON targets.id = results.lead_id
                JOIN lead_qualification_state state
                  ON state.lead_id = results.lead_id AND state.profile_revision = results.profile_revision
                ON CONFLICT (dedupe_key) DO NOTHING
                RETURNING 1
            ),
            scoring AS (
                INSERT INTO lead_events (id, lead_id, profile_id, round, step, kind, status, actor, detail, started_at, finished_at, error, dedupe_key)
                SELECT gen_random_uuid(), results.lead_id, targets.profile_id, 1, 'scoring', 'agent',
                       CASE results.status WHEN 'valid' THEN 'succeeded' ELSE 'failed' END,
                       jsonb_strip_nulls(jsonb_build_object('agent_id', results.agent_id, 'provider', NULLIF(results.provider_name, ''), 'model', NULLIF(results.model, ''))),
                       jsonb_strip_nulls(jsonb_build_object('reconstructed', true, 'score', results.score, 'verdict', results.verdict)),
                       results.created_at, results.created_at, results.error,
                       results.lead_id::text || ':backfill:scoring:' || results.id::text
                FROM scoring_results results
                JOIN targets ON targets.id = results.lead_id
                JOIN lead_qualification_state state
                  ON state.lead_id = results.lead_id AND state.profile_revision = results.profile_revision
                ON CONFLICT (dedupe_key) DO NOTHING
                RETURNING 1
            ),
            combination AS (
                INSERT INTO lead_events (id, lead_id, profile_id, round, step, kind, status, actor, detail, started_at, finished_at, error, dedupe_key)
                SELECT gen_random_uuid(), targets.id, targets.profile_id, 1, 'combination', 'combined', 'succeeded', '{}'::jsonb,
                       jsonb_strip_nulls(jsonb_build_object('reconstructed', true, 'verdict', targets.verdict, 'llm_score', targets.llm_score, 'confidence', targets.confidence)),
                       COALESCE(targets.scored_at, targets.completed_at), COALESCE(targets.scored_at, targets.completed_at), NULL,
                       targets.id::text || ':backfill:combination'
                FROM targets
                WHERE targets.completed_at IS NOT NULL AND targets.verdict IS NOT NULL
                ON CONFLICT (dedupe_key) DO NOTHING
                RETURNING 1
            ),
            aggregation AS (
                INSERT INTO lead_events (id, lead_id, profile_id, round, step, kind, status, actor, detail, started_at, finished_at, error, dedupe_key)
                SELECT gen_random_uuid(), assessments.lead_id, targets.profile_id, 1, 'aggregation', 'summary', 'succeeded',
                       jsonb_strip_nulls(jsonb_build_object('provider', NULLIF(assessments.provider_name, ''), 'model', NULLIF(assessments.model, ''))),
                       jsonb_build_object('reconstructed', true, 'evidence_verified', assessments.evidence_verified, 'quality_warnings', to_jsonb(assessments.quality_warnings), 'draft', assessments.draft_subject IS NOT NULL, 'draft_status', assessments.draft_status),
                       assessments.created_at, assessments.created_at, NULL,
                       assessments.lead_id::text || ':backfill:summary:' || assessments.id::text
                FROM assessments
                JOIN targets ON targets.id = assessments.lead_id
                ON CONFLICT (dedupe_key) DO NOTHING
                RETURNING 1
            ),
            retries AS (
                INSERT INTO lead_events (id, lead_id, profile_id, round, step, kind, status, actor, detail, started_at, finished_at, error, dedupe_key)
                SELECT gen_random_uuid(), retries.lead_id, targets.profile_id, 1, retries.kind, 'call_failed', 'waiting',
                       jsonb_build_object('agent_id', retries.agent_id),
                       jsonb_strip_nulls(jsonb_build_object('reconstructed', true, 'attempts', retries.attempts, 'next_attempt_at', retries.next_attempt_at)),
                       retries.updated_at, NULL, retries.error,
                       retries.lead_id::text || ':backfill:retry:' || retries.kind
                FROM lead_agent_retries retries
                JOIN targets ON targets.id = retries.lead_id
                ON CONFLICT (dedupe_key) DO NOTHING
                RETURNING 1
            ),
            stops AS (
                INSERT INTO lead_events (id, lead_id, profile_id, round, step, kind, status, actor, detail, started_at, finished_at, error, dedupe_key)
                SELECT gen_random_uuid(), stops.lead_id, targets.profile_id, 1, 'stop', 'agent_stopped', 'skipped',
                       jsonb_build_object('agent_id', stops.agent_id),
                       jsonb_build_object('reconstructed', true, 'work', stops.kind, 'reason', stops.reason),
                       stops.stopped_at, stops.stopped_at, NULL,
                       stops.lead_id::text || ':backfill:stop:' || stops.kind || ':' || stops.agent_id
                FROM lead_agent_stops stops
                JOIN targets ON targets.id = stops.lead_id
                ON CONFLICT (dedupe_key) DO NOTHING
                RETURNING 1
            ),
            decisions AS (
                INSERT INTO lead_events (id, lead_id, profile_id, round, step, kind, status, actor, detail, started_at, finished_at, error, dedupe_key)
                SELECT gen_random_uuid(), labels.lead_id, targets.profile_id, 1, 'decision', 'decision', 'succeeded', '{}'::jsonb,
                       jsonb_build_object('reconstructed', true, 'decision', labels.kind),
                       labels.decided_at, labels.decided_at, NULL,
                       labels.lead_id::text || ':backfill:decision:' || labels.id::text
                FROM decision_labels labels
                JOIN targets ON targets.id = labels.lead_id
                ON CONFLICT (dedupe_key) DO NOTHING
                RETURNING 1
            ),
            requalifications AS (
                INSERT INTO lead_events (id, lead_id, profile_id, round, step, kind, status, actor, detail, started_at, finished_at, error, dedupe_key)
                SELECT gen_random_uuid(), marks.lead_id, targets.profile_id,
                       CASE WHEN targets.completed_at IS NULL THEN 1 ELSE 2 END,
                       'requalification',
                       CASE WHEN marks.forced THEN 'reevaluation_requested' ELSE 'planned' END,
                       'waiting', '{}'::jsonb,
                       jsonb_build_object('reconstructed', true, 'requalify_after', marks.requalify_after),
                       COALESCE(marks.marked_at, marks.requalify_after), NULL, NULL,
                       marks.lead_id::text || ':backfill:requalification'
                FROM lead_requalifications marks
                JOIN targets ON targets.id = marks.lead_id
                WHERE marks.requalify_after IS NOT NULL
                ON CONFLICT (dedupe_key) DO NOTHING
                RETURNING 1
            )
            SELECT (
                (SELECT COUNT(*) FROM detection) + (SELECT COUNT(*) FROM exclusion)
                + (SELECT COUNT(*) FROM inspection) + (SELECT COUNT(*) FROM rounds)
                + (SELECT COUNT(*) FROM qualification) + (SELECT COUNT(*) FROM scoring)
                + (SELECT COUNT(*) FROM combination) + (SELECT COUNT(*) FROM aggregation)
                + (SELECT COUNT(*) FROM retries) + (SELECT COUNT(*) FROM stops)
                + (SELECT COUNT(*) FROM decisions) + (SELECT COUNT(*) FROM requalifications)
            ) AS "inserted!"
            "#,
        )
        .fetch_one(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(u64::try_from(inserted).unwrap_or_default())
    }
}
