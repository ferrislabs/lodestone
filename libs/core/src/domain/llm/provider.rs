use std::time::Duration;

use chrono::{DateTime, Utc};
use common::{CoreError, Secret};
use thiserror::Error;
use url::Url;

use crate::domain::{
    lead::{str_enum, uuid_id},
    llm::{ProviderDefinition, ProviderKind, ReasoningEffort},
};

uuid_id!(ProviderId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tier {
    Triage,
    Qualification,
}

str_enum!(Tier {
    Triage => "triage",
    Qualification => "qualification",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenPricing {
    pub input_cents_per_mtok: u32,
    pub output_cents_per_mtok: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedSecret {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmProviderRecord {
    pub id: ProviderId,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: Url,
    pub model: String,
    pub timeout: Duration,
    pub max_retries: u8,
    pub pricing: TokenPricing,
    pub api_key: Option<SealedSecret>,
    pub enabled: bool,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LlmProviderRecord {
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CipherError {
    #[error("secret could not be encrypted")]
    Seal,
    #[error("secret could not be decrypted with the configured key")]
    Open,
}

pub trait SecretCipher: Send + Sync {
    fn seal(&self, secret: &Secret, context: &[u8]) -> Result<SealedSecret, CipherError>;

    fn open(&self, sealed: &SealedSecret, context: &[u8]) -> Result<Secret, CipherError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TierError {
    #[error("no provider is assigned to the {0:?} tier")]
    Unassigned(Tier),
    #[error("the provider assigned to the {0:?} tier is disabled")]
    Disabled(Tier),
    #[error(transparent)]
    Cipher(#[from] CipherError),
    #[error("{0}")]
    Storage(String),
}

impl From<CoreError> for TierError {
    fn from(error: CoreError) -> Self {
        Self::Storage(error.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedProvider {
    pub id: ProviderId,
    pub definition: ProviderDefinition,
    pub max_retries: u8,
    pub pricing: TokenPricing,
}

#[cfg_attr(test, mockall::automock)]
pub trait LlmProviderRepository: Send {
    fn insert(
        &mut self,
        record: &LlmProviderRecord,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn update(
        &mut self,
        record: &LlmProviderRecord,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn find(
        &mut self,
        id: ProviderId,
    ) -> impl Future<Output = Result<Option<LlmProviderRecord>, CoreError>> + Send;

    fn list(&mut self) -> impl Future<Output = Result<Vec<LlmProviderRecord>, CoreError>> + Send;

    fn assign_tier(
        &mut self,
        tier: Tier,
        provider: ProviderId,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn tier_provider(
        &mut self,
        tier: Tier,
    ) -> impl Future<Output = Result<Option<LlmProviderRecord>, CoreError>> + Send;

    fn delete(&mut self, id: ProviderId) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn list_tiers(
        &mut self,
    ) -> impl Future<Output = Result<Vec<(Tier, ProviderId)>, CoreError>> + Send;

    fn unassign_tier(&mut self, tier: Tier)
    -> impl Future<Output = Result<bool, CoreError>> + Send;
}
