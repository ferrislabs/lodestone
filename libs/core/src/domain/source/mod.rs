use std::time::Duration;

use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::domain::{
    NormalizedDomain,
    fetch::FetchError,
    profile::{ActiveProfile, definition::ProfileDefinition},
    values::{Provenance, SignalKind, SourceName, UnitInterval},
};

#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub domain: NormalizedDomain,
    pub kind: SignalKind,
    pub value: Option<String>,
    pub raw: serde_json::Value,
    pub provenance: Provenance,
    pub strength: UnitInterval,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SourceError {
    #[error("rate limited, retry after {retry_after:?}")]
    RateLimited { retry_after: Duration },
    #[error("fetch failed: {0}")]
    Fetch(FetchError),
    #[error("upstream error: {0}")]
    Upstream(String),
}

impl From<FetchError> for SourceError {
    fn from(error: FetchError) -> Self {
        match error {
            FetchError::RateLimited { retry_after } => Self::RateLimited { retry_after },
            other => Self::Fetch(other),
        }
    }
}

pub trait Source: Send + Sync {
    type Item: Send + Sync;

    fn name(&self) -> SourceName;

    fn collect(
        &self,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<Self::Item>, SourceError>> + Send;

    fn evaluate(&self, item: &Self::Item, profile: &ActiveProfile) -> Vec<Observation>;
}

impl ProfileDefinition {
    pub fn enables(&self, source: SourceName) -> bool {
        let sources = &self.sources;
        match source {
            SourceName::CertificateTransparency => sources.certificate_transparency.is_some(),
            SourceName::HttpProbe => sources.http_probe.is_some(),
            SourceName::DnsCname => sources.dns_cname.is_some(),
            SourceName::Boamp => sources.boamp.is_some(),
            SourceName::Decp => sources.decp.is_some(),
            SourceName::Ted => sources.ted.is_some(),
            SourceName::FranceTravail => sources.france_travail.is_some(),
            SourceName::Ats => sources.ats.is_some(),
            SourceName::PublicPage | SourceName::SecurityTxt | SourceName::RoleGuess => true,
        }
    }
}
