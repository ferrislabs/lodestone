use std::{
    net::{Ipv4Addr, Ipv6Addr},
    time::Duration,
};

use chrono::{DateTime, Utc};
use common::{CoreError, Secret};
use thiserror::Error;
use url::{Host, Url};

use crate::domain::{
    lead::{str_enum, uuid_id},
    llm::{GenerationParams, ProviderDefinition, ProviderKind, ReasoningEffort},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaxConcurrency(u8);

impl MaxConcurrency {
    pub const MIN: u8 = 1;
    pub const MAX: u8 = 32;
    const LOCAL: Self = Self(2);
    const REMOTE: Self = Self(4);

    pub fn new(value: u8) -> Option<Self> {
        (Self::MIN..=Self::MAX)
            .contains(&value)
            .then_some(Self(value))
    }

    pub fn get(self) -> u8 {
        self.0
    }

    pub fn default_for(kind: ProviderKind, base_url: &Url) -> Self {
        if kind == ProviderKind::OpenAiCompatible && is_local(base_url) {
            Self::LOCAL
        } else {
            Self::REMOTE
        }
    }
}

fn is_local(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address == Ipv4Addr::LOCALHOST,
        Some(Host::Ipv6(address)) => address == Ipv6Addr::LOCALHOST,
        None => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedSecret {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LlmProviderRecord {
    pub id: ProviderId,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: Url,
    pub model: String,
    pub timeout: Duration,
    pub max_retries: u8,
    pub max_concurrency: MaxConcurrency,
    pub pricing: TokenPricing,
    pub api_key: Option<SealedSecret>,
    pub enabled: bool,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub generation: GenerationParams,
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
    pub name: String,
    pub definition: ProviderDefinition,
    pub max_retries: u8,
    pub max_concurrency: MaxConcurrency,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    #[test]
    fn concurrency_is_bounded_from_one_to_thirty_two() {
        assert_eq!(MaxConcurrency::new(0), None);
        assert_eq!(MaxConcurrency::new(1).map(MaxConcurrency::get), Some(1));
        assert_eq!(MaxConcurrency::new(32).map(MaxConcurrency::get), Some(32));
        assert_eq!(MaxConcurrency::new(33), None);
    }

    #[test]
    fn a_local_openai_compatible_server_defaults_to_two_and_anything_else_to_four() {
        for local in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:11434/v1",
            "http://[::1]:11434/v1",
            "http://LOCALHOST/v1",
        ] {
            assert_eq!(
                MaxConcurrency::default_for(ProviderKind::OpenAiCompatible, &url(local)).get(),
                2,
                "{local}"
            );
        }
        for remote in ["https://llm.example/v1", "http://10.0.0.5:11434/v1"] {
            assert_eq!(
                MaxConcurrency::default_for(ProviderKind::OpenAiCompatible, &url(remote)).get(),
                4,
                "{remote}"
            );
        }
        assert_eq!(
            MaxConcurrency::default_for(ProviderKind::Anthropic, &url("http://localhost/v1")).get(),
            4
        );
    }
}
