use lodestone_core::{
    LlmError, LlmProvider, LlmRequest, LlmResponse, ProviderDefinition, ProviderKind,
};

use crate::{anthropic::AnthropicProvider, openai::OpenAiCompatibleProvider};

pub enum AnyProvider {
    Anthropic(AnthropicProvider),
    OpenAiCompatible(OpenAiCompatibleProvider),
}

impl AnyProvider {
    pub fn from_definition(definition: &ProviderDefinition) -> Result<Self, LlmError> {
        match definition.kind {
            ProviderKind::Anthropic => {
                AnthropicProvider::new(definition.clone()).map(Self::Anthropic)
            }
            ProviderKind::Mistral | ProviderKind::OpenAiCompatible => {
                OpenAiCompatibleProvider::new(definition).map(Self::OpenAiCompatible)
            }
        }
    }
}

impl AnyProvider {
    pub fn reporting_from_definition(definition: &ProviderDefinition) -> Result<Self, LlmError> {
        match Self::from_definition(definition)? {
            Self::OpenAiCompatible(provider) => {
                Ok(Self::OpenAiCompatible(provider.without_fallback()))
            }
            other => Ok(other),
        }
    }
}

impl LlmProvider for AnyProvider {
    async fn complete(&self, request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        match self {
            Self::Anthropic(provider) => provider.complete(request).await,
            Self::OpenAiCompatible(provider) => provider.complete(request).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use common::Secret;
    use url::Url;

    use lodestone_core::GenerationParams;

    use super::*;

    fn definition(kind: ProviderKind, key: Option<&str>) -> ProviderDefinition {
        ProviderDefinition {
            kind,
            base_url: Url::parse("https://llm.example/").unwrap(),
            model: "test-model".to_owned(),
            api_key: key.map(Secret::new),
            timeout: Duration::from_secs(5),
            reasoning_effort: None,
            generation: GenerationParams::default(),
        }
    }

    #[test]
    fn dispatches_on_provider_kind() {
        assert!(matches!(
            AnyProvider::from_definition(&definition(ProviderKind::Anthropic, Some("k"))),
            Ok(AnyProvider::Anthropic(_))
        ));
        assert!(matches!(
            AnyProvider::from_definition(&definition(ProviderKind::Mistral, Some("k"))),
            Ok(AnyProvider::OpenAiCompatible(_))
        ));
        assert!(matches!(
            AnyProvider::from_definition(&definition(ProviderKind::OpenAiCompatible, None)),
            Ok(AnyProvider::OpenAiCompatible(_))
        ));
    }

    #[test]
    fn configuration_errors_surface() {
        assert!(matches!(
            AnyProvider::from_definition(&definition(ProviderKind::Anthropic, None)),
            Err(LlmError::Configuration(_))
        ));
    }
}
