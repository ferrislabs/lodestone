use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    NormalizedDomain,
    profile::{ProfileId, RevisionNumber, definition::AgentId},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheScope {
    pub profile_id: ProfileId,
    pub revision: RevisionNumber,
    pub prompt: String,
    pub domain: NormalizedDomain,
    pub agent: Option<AgentId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CachedOutput {
    pub output: serde_json::Value,
    pub model: String,
    pub created_at: DateTime<Utc>,
}

pub trait LlmCache: Send + Sync {
    fn get(
        &self,
        key: &[u8],
    ) -> impl Future<Output = Result<Option<CachedOutput>, CoreError>> + Send;

    fn put(
        &self,
        key: &[u8],
        scope: &CacheScope,
        entry: &CachedOutput,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;
}
