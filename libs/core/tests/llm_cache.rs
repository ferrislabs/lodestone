mod support;

use std::{
    num::NonZeroU32,
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use lodestone_core::{
    CacheScope, LlmError, LlmProvider, LlmRequest, LlmResponse, NormalizedDomain, ProfileId,
    RevisionNumber, TokenPricing, TokenUsage, UntrustedDocument,
    infrastructure::llm_cache::postgres::PgLlmCache,
    llm::gateway::{CallSettings, Prompt, RunBudget, call_structured_cached},
};
use sqlx::PgPool;
use support::create_profile;

#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct Verdict {
    relevant: bool,
    reason: String,
}

struct Counting {
    calls: AtomicU32,
}

impl LlmProvider for Counting {
    async fn complete(&self, _request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(LlmResponse {
            output: serde_json::json!({
                "relevant": false, "reason": "nothing about identity"
            }),
            model: "test-model".to_owned(),
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 10,
            },
        })
    }
}

fn scope(profile: ProfileId, revision: u32) -> CacheScope {
    CacheScope {
        profile_id: profile,
        revision: RevisionNumber::new(revision).unwrap(),
        prompt: "verdict@1".to_owned(),
        domain: NormalizedDomain::from_host("acme.com").unwrap(),
        agent: None,
        sample: 0,
        fresh: false,
    }
}

fn settings() -> CallSettings {
    CallSettings {
        max_retries: 0,
        pricing: TokenPricing {
            input_cents_per_mtok: 0,
            output_cents_per_mtok: 0,
        },
        base_delay: Duration::ZERO,
    }
}

async fn ask(provider: &Counting, cache: &PgLlmCache, scope: &CacheScope, content: &str) -> bool {
    let documents = [UntrustedDocument {
        source: "https://acme.com/jobs",
        content,
    }];
    let prompt = Prompt {
        system: "system",
        instruction: "decide",
        documents: &documents,
        max_tokens: 256,
        temperature: None,
    };
    let budget = RunBudget::new(NonZeroU32::new(10).unwrap());
    call_structured_cached::<Verdict, _, _>(provider, cache, scope, prompt, settings(), &budget)
        .await
        .unwrap()
        .from_cache
}

#[sqlx::test(migrations = "../../migrations")]
async fn unchanged_content_is_served_from_the_cache(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let cache = PgLlmCache::new(pool);
    let provider = Counting {
        calls: AtomicU32::new(0),
    };

    assert!(!ask(&provider, &cache, &scope(profile, 1), "We sell bread.").await);
    assert!(ask(&provider, &cache, &scope(profile, 1), "We sell bread.").await);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn changed_content_misses(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let cache = PgLlmCache::new(pool);
    let provider = Counting {
        calls: AtomicU32::new(0),
    };

    ask(&provider, &cache, &scope(profile, 1), "We sell bread.").await;
    assert!(
        !ask(
            &provider,
            &cache,
            &scope(profile, 1),
            "We sell bread and cakes."
        )
        .await
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_revision_misses_only_for_its_profile(pool: PgPool) {
    let alpha = create_profile(&pool, "alpha").await;
    let beta = create_profile(&pool, "beta").await;
    let cache = PgLlmCache::new(pool);
    let provider = Counting {
        calls: AtomicU32::new(0),
    };

    ask(&provider, &cache, &scope(alpha, 1), "We sell bread.").await;
    ask(&provider, &cache, &scope(beta, 1), "We sell bread.").await;
    assert!(!ask(&provider, &cache, &scope(alpha, 2), "We sell bread.").await);
    assert!(ask(&provider, &cache, &scope(beta, 1), "We sell bread.").await);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
}
