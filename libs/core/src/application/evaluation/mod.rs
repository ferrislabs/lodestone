use std::{num::NonZeroU32, time::Duration};

use chrono::{DateTime, SecondsFormat, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::transactional;
use tracing::{info, warn};

use crate::{
    application::{
        LodestoneUseCase,
        llm::gateway::{
            CallSettings, LlmCallError, Prompt, RunBudget, SchemaCall, Structured, call_with_schema,
        },
    },
    domain::{
        job::ports::Clock,
        lead::{Lead, LeadId, Signal, ports::SignalRepository},
        llm::{
            LlmError, LlmProvider, ProviderDefinition, UntrustedDocument,
            cache::{CacheScope, LlmCache},
            provider::{ProviderId, SecretCipher, Tier, TierError, TokenPricing},
        },
        profile::{
            ActiveProfile,
            contract::contract_schema,
            definition::{ProfileDefinition, QualificationAgent},
        },
        qualification::{
            Completion, QualificationResult, ResultId, ResultStatus, ValidAnswers, Violations,
            agent_score, judge,
            ports::QualificationRepository,
            prompt::{AgentPrompt, build, lead_document, with_violations},
            validate_answers,
        },
    },
};

pub const QUALIFICATION_LISTENER: &str = "qualification";
const PROVIDER_UNAVAILABLE: &str = "fournisseur indisponible";

pub trait ProviderBuilder: Send + Sync {
    type Provider: LlmProvider;

    fn build(&self, definition: &ProviderDefinition) -> Result<Self::Provider, LlmError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvaluationSettings {
    pub max_tokens: u32,
    pub budget_cents: NonZeroU32,
    pub batch_size: u32,
    pub base_delay: Duration,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EvaluationReport {
    pub leads: u64,
    pub valid: u64,
    pub invalid: u64,
    pub backlog: u64,
    pub budget_exhausted: bool,
}

pub(crate) struct Loaded<P> {
    pub(crate) provider: P,
    pub(crate) id: ProviderId,
    pub(crate) model: String,
    pub(crate) max_retries: u8,
    pub(crate) pricing: TokenPricing,
}

pub(crate) async fn load_provider<B, S>(
    usecase: &LodestoneUseCase,
    builder: &B,
    cipher: &S,
    wanted: Option<ProviderId>,
    default_tier: Tier,
    label: &str,
) -> Result<Option<Loaded<B::Provider>>, CoreError>
where
    B: ProviderBuilder,
    S: SecretCipher,
{
    let resolved = match wanted {
        Some(id) => usecase.resolve_llm_provider(id, cipher).await?,
        None => match usecase.resolve_llm_tier(default_tier, cipher).await {
            Ok(resolved) => Some(resolved),
            Err(TierError::Storage(message)) => return Err(CoreError::Internal(message)),
            Err(_) => None,
        },
    };
    Ok(
        resolved.and_then(|resolved| match builder.build(&resolved.definition) {
            Ok(provider) => Some(Loaded {
                provider,
                id: resolved.id,
                model: resolved.definition.model.clone(),
                max_retries: resolved.max_retries,
                pricing: resolved.pricing,
            }),
            Err(error) => {
                warn!(label, %error, "qualification provider could not be built");
                None
            }
        }),
    )
}

pub fn render_signal(signal: &Signal) -> String {
    let raw = serde_json::to_string_pretty(&signal.raw).unwrap_or_else(|_| signal.raw.to_string());
    format!(
        "signal: {}\nvalue: {}\nsource: {}\nobserved: {}\n{}",
        signal.kind.as_str(),
        signal.value.as_deref().unwrap_or("-"),
        signal.source.as_str(),
        signal
            .observed_at
            .to_rfc3339_opts(SecondsFormat::Secs, true),
        raw,
    )
}

struct Providers<P> {
    entries: Vec<(Option<ProviderId>, Option<Loaded<P>>)>,
}

impl<P> Providers<P> {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn position(&self, wanted: Option<ProviderId>) -> Option<usize> {
        self.entries.iter().position(|(key, _)| *key == wanted)
    }
}

struct Evaluator<'a, B, S, C> {
    builder: &'a B,
    cipher: &'a S,
    cache: &'a C,
    settings: EvaluationSettings,
}

enum AgentOutcome {
    Result(Box<QualificationResult>, Option<f64>),
    BudgetExhausted,
}

enum LeadOutcome {
    Done { valid: u64, invalid: u64 },
    BudgetExhausted,
}

struct Subject<'a> {
    active: &'a ActiveProfile,
    definition: &'a ProfileDefinition,
    lead: &'a Lead,
    documents: &'a [UntrustedDocument<'a>],
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn failed(
    subject: &Subject<'_>,
    agent: &QualificationAgent,
    prompt: &AgentPrompt,
    provider: Option<ProviderId>,
    model: &str,
    error: String,
    at: DateTime<Utc>,
) -> QualificationResult {
    QualificationResult {
        id: ResultId(generate_uuid_v7()),
        lead_id: subject.lead.id,
        profile_id: subject.active.profile.id,
        profile_revision: subject.active.revision.number,
        agent_id: agent.id.clone(),
        provider_id: provider,
        model: model.to_owned(),
        prompt_hash: prompt.hash.clone(),
        status: ResultStatus::Invalid,
        answers: serde_json::json!({}),
        agent_score: None,
        error: Some(error),
        created_at: at,
    }
}

fn succeeded(
    subject: &Subject<'_>,
    agent: &QualificationAgent,
    prompt: &AgentPrompt,
    provider: ProviderId,
    model: &str,
    answers: &ValidAnswers,
    at: DateTime<Utc>,
) -> (QualificationResult, f64) {
    let score = agent_score(agent, answers);
    let result = QualificationResult {
        id: ResultId(generate_uuid_v7()),
        lead_id: subject.lead.id,
        profile_id: subject.active.profile.id,
        profile_revision: subject.active.revision.number,
        agent_id: agent.id.clone(),
        provider_id: Some(provider),
        model: model.to_owned(),
        prompt_hash: prompt.hash.clone(),
        status: ResultStatus::Valid,
        answers: answers.to_json(),
        agent_score: Some(score),
        error: None,
        created_at: at,
    };
    (result, score)
}

impl<B, S, C> Evaluator<'_, B, S, C>
where
    B: ProviderBuilder,
    S: SecretCipher,
    C: LlmCache,
{
    async fn load(
        &self,
        usecase: &LodestoneUseCase,
        agent: &QualificationAgent,
        providers: &mut Providers<B::Provider>,
    ) -> Result<Option<usize>, CoreError> {
        let wanted = agent.provider_id.map(ProviderId);
        if let Some(index) = providers.position(wanted) {
            return Ok(providers.entries[index].1.is_some().then_some(index));
        }
        let loaded = load_provider(
            usecase,
            self.builder,
            self.cipher,
            wanted,
            Tier::Triage,
            agent.id.as_str(),
        )
        .await?;
        let available = loaded.is_some();
        providers.entries.push((wanted, loaded));
        Ok(available.then(|| providers.entries.len() - 1))
    }

    async fn ask(
        &self,
        loaded: &Loaded<B::Provider>,
        subject: &Subject<'_>,
        agent: &QualificationAgent,
        prompt: &AgentPrompt,
        instruction: &str,
        budget: &mut RunBudget,
    ) -> Result<Structured<serde_json::Value>, LlmCallError> {
        let schema = contract_schema(agent);
        let scope = CacheScope {
            profile_id: subject.active.profile.id,
            revision: subject.active.revision.number,
            prompt: format!("qualification@{}", prompt.hash),
            domain: subject.lead.domain.clone(),
            agent: Some(agent.id.clone()),
        };
        let call = SchemaCall {
            scope: &scope,
            prompt: Prompt {
                system: &prompt.system,
                instruction,
                documents: subject.documents,
                max_tokens: self.settings.max_tokens,
            },
            schema: &schema,
        };
        call_with_schema(
            &loaded.provider,
            self.cache,
            call,
            |output| validate_answers(agent, output).is_ok(),
            CallSettings {
                max_retries: loaded.max_retries,
                pricing: loaded.pricing,
                base_delay: self.settings.base_delay,
            },
            budget,
        )
        .await
    }

    async fn evaluate_agent(
        &self,
        usecase: &LodestoneUseCase,
        subject: &Subject<'_>,
        agent: &QualificationAgent,
        providers: &mut Providers<B::Provider>,
        budget: &mut RunBudget,
        at: DateTime<Utc>,
    ) -> Result<AgentOutcome, CoreError> {
        let schema = contract_schema(agent);
        let prompt = build(subject.definition, agent, &schema);
        let requested = agent.provider_id.map(ProviderId);
        let Some(index) = self.load(usecase, agent, providers).await? else {
            let result = failed(
                subject,
                agent,
                &prompt,
                requested,
                "",
                PROVIDER_UNAVAILABLE.to_owned(),
                at,
            );
            return Ok(AgentOutcome::Result(Box::new(result), None));
        };
        let Some(loaded) = providers.entries[index].1.as_ref() else {
            return Err(CoreError::Internal(
                "provider cache is inconsistent".to_owned(),
            ));
        };
        let mut instruction = prompt.instruction.clone();
        let mut last: Option<Violations> = None;
        for _ in 0..2 {
            let structured = match self
                .ask(loaded, subject, agent, &prompt, &instruction, budget)
                .await
            {
                Ok(structured) => structured,
                Err(LlmCallError::BudgetExhausted) => return Ok(AgentOutcome::BudgetExhausted),
                Err(error) => {
                    let result = failed(
                        subject,
                        agent,
                        &prompt,
                        Some(loaded.id),
                        &loaded.model,
                        format!("Échec de l'appel au modèle : {error}"),
                        at,
                    );
                    return Ok(AgentOutcome::Result(Box::new(result), None));
                }
            };
            match validate_answers(agent, &structured.value) {
                Ok(answers) => {
                    let (result, score) = succeeded(
                        subject,
                        agent,
                        &prompt,
                        loaded.id,
                        &structured.model,
                        &answers,
                        at,
                    );
                    return Ok(AgentOutcome::Result(Box::new(result), Some(score)));
                }
                Err(violations) => {
                    warn!(agent = agent.id.as_str(), %violations, "qualification answer broke the contract");
                    instruction = with_violations(&prompt.instruction, &violations);
                    last = Some(violations);
                }
            }
        }
        let message = last.map_or_else(String::new, |violations| violations.to_string());
        let result = failed(
            subject,
            agent,
            &prompt,
            Some(loaded.id),
            &loaded.model,
            format!("Réponse non conforme au contrat : {message}"),
            at,
        );
        Ok(AgentOutcome::Result(Box::new(result), None))
    }

    async fn evaluate_lead<K: Clock>(
        &self,
        usecase: &LodestoneUseCase,
        active: &ActiveProfile,
        lead: &Lead,
        providers: &mut Providers<B::Provider>,
        budget: &mut RunBudget,
        clock: &K,
    ) -> Result<LeadOutcome, CoreError> {
        let definition = active.revision.definition.definition();
        let signals = usecase.evaluation_signals(lead.id).await?;
        let overview = lead_document(lead.domain.as_str(), lead.first_seen_at);
        let contents: Vec<String> = signals.iter().map(render_signal).collect();
        let mut documents = Vec::with_capacity(signals.len() + 1);
        documents.push(UntrustedDocument {
            source: "lead",
            content: &overview,
        });
        documents.extend(signals.iter().zip(&contents).map(|(signal, content)| {
            UntrustedDocument {
                source: signal.provenance.as_str(),
                content,
            }
        }));
        let subject = Subject {
            active,
            definition,
            lead,
            documents: &documents,
        };
        let at = clock.now();
        let mut results = Vec::new();
        let mut scored: Vec<(u8, f64)> = Vec::new();
        for agent in definition
            .qualification_agents
            .iter()
            .filter(|agent| agent.enabled)
        {
            match self
                .evaluate_agent(usecase, &subject, agent, providers, budget, at)
                .await?
            {
                AgentOutcome::BudgetExhausted => return Ok(LeadOutcome::BudgetExhausted),
                AgentOutcome::Result(result, score) => {
                    let status = result.status.as_str();
                    info!(
                        monotonic_counter.qualification_results = 1u64,
                        agent = agent.id.as_str(),
                        status,
                        "qualification result recorded"
                    );
                    if let Some(score) = score {
                        scored.push((agent.weight, score));
                    }
                    results.push(*result);
                }
            }
        }
        let valid = count(scored.len());
        let invalid = count(results.len()) - valid;
        let mut sources: Vec<String> = signals
            .iter()
            .map(|signal| signal.source.as_str().to_owned())
            .collect();
        sources.sort();
        sources.dedup();
        let completion = Completion {
            lead_id: lead.id,
            profile_revision: active.revision.number,
            sources,
            judgement: judge(&scored, definition.verdict),
            at,
        };
        usecase.store_evaluation(&results, &completion).await?;
        Ok(LeadOutcome::Done { valid, invalid })
    }
}

impl LodestoneUseCase {
    pub async fn has_enabled_qualification_agent(&self) -> Result<bool, CoreError> {
        Ok(self.list_active_profiles().await?.iter().any(|active| {
            active
                .revision
                .definition
                .definition()
                .qualification_agents
                .iter()
                .any(|agent| agent.enabled)
        }))
    }

    pub async fn qualify_due_leads<B, S, C, K>(
        &self,
        builder: &B,
        cipher: &S,
        cache: &C,
        settings: EvaluationSettings,
        clock: &K,
    ) -> Result<EvaluationReport, CoreError>
    where
        B: ProviderBuilder,
        S: SecretCipher,
        C: LlmCache,
        K: Clock,
    {
        let evaluator = Evaluator {
            builder,
            cipher,
            cache,
            settings,
        };
        let profiles: Vec<ActiveProfile> = self
            .list_active_profiles()
            .await?
            .into_iter()
            .filter(|active| {
                active
                    .revision
                    .definition
                    .definition()
                    .qualification_agents
                    .iter()
                    .any(|agent| agent.enabled)
            })
            .collect();
        let mut report = EvaluationReport::default();
        let mut budget = RunBudget::new(settings.budget_cents);
        let mut providers = Providers::new();
        let mut remaining = settings.batch_size;
        'profiles: for active in &profiles {
            if remaining == 0 {
                break;
            }
            let leads = self.due_leads(active, remaining).await?;
            for lead in &leads {
                let outcome = evaluator
                    .evaluate_lead(self, active, lead, &mut providers, &mut budget, clock)
                    .await?;
                match outcome {
                    LeadOutcome::Done { valid, invalid } => {
                        report.leads += 1;
                        report.valid += valid;
                        report.invalid += invalid;
                        remaining = remaining.saturating_sub(1);
                    }
                    LeadOutcome::BudgetExhausted => {
                        report.budget_exhausted = true;
                        warn!("qualification stopped, the run budget is exhausted");
                        break 'profiles;
                    }
                }
            }
        }
        report.backlog = self.due_backlog(&profiles).await?;
        Ok(report)
    }

    async fn due_backlog(&self, profiles: &[ActiveProfile]) -> Result<u64, CoreError> {
        let mut backlog = 0;
        for active in profiles {
            backlog += self.count_due_leads(active).await?;
        }
        Ok(backlog)
    }

    #[transactional(qualification)]
    async fn due_leads(&self, active: &ActiveProfile, limit: u32) -> Result<Vec<Lead>, CoreError> {
        let mut repo = qualification_repository;
        repo.due_leads(active.profile.id, active.revision.number, limit)
            .await
    }

    #[transactional(qualification)]
    async fn count_due_leads(&self, active: &ActiveProfile) -> Result<u64, CoreError> {
        let mut repo = qualification_repository;
        repo.count_due(active.profile.id, active.revision.number)
            .await
    }

    #[transactional(signal)]
    async fn evaluation_signals(&self, lead_id: LeadId) -> Result<Vec<Signal>, CoreError> {
        let mut repo = signal_repository;
        repo.list_for_lead(lead_id).await
    }

    #[transactional(qualification)]
    async fn store_evaluation(
        &self,
        results: &[QualificationResult],
        completion: &Completion,
    ) -> Result<(), CoreError> {
        let mut repo = qualification_repository;
        for result in results {
            repo.store_result(result).await?;
        }
        repo.complete(completion).await
    }

    #[transactional(qualification)]
    pub async fn lead_qualification_results(
        &self,
        lead_id: LeadId,
    ) -> Result<Vec<QualificationResult>, CoreError> {
        let mut repo = qualification_repository;
        repo.current_results(lead_id).await
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;
    use crate::domain::{
        lead::SignalId,
        values::{Provenance, SignalKind, SourceName, UnitInterval},
    };

    fn signal(value: Option<&str>) -> Signal {
        Signal {
            id: SignalId(uuid::Uuid::nil()),
            lead_id: LeadId(uuid::Uuid::nil()),
            kind: SignalKind::parse("product_detected").unwrap(),
            value: value.map(str::to_owned),
            source: SourceName::HttpProbe,
            raw: json!({"version": "21.1"}),
            provenance: Provenance::parse("https://example.com/realms/master").unwrap(),
            strength: UnitInterval::new(0.5).unwrap(),
            observed_at: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
            agent: None,
        }
    }

    #[test]
    fn rendering_is_stable_plain_text() {
        assert_eq!(
            render_signal(&signal(Some("keycloak"))),
            "signal: product_detected\nvalue: keycloak\nsource: http_probe\nobserved: 2027-01-15T08:00:00Z\n{\n  \"version\": \"21.1\"\n}"
        );
    }

    #[test]
    fn a_missing_value_renders_a_dash() {
        assert!(render_signal(&signal(None)).contains("\nvalue: -\n"));
    }
}
