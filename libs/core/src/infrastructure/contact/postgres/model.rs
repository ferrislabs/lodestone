use chrono::{DateTime, Utc};
use common::CoreError;
use uuid::Uuid;

use crate::{
    ContactEmail, ContactId, ContactKind, ContactMethod, EmailAddress, LeadId, Provenance,
    UnitInterval,
};

#[derive(Debug, Clone)]
pub struct ContactRow {
    pub id: Uuid,
    pub lead_id: Uuid,
    pub address: String,
    pub provenance: String,
    pub method: String,
    pub confidence: f64,
    pub kind: String,
    pub found_at: DateTime<Utc>,
}

impl TryFrom<ContactRow> for ContactEmail {
    type Error = CoreError;

    fn try_from(row: ContactRow) -> Result<Self, Self::Error> {
        let address = EmailAddress::parse(&row.address)
            .map_err(|_| CoreError::Internal("stored contact address is invalid".to_owned()))?;
        let provenance = Provenance::parse(&row.provenance)
            .map_err(|_| CoreError::Internal("stored contact provenance is invalid".to_owned()))?;
        let method = ContactMethod::parse(&row.method)
            .ok_or_else(|| CoreError::Internal("stored contact method is invalid".to_owned()))?;
        let confidence = UnitInterval::new(row.confidence)
            .map_err(|_| CoreError::Internal("stored contact confidence is invalid".to_owned()))?;
        let kind = ContactKind::parse(&row.kind)
            .ok_or_else(|| CoreError::Internal("stored contact kind is invalid".to_owned()))?;
        Ok(Self {
            id: ContactId(row.id),
            lead_id: LeadId(row.lead_id),
            address,
            provenance,
            method,
            confidence,
            kind,
            found_at: row.found_at,
        })
    }
}
