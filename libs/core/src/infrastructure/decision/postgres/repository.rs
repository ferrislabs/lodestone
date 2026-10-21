use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use uuid::Uuid;

use crate::{
    LeadId, ProfileId, RevisionNumber, UserId,
    domain::quality::{
        DecisionKind, DecisionLabel, DecisionSnapshot, LabelFilter, VerdictCounts,
        ports::DecisionRepository,
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
        Ok(Self {
            id: row.id,
            lead_id: LeadId(row.lead_id),
            profile_id: ProfileId(row.profile_id),
            profile_revision,
            kind,
            decided_by: row.decided_by.map(UserId),
            decided_at: row.decided_at,
            snapshot,
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
            "INSERT INTO decision_labels (id, lead_id, profile_id, profile_revision, kind, decided_by, decided_at, snapshot)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(label.id)
        .bind(label.lead_id.0)
        .bind(label.profile_id.0)
        .bind(revision)
        .bind(label.kind.as_str())
        .bind(label.decided_by.map(|user| user.0))
        .bind(label.decided_at)
        .bind(snapshot)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "decision_labels"), err)]
    async fn list(&mut self, filter: &LabelFilter) -> Result<Vec<DecisionLabel>, CoreError> {
        let query = format!(
            "SELECT labels.id, labels.lead_id, labels.profile_id, labels.profile_revision, labels.kind, labels.decided_by, labels.decided_at, labels.snapshot
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
}
