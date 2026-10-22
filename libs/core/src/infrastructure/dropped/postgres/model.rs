use chrono::{DateTime, Utc};
use common::CoreError;
use serde_json::{Value, from_value};
use uuid::Uuid;

use crate::{
    NormalizedDomain, ProfileId, UserId,
    definition::AgentId,
    domain::{
        dropped::{DropReason, DropReview, DroppedCandidate, DroppedId, RecallRow},
        values::{Provenance, SignalKind, SourceName},
    },
};

#[derive(Debug, sqlx::FromRow)]
pub struct DroppedRow {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub agent_id: Option<String>,
    pub source: String,
    pub kind: String,
    pub domain: String,
    pub title: String,
    pub excerpts: Value,
    pub provenance: String,
    pub observed_at: DateTime<Utc>,
    pub reason: String,
    pub matched: Value,
    pub excluded: Value,
    pub details: Value,
    pub dropped_at: DateTime<Utc>,
    pub review: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub reviewed_by: Option<Uuid>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct RecallCountRow {
    pub profile_id: Uuid,
    pub agent_id: Option<String>,
    pub reason: String,
    pub dropped: i64,
    pub reviewed: i64,
    pub should_have_kept: i64,
}

fn invalid(what: &str) -> CoreError {
    CoreError::Internal(format!("stored dropped candidate {what} is invalid"))
}

fn words(value: Value, what: &str) -> Result<Vec<String>, CoreError> {
    from_value(value).map_err(|_| invalid(what))
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

impl TryFrom<DroppedRow> for DroppedCandidate {
    type Error = CoreError;

    fn try_from(row: DroppedRow) -> Result<Self, CoreError> {
        let review = row
            .review
            .as_deref()
            .map(|raw| DropReview::parse(raw).ok_or_else(|| invalid("review")))
            .transpose()?;
        Ok(Self {
            id: DroppedId(row.id),
            profile_id: ProfileId(row.profile_id),
            agent: row.agent_id.map(AgentId::new),
            source: row
                .source
                .parse::<SourceName>()
                .map_err(|_| invalid("source"))?,
            kind: SignalKind::parse(&row.kind).map_err(|_| invalid("kind"))?,
            domain: row
                .domain
                .parse::<NormalizedDomain>()
                .map_err(|_| invalid("domain"))?,
            title: row.title,
            excerpts: words(row.excerpts, "excerpts")?,
            provenance: Provenance::parse(&row.provenance).map_err(|_| invalid("provenance"))?,
            observed_at: row.observed_at,
            reason: DropReason::parse(&row.reason).ok_or_else(|| invalid("reason"))?,
            matched: words(row.matched, "matched keywords")?,
            excluded: words(row.excluded, "excluded keywords")?,
            details: row.details,
            dropped_at: row.dropped_at,
            review,
            reviewed_at: row.reviewed_at,
            reviewed_by: row.reviewed_by.map(UserId),
        })
    }
}

impl TryFrom<RecallCountRow> for RecallRow {
    type Error = CoreError;

    fn try_from(row: RecallCountRow) -> Result<Self, CoreError> {
        Ok(Self {
            profile_id: ProfileId(row.profile_id),
            agent: row.agent_id.map(AgentId::new),
            reason: DropReason::parse(&row.reason).ok_or_else(|| invalid("reason"))?,
            dropped: count(row.dropped),
            reviewed: count(row.reviewed),
            should_have_kept: count(row.should_have_kept),
        })
    }
}
