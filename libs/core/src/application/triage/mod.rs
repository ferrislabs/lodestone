use std::num::NonZeroU32;

use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::transactional;
use serde_json::json;
use tracing::{debug, warn};

use crate::{
    application::{
        LodestoneUseCase,
        llm::gateway::{CallSettings, LlmCallError, Prompt, RunBudget, call_structured_cached},
        sources::SourceRunError,
    },
    domain::{
        contact::FoundAddress,
        job::ports::Clock,
        lead::ContactMethod,
        llm::{
            LlmProvider, UntrustedDocument,
            cache::{CacheScope, LlmCache},
        },
        profile::{
            ActiveProfile,
            definition::{KeywordSource, ProfileDefinition},
        },
        prompt::{TRIAGE_V1, TriageVerdict},
        run::{Run, RunId, RunOutcome, ports::RunRepository},
        source::{Observation, SourceError},
        triage::{Candidate, Route, route},
        values::{EmailAddress, SourceName, UnitInterval},
    },
};

const TITLE_MAX_CHARS: usize = 200;
const RULES_STRENGTH: f64 = 0.7;
const LLM_STRENGTH: f64 = 0.6;
const CONTACT_CONFIDENCE: f64 = 0.5;
const BUDGET_EXHAUSTED: &str = "llm budget exhausted";

pub trait CandidateSource: Send + Sync {
    fn name(&self) -> SourceName;

    fn rules<'a>(&self, profile: &'a ProfileDefinition) -> Option<&'a KeywordSource>;

    fn collect(
        &self,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<Candidate>, SourceError>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriageSettings {
    pub call: CallSettings,
    pub max_tokens: u32,
    pub budget_cents: NonZeroU32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TriageReport {
    pub candidates: u64,
    pub accepted_by_rules: u64,
    pub rejected_by_rules: u64,
    pub sent_to_llm: u64,
    pub accepted_by_llm: u64,
    pub deferred: u64,
    pub budget_exhausted: bool,
}

struct Decided {
    observations: Vec<Observation>,
    contacts: Vec<(crate::domain::NormalizedDomain, FoundAddress)>,
    budget_exhausted: bool,
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn strength(value: f64) -> Result<UnitInterval, CoreError> {
    UnitInterval::new(value).map_err(|_| CoreError::Internal("invalid strength".to_owned()))
}

fn short_title(candidate: &Candidate) -> String {
    candidate.title.chars().take(TITLE_MAX_CHARS).collect()
}

fn candidate_text(candidate: &Candidate) -> String {
    format!("{}\n\n{}", candidate.title, candidate.text)
}

struct TriageLlm<'a, P, C> {
    provider: Option<&'a P>,
    cache: &'a C,
    settings: TriageSettings,
}

impl LodestoneUseCase {
    pub async fn run_triage<S, P, C, K>(
        &self,
        source: &S,
        provider: Option<&P>,
        cache: &C,
        settings: TriageSettings,
        clock: &K,
    ) -> Result<TriageReport, SourceRunError>
    where
        S: CandidateSource,
        P: LlmProvider,
        C: LlmCache,
        K: Clock,
    {
        let name = source.name();
        let profiles: Vec<ActiveProfile> = self
            .list_active_profiles()
            .await?
            .into_iter()
            .filter(|active| {
                source
                    .rules(active.revision.definition.definition())
                    .is_some()
            })
            .collect();
        if profiles.is_empty() {
            return Ok(TriageReport::default());
        }

        let started_at = clock.now();
        let runs: Vec<Run> = profiles
            .iter()
            .map(|active| {
                Run::start(
                    RunId(generate_uuid_v7()),
                    active.profile.id,
                    name,
                    started_at,
                )
            })
            .collect();
        self.insert_triage_runs(&runs).await?;

        let candidates = match source.collect(&profiles, started_at).await {
            Ok(candidates) => candidates,
            Err(error) => {
                let outcome = RunOutcome::Failed {
                    volume: 0,
                    signals: 0,
                    error: error.to_string(),
                };
                for run in &runs {
                    self.finish_triage_run(run.id, &outcome, clock.now())
                        .await?;
                }
                return Err(error.into());
            }
        };

        let volume = count(candidates.len());
        let mut report = TriageReport {
            candidates: volume,
            ..TriageReport::default()
        };
        let llm = TriageLlm {
            provider,
            cache,
            settings,
        };
        let mut first_error: Option<CoreError> = None;
        for (active, run) in profiles.iter().zip(&runs) {
            let Some(rules) = source.rules(active.revision.definition.definition()) else {
                continue;
            };
            let decided = match self
                .decide(active, rules, &candidates, &llm, &mut report)
                .await
            {
                Ok(decided) => decided,
                Err(error) => {
                    let outcome = RunOutcome::Failed {
                        volume,
                        signals: 0,
                        error: error.to_string(),
                    };
                    self.finish_triage_run(run.id, &outcome, clock.now())
                        .await?;
                    first_error.get_or_insert(error);
                    continue;
                }
            };
            let signals = count(decided.observations.len());
            let recorded = self.record_decided(active, name, &decided, clock).await;
            let outcome = match &recorded {
                Ok(()) if decided.budget_exhausted => RunOutcome::Failed {
                    volume,
                    signals,
                    error: BUDGET_EXHAUSTED.to_owned(),
                },
                Ok(()) => RunOutcome::Succeeded { volume, signals },
                Err(error) => RunOutcome::Failed {
                    volume,
                    signals: 0,
                    error: error.to_string(),
                },
            };
            self.finish_triage_run(run.id, &outcome, clock.now())
                .await?;
            if let Err(error) = recorded {
                first_error.get_or_insert(error);
            }
            report.budget_exhausted |= decided.budget_exhausted;
        }
        match first_error {
            Some(error) => Err(error.into()),
            None => Ok(report),
        }
    }

    async fn record_decided<K: Clock>(
        &self,
        active: &ActiveProfile,
        name: SourceName,
        decided: &Decided,
        clock: &K,
    ) -> Result<(), CoreError> {
        self.record_observations(active, name, &decided.observations)
            .await?;
        for (domain, address) in &decided.contacts {
            self.record_contacts(
                active,
                domain,
                std::slice::from_ref(address),
                name,
                clock.now(),
            )
            .await?;
        }
        Ok(())
    }

    async fn decide<P, C>(
        &self,
        active: &ActiveProfile,
        rules: &KeywordSource,
        candidates: &[Candidate],
        llm: &TriageLlm<'_, P, C>,
        report: &mut TriageReport,
    ) -> Result<Decided, CoreError>
    where
        P: LlmProvider,
        C: LlmCache,
    {
        let definition = active.revision.definition.definition();
        let system = TRIAGE_V1.system(definition);
        let tag = TRIAGE_V1.tag();
        let settings = llm.settings;
        let mut budget = RunBudget::new(settings.budget_cents);
        let mut decided = Decided {
            observations: Vec::new(),
            contacts: Vec::new(),
            budget_exhausted: false,
        };
        for candidate in candidates {
            let content = candidate_text(candidate);
            match route(&content, rules) {
                Route::Reject => report.rejected_by_rules += 1,
                Route::Accept { matched } => {
                    report.accepted_by_rules += 1;
                    decided.observations.push(Observation {
                        domain: candidate.domain.clone(),
                        kind: candidate.kind.clone(),
                        value: Some(short_title(candidate)),
                        raw: json!({
                            "title": candidate.title,
                            "matched": matched,
                            "route": "keywords",
                            "details": candidate.details,
                        }),
                        provenance: candidate.provenance.clone(),
                        strength: strength(RULES_STRENGTH)?,
                        observed_at: candidate.observed_at,
                    });
                }
                Route::Ambiguous { matched, .. } => {
                    if decided.budget_exhausted
                        || self.is_suppressed(&candidate.domain, None).await?
                    {
                        continue;
                    }
                    let Some(provider) = llm.provider else {
                        report.deferred += 1;
                        continue;
                    };
                    let documents = [UntrustedDocument {
                        source: candidate.provenance.as_str(),
                        content: &content,
                    }];
                    let scope = CacheScope {
                        profile_id: active.profile.id,
                        revision: active.revision.number,
                        prompt: tag.clone(),
                        domain: candidate.domain.clone(),
                    };
                    let prompt = Prompt {
                        system: &system,
                        instruction: TRIAGE_V1.instruction(),
                        documents: &documents,
                        max_tokens: settings.max_tokens,
                    };
                    report.sent_to_llm += 1;
                    let verdict = match call_structured_cached::<TriageVerdict, _, _>(
                        provider,
                        llm.cache,
                        &scope,
                        prompt,
                        settings.call,
                        &mut budget,
                    )
                    .await
                    {
                        Ok(verdict) => verdict,
                        Err(LlmCallError::BudgetExhausted) => {
                            report.sent_to_llm -= 1;
                            decided.budget_exhausted = true;
                            continue;
                        }
                        Err(error) => {
                            warn!(domain = %candidate.domain, %error, "triage call failed, candidate skipped");
                            continue;
                        }
                    };
                    if !verdict.value.is_supported_relevance(&documents) {
                        debug!(domain = %candidate.domain, "candidate judged not relevant");
                        continue;
                    }
                    report.accepted_by_llm += 1;
                    let evidence: Vec<serde_json::Value> = verdict
                        .value
                        .evidence
                        .iter()
                        .map(|quote| {
                            json!({ "document_index": quote.document_index, "quote": quote.quote })
                        })
                        .collect();
                    decided.observations.push(Observation {
                        domain: candidate.domain.clone(),
                        kind: candidate.kind.clone(),
                        value: Some(short_title(candidate)),
                        raw: json!({
                            "title": candidate.title,
                            "matched": matched,
                            "route": "llm",
                            "reason": verdict.value.reason,
                            "evidence": evidence,
                            "model": verdict.model,
                            "prompt": tag,
                            "from_cache": verdict.from_cache,
                            "details": candidate.details,
                        }),
                        provenance: candidate.provenance.clone(),
                        strength: strength(LLM_STRENGTH)?,
                        observed_at: candidate.observed_at,
                    });
                    if let Some(address) = verdict
                        .value
                        .contact_email
                        .as_deref()
                        .and_then(|raw| EmailAddress::parse(raw).ok())
                        && !self
                            .is_suppressed(&candidate.domain, Some(&address))
                            .await?
                    {
                        decided.contacts.push((
                            candidate.domain.clone(),
                            FoundAddress {
                                address,
                                provenance: candidate.provenance.clone(),
                                method: ContactMethod::LlmExtraction,
                                confidence: strength(CONTACT_CONFIDENCE)?,
                            },
                        ));
                    }
                }
            }
        }
        Ok(decided)
    }

    #[transactional(run)]
    async fn insert_triage_runs(&self, runs: &[Run]) -> Result<(), CoreError> {
        let mut repo = run_repository;
        for run in runs {
            repo.insert(run).await?;
        }
        Ok(())
    }

    #[transactional(run)]
    async fn finish_triage_run(
        &self,
        id: RunId,
        outcome: &RunOutcome,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut repo = run_repository;
        repo.finish(id, outcome, at).await?;
        Ok(())
    }
}
