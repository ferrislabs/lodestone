use chrono::{DateTime, Utc};
use common::CoreError;
use uuid::Uuid;

use crate::{
    EmailAddress, NormalizedDomain, SuppressionEntry, SuppressionId, SuppressionTarget, UserId,
};

#[derive(Debug, Clone)]
pub struct SuppressionRow {
    pub id: Uuid,
    pub kind: String,
    pub value: String,
    pub reason: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub fn target_columns(target: &SuppressionTarget) -> (&'static str, &str) {
    match target {
        SuppressionTarget::Domain(domain) => ("domain", domain.as_str()),
        SuppressionTarget::Email(email) => ("email", email.as_str()),
    }
}

impl TryFrom<SuppressionRow> for SuppressionEntry {
    type Error = CoreError;

    fn try_from(row: SuppressionRow) -> Result<Self, Self::Error> {
        let target = match row.kind.as_str() {
            "domain" => {
                SuppressionTarget::Domain(row.value.parse::<NormalizedDomain>().map_err(|_| {
                    CoreError::Internal("stored suppression value is invalid".to_owned())
                })?)
            }
            "email" => SuppressionTarget::Email(EmailAddress::parse(&row.value).map_err(|_| {
                CoreError::Internal("stored suppression value is invalid".to_owned())
            })?),
            _ => {
                return Err(CoreError::Internal(
                    "stored suppression kind is invalid".to_owned(),
                ));
            }
        };
        Ok(Self {
            id: SuppressionId(row.id),
            target,
            reason: row.reason,
            created_by: row.created_by.map(UserId),
            created_at: row.created_at,
        })
    }
}
