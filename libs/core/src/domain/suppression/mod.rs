use chrono::{DateTime, Utc};

use crate::domain::{NormalizedDomain, UserId, lead::uuid_id, values::EmailAddress};

pub mod ports;

uuid_id!(SuppressionId);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SuppressionTarget {
    Domain(NormalizedDomain),
    Email(EmailAddress),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuppressionEntry {
    pub id: SuppressionId,
    pub target: SuppressionTarget,
    pub reason: Option<String>,
    pub created_by: Option<UserId>,
    pub created_at: DateTime<Utc>,
}
