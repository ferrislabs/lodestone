use std::{
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use chrono::{TimeZone, Utc};
use common::{CoreError, Secret};
use httpmock::prelude::{MockServer, POST};
use llm::openai::OpenAiCompatibleProvider;
use lodestone_core::{
    ActiveProfile, AgentRegistry, CacheScope, CachedOutput, CancelReason, GenerationParams,
    LlmCache, NormalizedDomain, Profile, ProfileId, ProfileName, ProfileRevision,
    ProviderDefinition, ProviderKind, RevisionNumber, TokenPricing, UntrustedDocument,
    ValidatedProfile, WorkKind, WorkTarget,
    definition::AgentId,
    infrastructure::agent_registry::InMemoryAgentRegistry,
    llm::gateway::{
        CallSettings, LlmCallError, Prompt, RunBudget, SchemaCall, call_with_schema_until,
    },
};
use serde_json::{Value, from_str, json};
use tokio::{spawn, time::sleep};
use url::Url;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");

#[derive(Default)]
struct MemoryCache {
    entries: Mutex<Vec<Vec<u8>>>,
}

impl LlmCache for MemoryCache {
    async fn get(&self, _key: &[u8]) -> Result<Option<CachedOutput>, CoreError> {
        Ok(None)
    }

    async fn put(
        &self,
        key: &[u8],
        _scope: &CacheScope,
        _entry: &CachedOutput,
    ) -> Result<(), CoreError> {
        self.entries.lock().unwrap().push(key.to_vec());
        Ok(())
    }
}

fn profile(revision: u32, edit: impl FnOnce(&mut Value)) -> ActiveProfile {
    let mut document: Value = from_str(IAM).unwrap();
    edit(&mut document);
    let at = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
    let id: ProfileId = "00000000-0000-0000-0000-00000000002a".parse().unwrap();
    ActiveProfile {
        profile: Profile {
            id,
            name: ProfileName::parse("IAM").unwrap(),
            active: true,
            created_at: at,
            updated_at: at,
        },
        revision: ProfileRevision {
            profile_id: id,
            number: RevisionNumber::new(revision).unwrap(),
            definition: ValidatedProfile::parse(document).unwrap(),
            created_by: None,
            created_at: at,
        },
    }
}

fn provider(server: &MockServer) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(&ProviderDefinition {
        kind: ProviderKind::OpenAiCompatible,
        base_url: Url::parse(&server.base_url()).unwrap(),
        model: "test-model".to_owned(),
        api_key: Some(Secret::new("sk-test")),
        timeout: Duration::from_secs(30),
        reasoning_effort: None,
        generation: GenerationParams::default(),
    })
    .unwrap()
}

#[tokio::test]
async fn an_llm_call_in_flight_is_dropped_when_its_agent_is_removed() {
    let server = MockServer::start();
    let completion = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200)
            .delay(Duration::from_secs(3))
            .header("content-type", "application/json")
            .json_body(json!({
                "model": "served-model",
                "choices": [{
                    "message": {"role": "assistant", "content": "{\"ok\": true}"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 1000000, "completion_tokens": 1000000}
            }));
    });
    let registry = Arc::new(InMemoryAgentRegistry::new());
    let current = profile(1, |_| {});
    registry.publish(&current);
    let target = WorkTarget::new(WorkKind::Qualification, &AgentId::new("marche"));
    let work = registry.token(current.profile.id, current.revision.number, target);
    let removal = Arc::clone(&registry);
    let remover = spawn(async move {
        sleep(Duration::from_millis(300)).await;
        removal.publish(&profile(2, |document| {
            document["qualification_agents"]
                .as_array_mut()
                .unwrap()
                .retain(|agent| agent["id"] != "marche");
        }));
    });
    let llm = provider(&server);
    let cache = MemoryCache::default();
    let schema =
        json!({"type": "object", "properties": {"ok": {"type": "boolean"}}, "required": ["ok"]});
    let documents = [UntrustedDocument {
        source: "lead",
        content: "acme.com",
    }];
    let scope = CacheScope {
        profile_id: current.profile.id,
        revision: current.revision.number,
        prompt: "qualification@test".to_owned(),
        domain: NormalizedDomain::from_host("acme.com").unwrap(),
        agent: Some(AgentId::new("marche")),
        sample: 0,
        fresh: false,
    };
    let mut budget = RunBudget::new(NonZeroU32::new(100).unwrap());
    let started = Instant::now();
    let result = call_with_schema_until(
        &llm,
        &cache,
        SchemaCall {
            scope: &scope,
            prompt: Prompt {
                system: "system",
                instruction: "instruction",
                documents: &documents,
                max_tokens: 64,
                temperature: None,
            },
            schema: &schema,
        },
        |_| true,
        CallSettings {
            max_retries: 0,
            pricing: TokenPricing {
                input_cents_per_mtok: 100,
                output_cents_per_mtok: 100,
            },
            base_delay: Duration::from_millis(10),
        },
        &mut budget,
        &work,
    )
    .await;
    remover.await.unwrap();
    assert_eq!(
        result,
        Err(LlmCallError::Cancelled(CancelReason::AgentRemoved))
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(completion.hits(), 1);
    assert!(cache.entries.lock().unwrap().is_empty());
    assert_eq!(budget.spent_cents(), 0.0);
}

#[tokio::test]
async fn a_call_for_an_already_removed_agent_never_reaches_the_provider() {
    let server = MockServer::start();
    let completion = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(500);
    });
    let registry = InMemoryAgentRegistry::new();
    let current = profile(1, |_| {});
    registry.publish(&current);
    let work = registry.token(
        current.profile.id,
        current.revision.number,
        WorkTarget::new(WorkKind::Scoring, &AgentId::new("absent")),
    );
    let cache = MemoryCache::default();
    let schema = json!({"type": "object"});
    let scope = CacheScope {
        profile_id: current.profile.id,
        revision: current.revision.number,
        prompt: "scoring@test".to_owned(),
        domain: NormalizedDomain::from_host("acme.com").unwrap(),
        agent: Some(AgentId::new("absent")),
        sample: 0,
        fresh: false,
    };
    let mut budget = RunBudget::new(NonZeroU32::new(100).unwrap());
    let result = call_with_schema_until(
        &provider(&server),
        &cache,
        SchemaCall {
            scope: &scope,
            prompt: Prompt {
                system: "system",
                instruction: "instruction",
                documents: &[],
                max_tokens: 64,
                temperature: None,
            },
            schema: &schema,
        },
        |_| true,
        CallSettings {
            max_retries: 0,
            pricing: TokenPricing {
                input_cents_per_mtok: 100,
                output_cents_per_mtok: 100,
            },
            base_delay: Duration::from_millis(10),
        },
        &mut budget,
        &work,
    )
    .await;
    assert_eq!(
        result,
        Err(LlmCallError::Cancelled(CancelReason::AgentRemoved))
    );
    assert_eq!(completion.hits(), 0);
}
