use std::{num::NonZeroU32, time::Duration};

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tracing::warn;

use aws_lc_rs::digest::{Context, SHA256};
use chrono::Utc;
use serde::Serialize;

use crate::domain::llm::{
    LlmError, LlmProvider, LlmRequest, TokenUsage, UntrustedDocument,
    cache::{CacheScope, CachedOutput, LlmCache},
    provider::TokenPricing,
};

const MICROCENTS_PER_CENT: u64 = 1_000_000;
const MAX_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunBudget {
    limit_microcents: u64,
    spent_microcents: u64,
}

impl RunBudget {
    pub fn new(limit_cents: NonZeroU32) -> Self {
        Self {
            limit_microcents: u64::from(limit_cents.get()) * MICROCENTS_PER_CENT,
            spent_microcents: 0,
        }
    }

    pub fn is_exhausted(&self) -> bool {
        self.spent_microcents >= self.limit_microcents
    }

    pub fn spent_cents(&self) -> f64 {
        self.spent_microcents as f64 / MICROCENTS_PER_CENT as f64
    }

    pub fn charge(&mut self, usage: TokenUsage, pricing: TokenPricing) {
        let cost = usage
            .input_tokens
            .saturating_mul(u64::from(pricing.input_cents_per_mtok))
            .saturating_add(
                usage
                    .output_tokens
                    .saturating_mul(u64::from(pricing.output_cents_per_mtok)),
            );
        self.spent_microcents = self.spent_microcents.saturating_add(cost);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallSettings {
    pub max_retries: u8,
    pub pricing: TokenPricing,
    pub base_delay: Duration,
}

#[derive(Debug, Clone, Copy)]
pub struct Prompt<'a> {
    pub system: &'a str,
    pub instruction: &'a str,
    pub documents: &'a [UntrustedDocument<'a>],
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Structured<T> {
    pub value: T,
    pub model: String,
    pub usage: TokenUsage,
    pub from_cache: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LlmCallError {
    #[error("llm budget exhausted")]
    BudgetExhausted,
    #[error("llm output did not match the expected schema: {0}")]
    InvalidOutput(String),
    #[error(transparent)]
    Provider(LlmError),
    #[error("llm output schema could not be generated: {0}")]
    Schema(String),
}

pub fn output_schema<T: JsonSchema>() -> Result<serde_json::Value, LlmCallError> {
    serde_json::to_value(schemars::schema_for!(T)).map_err(|e| LlmCallError::Schema(e.to_string()))
}

fn is_retryable(error: &LlmError) -> bool {
    match error {
        LlmError::Timeout
        | LlmError::RateLimited { .. }
        | LlmError::Network(_)
        | LlmError::InvalidOutput(_) => true,
        LlmError::Status { status } => *status >= 500,
        LlmError::Refused | LlmError::Truncated | LlmError::Configuration(_) => false,
    }
}

fn backoff(settings: &CallSettings, attempt: u32, error: &LlmError) -> Duration {
    if let LlmError::RateLimited {
        retry_after: Some(retry_after),
    } = error
    {
        return (*retry_after).min(MAX_BACKOFF);
    }
    settings
        .base_delay
        .saturating_mul(2u32.saturating_pow(attempt))
        .min(MAX_BACKOFF)
}

pub async fn call_structured<T, P>(
    provider: &P,
    prompt: Prompt<'_>,
    settings: CallSettings,
    budget: &mut RunBudget,
) -> Result<Structured<T>, LlmCallError>
where
    T: DeserializeOwned + JsonSchema,
    P: LlmProvider,
{
    let schema = output_schema::<T>()?;
    let request = LlmRequest {
        system: prompt.system,
        instruction: prompt.instruction,
        documents: prompt.documents,
        schema_name: "output",
        schema: &schema,
        max_tokens: prompt.max_tokens,
    };
    let mut attempt: u32 = 0;
    loop {
        if budget.is_exhausted() {
            return Err(LlmCallError::BudgetExhausted);
        }
        let failure = match provider.complete(&request).await {
            Ok(response) => {
                budget.charge(response.usage, settings.pricing);
                match serde_json::from_value::<T>(response.output) {
                    Ok(value) => {
                        return Ok(Structured {
                            value,
                            model: response.model,
                            usage: response.usage,
                            from_cache: false,
                        });
                    }
                    Err(error) => LlmError::InvalidOutput(error.to_string()),
                }
            }
            Err(error) => error,
        };
        if !is_retryable(&failure) || attempt >= u32::from(settings.max_retries) {
            return Err(match failure {
                LlmError::InvalidOutput(message) => LlmCallError::InvalidOutput(message),
                other => LlmCallError::Provider(other),
            });
        }
        warn!(attempt, error = %failure, "llm call failed, retrying");
        tokio::time::sleep(backoff(&settings, attempt, &failure)).await;
        attempt += 1;
    }
}

fn hash_field(context: &mut Context, field: &[u8]) {
    context.update(&(field.len() as u64).to_be_bytes());
    context.update(field);
}

pub fn cache_key(scope: &CacheScope, prompt: &Prompt<'_>, schema: &serde_json::Value) -> Vec<u8> {
    let mut context = Context::new(&SHA256);
    hash_field(&mut context, scope.profile_id.0.as_bytes());
    hash_field(&mut context, &scope.revision.get().to_be_bytes());
    hash_field(&mut context, scope.prompt.as_bytes());
    hash_field(&mut context, scope.domain.as_str().as_bytes());
    if let Some(agent) = &scope.agent {
        hash_field(&mut context, b"agent");
        hash_field(&mut context, agent.as_str().as_bytes());
    }
    hash_field(&mut context, prompt.system.as_bytes());
    hash_field(&mut context, prompt.instruction.as_bytes());
    hash_field(&mut context, schema.to_string().as_bytes());
    for document in prompt.documents {
        hash_field(&mut context, document.source.as_bytes());
        hash_field(&mut context, document.content.as_bytes());
    }
    context.finish().as_ref().to_vec()
}

pub async fn store_cached<T, C>(
    cache: &C,
    scope: &CacheScope,
    prompt: &Prompt<'_>,
    value: &T,
    model: &str,
) where
    T: JsonSchema + Serialize,
    C: LlmCache,
{
    let schema = match output_schema::<T>() {
        Ok(schema) => schema,
        Err(error) => {
            warn!(%error, "llm output could not be cached");
            return;
        }
    };
    let key = cache_key(scope, prompt, &schema);
    match serde_json::to_value(value) {
        Ok(output) => {
            let entry = CachedOutput {
                output,
                model: model.to_owned(),
                created_at: Utc::now(),
            };
            if let Err(error) = cache.put(&key, scope, &entry).await {
                warn!(%error, "llm cache write failed");
            }
        }
        Err(error) => warn!(%error, "llm output could not be cached"),
    }
}

pub async fn call_structured_cached<T, P, C>(
    provider: &P,
    cache: &C,
    scope: &CacheScope,
    prompt: Prompt<'_>,
    settings: CallSettings,
    budget: &mut RunBudget,
) -> Result<Structured<T>, LlmCallError>
where
    T: DeserializeOwned + JsonSchema + Serialize,
    P: LlmProvider,
    C: LlmCache,
{
    let schema = output_schema::<T>()?;
    let key = cache_key(scope, &prompt, &schema);
    match cache.get(&key).await {
        Ok(Some(hit)) => match serde_json::from_value::<T>(hit.output) {
            Ok(value) => {
                return Ok(Structured {
                    value,
                    model: hit.model,
                    usage: TokenUsage::default(),
                    from_cache: true,
                });
            }
            Err(error) => warn!(%error, "cached llm output no longer matches its type"),
        },
        Ok(None) => {}
        Err(error) => warn!(%error, "llm cache read failed"),
    }
    let fresh = call_structured::<T, P>(provider, prompt, settings, budget).await?;
    match serde_json::to_value(&fresh.value) {
        Ok(output) => {
            let entry = CachedOutput {
                output,
                model: fresh.model.clone(),
                created_at: Utc::now(),
            };
            if let Err(error) = cache.put(&key, scope, &entry).await {
                warn!(%error, "llm cache write failed");
            }
        }
        Err(error) => warn!(%error, "llm output could not be cached"),
    }
    Ok(fresh)
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex};

    use schemars::JsonSchema;
    use serde::Deserialize;

    use super::*;
    use crate::domain::llm::LlmResponse;

    fn scope_of(agent: Option<&str>) -> CacheScope {
        CacheScope {
            profile_id: crate::domain::profile::ProfileId(uuid::Uuid::nil()),
            revision: crate::domain::profile::RevisionNumber::FIRST,
            prompt: "triage@1".to_owned(),
            domain: crate::domain::NormalizedDomain::from_host("acme.com").unwrap(),
            agent: agent.map(crate::domain::profile::definition::AgentId::new),
        }
    }

    #[test]
    fn cache_keys_differ_per_agent() {
        let prompt = Prompt {
            system: "system",
            instruction: "instruction",
            documents: &[],
            max_tokens: 1,
        };
        let schema = serde_json::json!({});
        let key = |agent| cache_key(&scope_of(agent), &prompt, &schema);
        assert_ne!(key(Some("ats-fr")), key(Some("ats-en")));
        assert_ne!(key(Some("ats-fr")), key(None));
        assert_eq!(key(Some("ats-fr")), key(Some("ats-fr")));
    }

    #[derive(Debug, PartialEq, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct Verdict {
        relevant: bool,
        reason: String,
    }

    struct Scripted {
        replies: Mutex<VecDeque<Result<serde_json::Value, LlmError>>>,
        calls: Mutex<u32>,
        schemas: Mutex<Vec<serde_json::Value>>,
    }

    impl Scripted {
        fn new(replies: Vec<Result<serde_json::Value, LlmError>>) -> Self {
            Self {
                replies: Mutex::new(replies.into()),
                calls: Mutex::new(0),
                schemas: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> u32 {
            *self.calls.lock().unwrap()
        }
    }

    impl LlmProvider for Scripted {
        async fn complete(&self, request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
            *self.calls.lock().unwrap() += 1;
            self.schemas.lock().unwrap().push(request.schema.clone());
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Err(LlmError::Status { status: 500 }));
            reply.map(|output| LlmResponse {
                output,
                model: "test-model".to_owned(),
                usage: TokenUsage {
                    input_tokens: 1_000,
                    output_tokens: 100,
                },
            })
        }
    }

    fn settings(max_retries: u8) -> CallSettings {
        CallSettings {
            max_retries,
            pricing: TokenPricing {
                input_cents_per_mtok: 300,
                output_cents_per_mtok: 1_500,
            },
            base_delay: Duration::from_secs(1),
        }
    }

    fn prompt() -> Prompt<'static> {
        Prompt {
            system: "system",
            instruction: "decide",
            documents: &[],
            max_tokens: 256,
        }
    }

    fn budget(cents: u32) -> RunBudget {
        RunBudget::new(NonZeroU32::new(cents).unwrap())
    }

    fn valid() -> serde_json::Value {
        serde_json::json!({"relevant": true, "reason": "mentions Keycloak"})
    }

    #[tokio::test(start_paused = true)]
    async fn valid_output_is_typed_and_charged() {
        let provider = Scripted::new(vec![Ok(valid())]);
        let mut budget = budget(10);
        let result: Structured<Verdict> =
            call_structured(&provider, prompt(), settings(2), &mut budget)
                .await
                .unwrap();
        assert!(result.value.relevant);
        assert!((budget.spent_cents() - 0.45).abs() < 1e-9);
        let schema = &provider.schemas.lock().unwrap()[0];
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    }

    #[tokio::test(start_paused = true)]
    async fn invalid_output_is_retried_then_rejected_and_never_returned() {
        let wrong = serde_json::json!({"relevant": "yes"});
        let provider = Scripted::new(vec![Ok(wrong.clone()), Ok(wrong.clone()), Ok(wrong)]);
        let mut budget = budget(10);
        let result =
            call_structured::<Verdict, _>(&provider, prompt(), settings(2), &mut budget).await;
        assert!(matches!(result, Err(LlmCallError::InvalidOutput(_))));
        assert_eq!(provider.calls(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn transient_errors_are_retried_with_backoff() {
        let provider = Scripted::new(vec![
            Err(LlmError::Timeout),
            Err(LlmError::RateLimited {
                retry_after: Some(Duration::from_secs(7)),
            }),
            Ok(valid()),
        ]);
        let started = tokio::time::Instant::now();
        let mut budget = budget(10);
        let result =
            call_structured::<Verdict, _>(&provider, prompt(), settings(3), &mut budget).await;
        assert!(result.is_ok());
        assert_eq!(provider.calls(), 3);
        assert_eq!(started.elapsed(), Duration::from_secs(1 + 7));
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_beyond_the_retry_bound_fails() {
        let provider = Scripted::new(vec![
            Err(LlmError::Timeout),
            Err(LlmError::Timeout),
            Err(LlmError::Timeout),
        ]);
        let mut budget = budget(10);
        let result =
            call_structured::<Verdict, _>(&provider, prompt(), settings(1), &mut budget).await;
        assert_eq!(result, Err(LlmCallError::Provider(LlmError::Timeout)));
        assert_eq!(provider.calls(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn refusal_and_client_errors_are_not_retried() {
        for error in [
            LlmError::Refused,
            LlmError::Truncated,
            LlmError::Status { status: 400 },
        ] {
            let provider = Scripted::new(vec![Err(error.clone()), Ok(valid())]);
            let mut budget = budget(10);
            let result =
                call_structured::<Verdict, _>(&provider, prompt(), settings(3), &mut budget).await;
            assert_eq!(result, Err(LlmCallError::Provider(error)));
            assert_eq!(provider.calls(), 1);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn exhausted_budget_stops_calls() {
        let provider = Scripted::new(vec![Ok(valid()), Ok(valid()), Ok(valid()), Ok(valid())]);
        let mut budget = budget(1);
        for _ in 0..3 {
            call_structured::<Verdict, _>(&provider, prompt(), settings(0), &mut budget)
                .await
                .unwrap();
        }
        let fourth =
            call_structured::<Verdict, _>(&provider, prompt(), settings(0), &mut budget).await;
        assert_eq!(fourth, Err(LlmCallError::BudgetExhausted));
        assert_eq!(provider.calls(), 3);
        assert_eq!(
            LlmCallError::BudgetExhausted.to_string(),
            "llm budget exhausted"
        );
    }

    #[test]
    fn budget_charges_input_and_output_prices() {
        let mut budget = budget(100);
        budget.charge(
            TokenUsage {
                input_tokens: 2_000_000,
                output_tokens: 1_000_000,
            },
            TokenPricing {
                input_cents_per_mtok: 300,
                output_cents_per_mtok: 1_500,
            },
        );
        assert!((budget.spent_cents() - 2_100.0).abs() < 1e-9);
        assert!(budget.is_exhausted());
    }
}
