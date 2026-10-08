use std::time::Duration;

use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::domain::{
    NormalizedDomain,
    fetch::FetchError,
    profile::{
        ActiveProfile,
        definition::{AgentId, SourceTarget},
    },
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
    pub agent: Option<AgentId>,
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

    fn serves(&self, _item: &Self::Item, _target: &SourceTarget<'_>) -> bool {
        true
    }

    fn evaluate(
        &self,
        item: &Self::Item,
        profile: &ActiveProfile,
        target: &SourceTarget<'_>,
    ) -> Vec<Observation>;
}
