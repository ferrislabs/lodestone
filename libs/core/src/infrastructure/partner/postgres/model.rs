use chrono::{DateTime, NaiveDate, Utc};
use common::CoreError;
use uuid::Uuid;

use crate::{Partner, PartnerId, ProfileId, Siret};

#[derive(Debug, Clone)]
pub struct PartnerRow {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub siret: String,
    pub name: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_award_at: Option<NaiveDate>,
    pub award_count: i32,
}

impl TryFrom<PartnerRow> for Partner {
    type Error = CoreError;

    fn try_from(row: PartnerRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: PartnerId(row.id),
            profile_id: ProfileId(row.profile_id),
            siret: Siret::parse(&row.siret)
                .map_err(|_| CoreError::Internal("stored partner siret is invalid".to_owned()))?,
            name: row.name,
            first_seen_at: row.first_seen_at,
            last_award_at: row.last_award_at,
            award_count: u32::try_from(row.award_count)
                .map_err(|_| CoreError::Internal("stored award count is negative".to_owned()))?,
        })
    }
}
