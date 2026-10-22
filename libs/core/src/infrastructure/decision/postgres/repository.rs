use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use uuid::Uuid;

use crate::{
    LeadId, ProfileId, RevisionNumber, UserId, Verdict,
    domain::quality::{
        DecisionKind, DecisionLabel, DecisionSnapshot, ExclusionCount, LabelFilter, Rejection,
        RejectionReason, SourcingRow, VerdictCounts, ports::DecisionRepository,
    },
    infrastructure::{
        lead::postgres::repository::NOT_SUPPRESSED,
        postgres::{SharedTx, error::map_sqlx_error},
    },
};

#[repository(domain = Decision, backend = Postgres)]
pub struct PgDecisionRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgDecisionRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct LabelRow {
    id: Uuid,
    lead_id: Uuid,
    profile_id: Uuid,
    profile_revision: i32,
    kind: String,
    decided_by: Option<Uuid>,
    decided_at: DateTime<Utc>,
    snapshot: serde_json::Value,
    rejection_reason: Option<String>,
    rejection_note: Option<String>,
}

impl TryFrom<LabelRow> for DecisionLabel {
    type Error = CoreError;

    fn try_from(row: LabelRow) -> Result<Self, Self::Error> {
        let profile_revision = u32::try_from(row.profile_revision)
            .ok()
            .and_then(RevisionNumber::new)
            .ok_or_else(|| CoreError::Internal("stored label revision is invalid".to_owned()))?;
        let kind = DecisionKind::parse(&row.kind)
            .ok_or_else(|| CoreError::Internal("stored label kind is invalid".to_owned()))?;
        let snapshot: DecisionSnapshot = serde_json::from_value(row.snapshot)
            .map_err(|_| CoreError::Internal("stored label snapshot is invalid".to_owned()))?;
        let rejection = row
            .rejection_reason
            .as_deref()
            .map(|raw| {
                RejectionReason::parse(raw)
                    .ok_or_else(|| {
                        CoreError::Internal("stored label rejection reason is invalid".to_owned())
                    })
                    .and_then(|reason| {
                        Rejection::new(reason, row.rejection_note.as_deref()).map_err(|_| {
                            CoreError::Internal("stored label rejection note is invalid".to_owned())
                        })
                    })
            })
            .transpose()?;
        Ok(Self {
            id: row.id,
            lead_id: LeadId(row.lead_id),
            profile_id: ProfileId(row.profile_id),
            profile_revision,
            kind,
            decided_by: row.decided_by.map(UserId),
            decided_at: row.decided_at,
            snapshot,
            rejection,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SourcingRecord {
    profile_id: Uuid,
    agent_id: String,
    lead_id: Uuid,
    matched: Option<serde_json::Value>,
    title: Option<String>,
    verdict: Option<String>,
    decision: Option<String>,
}

impl TryFrom<SourcingRecord> for SourcingRow {
    type Error = CoreError;

    fn try_from(record: SourcingRecord) -> Result<Self, Self::Error> {
        let verdict = record
            .verdict
            .as_deref()
            .map(|raw| {
                Verdict::parse(raw)
                    .ok_or_else(|| CoreError::Internal("stored lead verdict is invalid".to_owned()))
            })
            .transpose()?;
        let decision = record
            .decision
            .as_deref()
            .map(|raw| {
                DecisionKind::parse(raw)
                    .ok_or_else(|| CoreError::Internal("stored label kind is invalid".to_owned()))
            })
            .transpose()?;
        let matched = record
            .matched
            .as_ref()
            .and_then(serde_json::Value::as_array)
            .map(|words| {
                words
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            profile_id: ProfileId(record.profile_id),
            agent_id: record.agent_id,
            lead_id: LeadId(record.lead_id),
            matched,
            title: record.title.unwrap_or_default(),
            verdict,
            decision,
        })
    }
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

impl<'tx> DecisionRepository for PgDecisionRepository<'tx> {
    #[tracing::instrument(skip(self, label), fields(db.system = "postgresql", db.operation = "insert", db.table = "decision_labels", lead.id = %label.lead_id), err)]
    async fn record(&mut self, label: &DecisionLabel) -> Result<(), CoreError> {
        let revision = i32::try_from(label.profile_revision.get())
            .map_err(|_| CoreError::Internal("profile revision number out of range".to_owned()))?;
        let snapshot = serde_json::to_value(&label.snapshot)
            .map_err(|error| CoreError::Internal(error.to_string()))?;
        let mut tx = self.tx.lock().await;
        sqlx::query(
            "INSERT INTO decision_labels (id, lead_id, profile_id, profile_revision, kind, decided_by, decided_at, snapshot, rejection_reason, rejection_note)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(label.id)
        .bind(label.lead_id.0)
        .bind(label.profile_id.0)
        .bind(revision)
        .bind(label.kind.as_str())
        .bind(label.decided_by.map(|user| user.0))
        .bind(label.decided_at)
        .bind(snapshot)
        .bind(label.rejection.as_ref().map(|rejection| rejection.reason().as_str()))
        .bind(label.rejection.as_ref().and_then(Rejection::note))
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "decision_labels"), err)]
    async fn list(&mut self, filter: &LabelFilter) -> Result<Vec<DecisionLabel>, CoreError> {
        let query = format!(
            "SELECT labels.id, labels.lead_id, labels.profile_id, labels.profile_revision, labels.kind, labels.decided_by, labels.decided_at, labels.snapshot, labels.rejection_reason, labels.rejection_note
             FROM decision_labels labels
             JOIN leads ON leads.id = labels.lead_id
             WHERE ($1::timestamptz IS NULL OR labels.decided_at >= $1)
               AND ($2::timestamptz IS NULL OR labels.decided_at <= $2)
               AND ($3::uuid IS NULL OR labels.profile_id = $3){NOT_SUPPRESSED}
             ORDER BY labels.decided_at, labels.id"
        );
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, LabelRow>(&query)
            .bind(filter.from)
            .bind(filter.to)
            .bind(filter.profile_id.map(|id| id.0))
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        rows.into_iter().map(DecisionLabel::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "leads"), err)]
    async fn verdict_counts(&mut self, filter: &LabelFilter) -> Result<VerdictCounts, CoreError> {
        let query = format!(
            "SELECT
                 count(*) FILTER (WHERE leads.verdict = 'fiable'),
                 count(*) FILTER (WHERE leads.verdict = 'a_verifier'),
                 count(*) FILTER (WHERE leads.verdict = 'ecarte')
             FROM leads
             WHERE leads.verdict IS NOT NULL
               AND ($1::timestamptz IS NULL OR leads.qualified_at >= $1)
               AND ($2::timestamptz IS NULL OR leads.qualified_at <= $2)
               AND ($3::uuid IS NULL OR leads.profile_id = $3){NOT_SUPPRESSED}"
        );
        let mut tx = self.tx.lock().await;
        let (reliable, to_check, discarded): (i64, i64, i64) = sqlx::query_as(&query)
            .bind(filter.from)
            .bind(filter.to)
            .bind(filter.profile_id.map(|id| id.0))
            .fetch_one(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(VerdictCounts {
            reliable: count(reliable),
            to_check: count(to_check),
            discarded: count(discarded),
        })
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "lead_qualification_state"), err)]
    async fn exclusion_counts(
        &mut self,
        filter: &LabelFilter,
    ) -> Result<Vec<ExclusionCount>, CoreError> {
        let query = format!(
            "SELECT item->>'agent_id', item->>'agent_name', item->>'question_id', item->>'option', item->>'label', count(*)
             FROM lead_qualification_state state
             JOIN leads ON leads.id = state.lead_id
             CROSS JOIN LATERAL jsonb_array_elements(state.exclusions) AS item
             WHERE leads.verdict = 'ecarte'
               AND ($1::timestamptz IS NULL OR leads.qualified_at >= $1)
               AND ($2::timestamptz IS NULL OR leads.qualified_at <= $2)
               AND ($3::uuid IS NULL OR leads.profile_id = $3){NOT_SUPPRESSED}
             GROUP BY 1, 2, 3, 4, 5
             ORDER BY 6 DESC, 1 ASC, 3 ASC, 4 ASC"
        );
        let mut tx = self.tx.lock().await;
        let rows: Vec<(String, String, String, String, String, i64)> = sqlx::query_as(&query)
            .bind(filter.from)
            .bind(filter.to)
            .bind(filter.profile_id.map(|id| id.0))
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(
                |(agent_id, agent_name, question_id, option, label, count)| ExclusionCount {
                    agent_id,
                    agent_name,
                    question_id,
                    option,
                    label,
                    count: u64::try_from(count).unwrap_or_default(),
                },
            )
            .collect())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "signals"), err)]
    async fn sourcing_rows(
        &mut self,
        profile_id: Option<ProfileId>,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> Result<Vec<SourcingRow>, CoreError> {
        let query = format!(
            "SELECT leads.profile_id, signals.agent_id, signals.lead_id,
                    signals.raw -> 'matched' AS matched,
                    COALESCE(signals.raw ->> 'title', signals.value) AS title,
                    leads.verdict,
                    decision.kind AS decision
             FROM signals
             JOIN leads ON leads.id = signals.lead_id
             LEFT JOIN LATERAL (
                 SELECT labels.kind
                 FROM decision_labels labels
                 WHERE labels.lead_id = leads.id
                   AND labels.kind IN ('lead_validated', 'lead_rejected')
                 ORDER BY labels.decided_at DESC, labels.id DESC
                 LIMIT 1
             ) decision ON TRUE
             WHERE signals.agent_id IS NOT NULL
               AND ($1::uuid IS NULL OR leads.profile_id = $1)
               AND ($2::timestamptz IS NULL OR signals.observed_at >= $2)
               AND ($3::timestamptz IS NULL OR signals.observed_at <= $3){NOT_SUPPRESSED}
             ORDER BY leads.profile_id, signals.agent_id, signals.lead_id, signals.observed_at, signals.id"
        );
        let mut tx = self.tx.lock().await;
        let records = sqlx::query_as::<_, SourcingRecord>(&query)
            .bind(profile_id.map(|id| id.0))
            .bind(from)
            .bind(to)
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        records.into_iter().map(SourcingRow::try_from).collect()
    }
}
