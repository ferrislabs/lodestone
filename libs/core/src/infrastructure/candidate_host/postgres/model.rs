use chrono::{DateTime, Utc};
use common::CoreError;

use crate::{CandidateHost, HostOrigin, Hostname, NormalizedDomain};

#[derive(Debug, Clone)]
pub struct CandidateHostRow {
    pub host: String,
    pub domain: String,
    pub origin: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

impl TryFrom<CandidateHostRow> for CandidateHost {
    type Error = CoreError;

    fn try_from(row: CandidateHostRow) -> Result<Self, Self::Error> {
        let invalid =
            |what: &str| CoreError::Internal(format!("stored candidate host {what} is invalid"));
        Ok(Self {
            host: Hostname::parse(&row.host).map_err(|_| invalid("hostname"))?,
            domain: NormalizedDomain::from_host(&row.domain).map_err(|_| invalid("domain"))?,
            origin: HostOrigin::parse(&row.origin).ok_or_else(|| invalid("origin"))?,
            first_seen_at: row.first_seen_at,
            last_seen_at: row.last_seen_at,
        })
    }
}
