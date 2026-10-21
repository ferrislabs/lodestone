use chrono::{DateTime, Utc};
use common::CoreError;
use uuid::Uuid;

use crate::{
    Lead, LeadId, LeadStatus, NormalizedDomain, ProfileId, ProfileName, ProfileSummary,
    RevisionNumber, Score, domain::qualification::Verdict,
};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LeadRow {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub domain: String,
    pub status: String,
    pub score: f64,
    pub score_breakdown: serde_json::Value,
    pub confidence: Option<f64>,
    pub verdict: Option<String>,
    pub qualified_at: Option<DateTime<Utc>>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GroupedLeadRow {
    #[sqlx(flatten)]
    pub lead: LeadRow,
    pub siren: Option<String>,
}

impl TryFrom<LeadRow> for Lead {
    type Error = CoreError;

    fn try_from(row: LeadRow) -> Result<Self, Self::Error> {
        let domain = row
            .domain
            .parse::<NormalizedDomain>()
            .map_err(|_| CoreError::Internal("stored lead domain is invalid".to_owned()))?;
        let status = row
            .status
            .parse::<LeadStatus>()
            .map_err(|_| CoreError::Internal("stored lead status is invalid".to_owned()))?;
        let score = Score::new(row.score)
            .map_err(|_| CoreError::Internal("stored lead score is invalid".to_owned()))?;
        let score_breakdown = serde_json::from_value(row.score_breakdown)
            .map_err(|_| CoreError::Internal("stored score breakdown is invalid".to_owned()))?;
        let verdict = row
            .verdict
            .as_deref()
            .map(|raw| {
                Verdict::parse(raw)
                    .ok_or_else(|| CoreError::Internal("stored lead verdict is invalid".to_owned()))
            })
            .transpose()?;
        Ok(Self {
            id: LeadId(row.id),
            profile_id: ProfileId(row.profile_id),
            domain,
            status,
            score,
            score_breakdown,
            confidence: row.confidence,
            verdict,
            qualified_at: row.qualified_at,
            first_seen_at: row.first_seen_at,
            last_seen_at: row.last_seen_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProfileSummaryRow {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
    pub revision: Option<i32>,
    pub lead_count: i64,
}

impl TryFrom<ProfileSummaryRow> for ProfileSummary {
    type Error = CoreError;

    fn try_from(row: ProfileSummaryRow) -> Result<Self, Self::Error> {
        let name = ProfileName::parse(&row.name)
            .map_err(|_| CoreError::Internal("stored profile name is invalid".to_owned()))?;
        let revision = row
            .revision
            .map(|number| {
                u32::try_from(number)
                    .ok()
                    .and_then(RevisionNumber::new)
                    .ok_or_else(|| CoreError::Internal("stored revision is invalid".to_owned()))
            })
            .transpose()?;
        Ok(Self {
            id: ProfileId(row.id),
            name,
            active: row.active,
            revision,
            lead_count: u64::try_from(row.lead_count).unwrap_or_default(),
        })
    }
}
