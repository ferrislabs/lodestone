pub mod gateway;

use std::time::Duration;

use chrono::Utc;
use common::{CoreError, Secret, generate_uuid_v7};
use lodestone_macros::transactional;
use url::Url;

use crate::{
    application::LodestoneUseCase,
    domain::llm::{
        ProviderDefinition, ProviderKind, ReasoningEffort,
        provider::{
            CipherError, LlmProviderRecord, LlmProviderRepository, ProviderId, ResolvedProvider,
            SecretCipher, Tier, TierError, TokenPricing,
        },
    },
};

#[derive(Debug, Clone)]
pub struct CreateLlmProvider {
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: Url,
    pub model: String,
    pub timeout: Duration,
    pub max_retries: u8,
    pub pricing: TokenPricing,
    pub api_key: Option<Secret>,
    pub reasoning_effort: Option<ReasoningEffort>,
}

#[derive(Debug, Clone)]
pub enum KeyChange {
    Keep,
    Replace(Secret),
    Remove,
}

#[derive(Debug, Clone)]
pub struct UpdateLlmProvider {
    pub id: ProviderId,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: Url,
    pub model: String,
    pub timeout: Duration,
    pub max_retries: u8,
    pub pricing: TokenPricing,
    pub api_key: KeyChange,
    pub reasoning_effort: Option<ReasoningEffort>,
}

fn requires_key(kind: ProviderKind) -> bool {
    matches!(kind, ProviderKind::Anthropic | ProviderKind::Mistral)
}

fn missing_key() -> CoreError {
    CoreError::Invalid {
        field: "api_key".to_owned(),
        message: "is required for this provider kind".to_owned(),
    }
}

impl LodestoneUseCase {
    #[transactional(llm_provider)]
    pub async fn create_llm_provider<C: SecretCipher>(
        &self,
        command: CreateLlmProvider,
        cipher: &C,
    ) -> Result<LlmProviderRecord, CoreError> {
        let mut repo = llm_provider_repository;
        if command.api_key.is_none() && requires_key(command.kind) {
            return Err(missing_key());
        }
        let id = ProviderId(generate_uuid_v7());
        let api_key = command
            .api_key
            .as_ref()
            .map(|key| cipher.seal(key, id.0.as_bytes()))
            .transpose()
            .map_err(|_| CoreError::Internal("llm provider key could not be sealed".to_owned()))?;
        let now = Utc::now();
        let record = LlmProviderRecord {
            id,
            name: command.name,
            kind: command.kind,
            base_url: command.base_url,
            model: command.model,
            timeout: command.timeout,
            max_retries: command.max_retries,
            pricing: command.pricing,
            api_key,
            enabled: true,
            reasoning_effort: command.reasoning_effort,
            created_at: now,
            updated_at: now,
        };
        repo.insert(&record).await?;
        Ok(record)
    }

    #[transactional(llm_provider)]
    pub async fn update_llm_provider<C: SecretCipher>(
        &self,
        command: UpdateLlmProvider,
        cipher: &C,
    ) -> Result<LlmProviderRecord, CoreError> {
        let mut repo = llm_provider_repository;
        let current = repo.find(command.id).await?.ok_or(CoreError::NotFound)?;
        let api_key = match &command.api_key {
            KeyChange::Keep => current.api_key,
            KeyChange::Remove => None,
            KeyChange::Replace(secret) => {
                Some(cipher.seal(secret, command.id.0.as_bytes()).map_err(|_| {
                    CoreError::Internal("llm provider key could not be sealed".to_owned())
                })?)
            }
        };
        if api_key.is_none() && requires_key(command.kind) {
            return Err(missing_key());
        }
        let record = LlmProviderRecord {
            id: current.id,
            name: command.name,
            kind: command.kind,
            base_url: command.base_url,
            model: command.model,
            timeout: command.timeout,
            max_retries: command.max_retries,
            pricing: command.pricing,
            api_key,
            enabled: current.enabled,
            reasoning_effort: command.reasoning_effort,
            created_at: current.created_at,
            updated_at: Utc::now(),
        };
        if !repo.update(&record).await? {
            return Err(CoreError::NotFound);
        }
        Ok(record)
    }

    #[transactional(llm_provider)]
    pub async fn set_llm_provider_enabled(
        &self,
        id: ProviderId,
        enabled: bool,
    ) -> Result<LlmProviderRecord, CoreError> {
        let mut repo = llm_provider_repository;
        let mut record = repo.find(id).await?.ok_or(CoreError::NotFound)?;
        record.enabled = enabled;
        record.updated_at = Utc::now();
        if !repo.update(&record).await? {
            return Err(CoreError::NotFound);
        }
        Ok(record)
    }

    #[transactional(llm_provider)]
    pub async fn list_llm_providers(&self) -> Result<Vec<LlmProviderRecord>, CoreError> {
        let mut repo = llm_provider_repository;
        repo.list().await
    }

    #[transactional(llm_provider)]
    pub async fn get_llm_provider(&self, id: ProviderId) -> Result<LlmProviderRecord, CoreError> {
        let mut repo = llm_provider_repository;
        repo.find(id).await?.ok_or(CoreError::NotFound)
    }

    #[transactional(llm_provider)]
    pub async fn list_llm_tiers(&self) -> Result<Vec<(Tier, ProviderId)>, CoreError> {
        let mut repo = llm_provider_repository;
        repo.list_tiers().await
    }

    #[transactional(llm_provider)]
    pub async fn unassign_llm_tier(&self, tier: Tier) -> Result<(), CoreError> {
        let mut repo = llm_provider_repository;
        if repo.unassign_tier(tier).await? {
            Ok(())
        } else {
            Err(CoreError::NotFound)
        }
    }

    #[transactional(llm_provider)]
    pub async fn delete_llm_provider(&self, id: ProviderId) -> Result<(), CoreError> {
        let mut repo = llm_provider_repository;
        if repo.find(id).await?.is_none() {
            return Err(CoreError::NotFound);
        }
        if let Some((tier, _)) = repo
            .list_tiers()
            .await?
            .into_iter()
            .find(|(_, provider)| *provider == id)
        {
            return Err(CoreError::Conflict(format!(
                "provider is assigned to the {} tier",
                tier.as_str()
            )));
        }
        if repo.delete(id).await? {
            Ok(())
        } else {
            Err(CoreError::NotFound)
        }
    }

    #[transactional(llm_provider)]
    pub async fn assign_llm_tier(&self, tier: Tier, provider: ProviderId) -> Result<(), CoreError> {
        let mut repo = llm_provider_repository;
        if repo.find(provider).await?.is_none() {
            return Err(CoreError::NotFound);
        }
        repo.assign_tier(tier, provider, Utc::now()).await
    }

    #[transactional(llm_provider)]
    async fn load_tier_provider(&self, tier: Tier) -> Result<Option<LlmProviderRecord>, CoreError> {
        let mut repo = llm_provider_repository;
        repo.tier_provider(tier).await
    }

    #[transactional(llm_provider)]
    async fn load_provider_record(
        &self,
        id: ProviderId,
    ) -> Result<Option<LlmProviderRecord>, CoreError> {
        let mut repo = llm_provider_repository;
        repo.find(id).await
    }

    pub async fn resolve_llm_tier<C: SecretCipher>(
        &self,
        tier: Tier,
        cipher: &C,
    ) -> Result<ResolvedProvider, TierError> {
        let record = self
            .load_tier_provider(tier)
            .await?
            .ok_or(TierError::Unassigned(tier))?;
        if !record.enabled {
            return Err(TierError::Disabled(tier));
        }
        Ok(resolve_record(record, cipher)?)
    }

    pub async fn resolve_llm_provider<C: SecretCipher>(
        &self,
        id: ProviderId,
        cipher: &C,
    ) -> Result<Option<ResolvedProvider>, CoreError> {
        let Some(record) = self.load_provider_record(id).await? else {
            return Ok(None);
        };
        if !record.enabled {
            return Ok(None);
        }
        match resolve_record(record, cipher) {
            Ok(resolved) => Ok(Some(resolved)),
            Err(error) => {
                tracing::warn!(provider = %id, %error, "llm provider key could not be opened");
                Ok(None)
            }
        }
    }
}

fn resolve_record<C: SecretCipher>(
    record: LlmProviderRecord,
    cipher: &C,
) -> Result<ResolvedProvider, CipherError> {
    let api_key = record
        .api_key
        .as_ref()
        .map(|sealed| cipher.open(sealed, record.id.0.as_bytes()))
        .transpose()?;
    Ok(ResolvedProvider {
        id: record.id,
        definition: ProviderDefinition {
            kind: record.kind,
            base_url: record.base_url,
            model: record.model,
            api_key,
            timeout: record.timeout,
            reasoning_effort: record.reasoning_effort,
        },
        max_retries: record.max_retries,
        pricing: record.pricing,
    })
}
