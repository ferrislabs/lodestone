use chrono::{DateTime, Utc};
use common::CoreError;
use uuid::Uuid;

use crate::{
    Profile, ProfileId, ProfileName, ProfileRevision, RevisionNumber, UserId, ValidatedProfile,
    domain::profile::ActiveProfile,
};

#[derive(Debug, Clone)]
pub struct ProfileRow {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RevisionRow {
    pub profile_id: Uuid,
    pub revision: i32,
    pub definition: serde_json::Value,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ActiveProfileRow {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: i32,
    pub definition: serde_json::Value,
    pub created_by: Option<Uuid>,
    pub revision_created_at: DateTime<Utc>,
}

pub fn revision_number(value: i32) -> Result<RevisionNumber, CoreError> {
    u32::try_from(value)
        .ok()
        .and_then(RevisionNumber::new)
        .ok_or_else(|| CoreError::Internal("stored profile revision number is invalid".to_owned()))
}

impl TryFrom<ProfileRow> for Profile {
    type Error = CoreError;

    fn try_from(row: ProfileRow) -> Result<Self, Self::Error> {
        let name = ProfileName::parse(&row.name)
            .map_err(|_| CoreError::Internal("stored profile name is invalid".to_owned()))?;
        Ok(Self {
            id: ProfileId(row.id),
            name,
            active: row.active,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

impl TryFrom<RevisionRow> for ProfileRevision {
    type Error = CoreError;

    fn try_from(row: RevisionRow) -> Result<Self, Self::Error> {
        let definition = ValidatedProfile::parse(row.definition)
            .map_err(|_| CoreError::Internal("stored profile definition is invalid".to_owned()))?;
        Ok(Self {
            profile_id: ProfileId(row.profile_id),
            number: revision_number(row.revision)?,
            definition,
            created_by: row.created_by.map(UserId),
            created_at: row.created_at,
        })
    }
}

impl TryFrom<ActiveProfileRow> for ActiveProfile {
    type Error = CoreError;

    fn try_from(row: ActiveProfileRow) -> Result<Self, Self::Error> {
        let revision = RevisionRow {
            profile_id: row.id,
            revision: row.revision,
            definition: row.definition,
            created_by: row.created_by,
            created_at: row.revision_created_at,
        };
        let profile = ProfileRow {
            id: row.id,
            name: row.name,
            active: row.active,
            created_at: row.created_at,
            updated_at: row.updated_at,
        };
        Ok(Self {
            profile: profile.try_into()?,
            revision: revision.try_into()?,
        })
    }
}
