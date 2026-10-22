use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use serde_json::json;
use sqlx::{query, query_as};

use crate::{
    ProfileId, UserId,
    definition::AgentId,
    domain::dropped::{
        DropReview, DroppedCandidate, DroppedFilter, DroppedId, RecallRow,
        ports::DroppedCandidateRepository,
    },
    infrastructure::{
        dropped::postgres::model::{DroppedRow, RecallCountRow},
        postgres::{SharedTx, error::map_sqlx_error},
    },
};

const COLUMNS: &str = "id, profile_id, agent_id, source, kind, domain, title, excerpts, provenance, observed_at, reason, matched, excluded, details, dropped_at, review, reviewed_at, reviewed_by";

#[repository(domain = Dropped, backend = Postgres)]
pub struct PgDroppedCandidateRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgDroppedCandidateRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> DroppedCandidateRepository for PgDroppedCandidateRepository<'tx> {
    #[tracing::instrument(skip(self, candidates), fields(db.system = "postgresql", db.operation = "upsert", db.table = "dropped_candidates", candidates = candidates.len()), err)]
    async fn record(&mut self, candidates: &[DroppedCandidate]) -> Result<u64, CoreError> {
        let mut tx = self.tx.lock().await;
        let mut stored = 0;
        for candidate in candidates {
            let result = query(
                "INSERT INTO dropped_candidates (id, profile_id, agent_id, source, kind, domain, title, excerpts, provenance, observed_at, reason, matched, excluded, details, dropped_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                 ON CONFLICT (profile_id, (COALESCE(agent_id, '')), provenance) DO UPDATE SET
                     title = EXCLUDED.title,
                     excerpts = EXCLUDED.excerpts,
                     reason = EXCLUDED.reason,
                     matched = EXCLUDED.matched,
                     excluded = EXCLUDED.excluded,
                     details = EXCLUDED.details,
                     observed_at = EXCLUDED.observed_at,
                     dropped_at = EXCLUDED.dropped_at",
            )
            .bind(candidate.id.0)
            .bind(candidate.profile_id.0)
            .bind(candidate.agent.as_ref().map(AgentId::as_str))
            .bind(candidate.source.as_str())
            .bind(candidate.kind.as_str())
            .bind(candidate.domain.as_str())
            .bind(&candidate.title)
            .bind(json!(candidate.excerpts))
            .bind(candidate.provenance.as_str())
            .bind(candidate.observed_at)
            .bind(candidate.reason.as_str())
            .bind(json!(candidate.matched))
            .bind(json!(candidate.excluded))
            .bind(&candidate.details)
            .bind(candidate.dropped_at)
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
            stored += result.rows_affected();
        }
        Ok(stored)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "dropped_candidates", profile.id = %profile_id), err)]
    async fn list(
        &mut self,
        profile_id: ProfileId,
        filter: &DroppedFilter,
        limit: u32,
        random: bool,
    ) -> Result<Vec<DroppedCandidate>, CoreError> {
        let order = if random {
            "random()"
        } else {
            "dropped_at DESC, id"
        };
        let statement = format!(
            "SELECT {COLUMNS}
             FROM dropped_candidates
             WHERE profile_id = $1
               AND ($2::text IS NULL OR reason = $2)
               AND ($3::text IS NULL OR agent_id = $3)
               AND ($4::boolean IS NULL OR (review IS NOT NULL) = $4)
             ORDER BY {order}
             LIMIT $5"
        );
        let mut tx = self.tx.lock().await;
        let rows = query_as::<_, DroppedRow>(&statement)
            .bind(profile_id.0)
            .bind(filter.reason.map(|reason| reason.as_str()))
            .bind(filter.agent.as_ref().map(AgentId::as_str))
            .bind(filter.reviewed)
            .bind(i64::from(limit))
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        rows.into_iter().map(DroppedCandidate::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "dropped_candidates", dropped.id = %id), err)]
    async fn find(&mut self, id: DroppedId) -> Result<Option<DroppedCandidate>, CoreError> {
        let statement = format!("SELECT {COLUMNS} FROM dropped_candidates WHERE id = $1");
        let mut tx = self.tx.lock().await;
        let row = query_as::<_, DroppedRow>(&statement)
            .bind(id.0)
            .fetch_optional(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        row.map(DroppedCandidate::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "dropped_candidates", dropped.id = %id), err)]
    async fn review(
        &mut self,
        id: DroppedId,
        review: DropReview,
        by: Option<UserId>,
        at: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = query(
            "UPDATE dropped_candidates
             SET review = $2, reviewed_by = $3, reviewed_at = $4
             WHERE id = $1 AND review IS NULL",
        )
        .bind(id.0)
        .bind(review.as_str())
        .bind(by.map(|user| user.0))
        .bind(at)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() == 1)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "delete", db.table = "dropped_candidates"), err)]
    async fn purge(&mut self, before: DateTime<Utc>) -> Result<u64, CoreError> {
        let mut tx = self.tx.lock().await;
        let result =
            query("DELETE FROM dropped_candidates WHERE review IS NULL AND dropped_at < $1")
                .bind(before)
                .execute(&mut ***tx)
                .await
                .map_err(map_sqlx_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "dropped_candidates"), err)]
    async fn recall(
        &mut self,
        profile_id: Option<ProfileId>,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> Result<Vec<RecallRow>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = query_as::<_, RecallCountRow>(
            "SELECT profile_id, agent_id, reason,
                    COUNT(*) AS dropped,
                    COUNT(review) AS reviewed,
                    COUNT(*) FILTER (WHERE review = 'should_have_kept') AS should_have_kept
             FROM dropped_candidates
             WHERE ($1::uuid IS NULL OR profile_id = $1)
               AND ($2::timestamptz IS NULL OR dropped_at >= $2)
               AND ($3::timestamptz IS NULL OR dropped_at <= $3)
             GROUP BY profile_id, agent_id, reason
             ORDER BY profile_id, agent_id NULLS FIRST, reason",
        )
        .bind(profile_id.map(|id| id.0))
        .bind(from)
        .bind(to)
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter().map(RecallRow::try_from).collect()
    }
}
