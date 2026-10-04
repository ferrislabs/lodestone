use chrono::{DateTime, Utc};
use common::CoreError;
use uuid::Uuid;

use crate::{Lead, LeadId, LeadStatus, NormalizedDomain, ProfileId, Score};

#[derive(Debug, Clone)]
pub struct LeadRow {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub domain: String,
    pub status: String,
    pub score: f64,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
        Ok(Self {
            id: LeadId(row.id),
            profile_id: ProfileId(row.profile_id),
            domain,
            status,
            score,
            first_seen_at: row.first_seen_at,
            last_seen_at: row.last_seen_at,
            updated_at: row.updated_at,
        })
    }
}
