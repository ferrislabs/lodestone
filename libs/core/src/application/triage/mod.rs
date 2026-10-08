use std::borrow::Cow;

use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::transactional;
use serde_json::json;

use crate::{
    application::{
        LodestoneUseCase,
        sources::{LeadCreation, SourceRunError},
    },
    domain::{
        job::ports::Clock,
        profile::{
            ActiveProfile,
            definition::{AgentId, ProfileDefinition, SearchAgent},
        },
        run::{Run, RunId, RunOutcome, ports::RunRepository},
        source::{Observation, SourceError},
        triage::{Candidate, Route, passes_filters, route},
        values::{SourceName, UnitInterval},
    },
};

const TITLE_MAX_CHARS: usize = 200;
const RULES_STRENGTH: f64 = 0.7;
const AMBIGUOUS_STRENGTH: f64 = 0.5;

pub trait CandidateSource: Send + Sync {
    fn name(&self) -> SourceName;

    fn collect(
        &self,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<Candidate>, SourceError>> + Send;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TriageReport {
    pub candidates: u64,
    pub accepted_by_rules: u64,
    pub accepted_ambiguous: u64,
    pub rejected_by_rules: u64,
    pub rejected_by_filters: u64,
    pub skipped_no_lead: u64,
}

impl TriageReport {
    pub fn accepted(&self) -> u64 {
        self.accepted_by_rules + self.accepted_ambiguous
    }
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

struct KeywordTarget<'a> {
    agent: Option<&'a AgentId>,
    rules: Cow<'a, SearchAgent>,
}

fn keyword_targets(definition: &ProfileDefinition, source: SourceName) -> Vec<KeywordTarget<'_>> {
    let mut targets: Vec<KeywordTarget<'_>> = definition
        .enabled_search_agents(source)
        .map(|agent| KeywordTarget {
            agent: Some(&agent.id),
            rules: Cow::Borrowed(agent),
        })
        .collect();
    if source == SourceName::Ats
        && let Some(rules) = definition.company_jobs_rules()
    {
        targets.push(KeywordTarget {
            agent: None,
            rules: Cow::Owned(rules),
        });
    }
    targets
}

fn observation(
    candidate: &Candidate,
    agent: Option<&AgentId>,
    matched: &[String],
    excluded: Option<&[String]>,
    route: &str,
    strength: f64,
) -> Result<Observation, CoreError> {
    let mut raw = json!({
        "title": candidate.title,
        "matched": matched,
        "route": route,
        "details": candidate.details,
    });
    if let Some(excluded) = excluded {
        raw["excluded"] = json!(excluded);
    }
    Ok(Observation {
        domain: candidate.domain.clone(),
        kind: candidate.kind.clone(),
        value: Some(short_title(candidate)),
        raw,
        provenance: candidate.provenance.clone(),
        strength: self::strength(strength)?,
        observed_at: candidate.observed_at,
        agent: agent.cloned(),
    })
}

fn decide(
    target: &KeywordTarget<'_>,
    candidates: &[Candidate],
    report: &mut TriageReport,
) -> Result<Vec<Observation>, CoreError> {
    let agent = target.rules.as_ref();
    let mut observations = Vec::new();
    for candidate in candidates {
        if !passes_filters(&agent.filters, candidate) {
            report.rejected_by_filters += 1;
            continue;
        }
        match route(&candidate_text(candidate), agent) {
            Route::Reject => report.rejected_by_rules += 1,
            Route::Accept { matched } => {
                report.accepted_by_rules += 1;
                observations.push(observation(
                    candidate,
                    target.agent,
                    &matched,
                    None,
                    "keywords",
                    RULES_STRENGTH,
                )?);
            }
            Route::Ambiguous { matched, excluded } => {
                report.accepted_ambiguous += 1;
                observations.push(observation(
                    candidate,
                    target.agent,
                    &matched,
                    Some(&excluded),
                    "keywords_ambiguous",
                    AMBIGUOUS_STRENGTH,
                )?);
            }
        }
    }
    Ok(observations)
}

impl LodestoneUseCase {
    pub async fn run_triage<S, K>(
        &self,
        source: &S,
        clock: &K,
    ) -> Result<TriageReport, SourceRunError>
    where
        S: CandidateSource,
        K: Clock,
    {
        let name = source.name();
        let profiles: Vec<ActiveProfile> = self
            .list_active_profiles()
            .await?
            .into_iter()
            .filter(|active| {
                !keyword_targets(active.revision.definition.definition(), name).is_empty()
            })
            .collect();
        let targets: Vec<(&ActiveProfile, KeywordTarget<'_>)> = profiles
            .iter()
            .flat_map(|active| {
                keyword_targets(active.revision.definition.definition(), name)
                    .into_iter()
                    .map(move |target| (active, target))
            })
            .collect();
        if targets.is_empty() {
            return Ok(TriageReport::default());
        }

        let started_at = clock.now();
        let runs: Vec<Run> = targets
            .iter()
            .map(|(active, target)| {
                Run::start(
                    RunId(generate_uuid_v7()),
                    active.profile.id,
                    name,
                    target.agent.cloned(),
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
        let mut first_error: Option<CoreError> = None;
        for ((active, target), run) in targets.iter().zip(&runs) {
            let observations = match decide(target, &candidates, &mut report) {
                Ok(observations) => observations,
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
            let creation = match target.agent {
                Some(_) => LeadCreation::Create,
                None => LeadCreation::ExistingOnly,
            };
            let recorded = self
                .record_observations_with(active, name, &observations, clock.now(), creation)
                .await;
            let skipped = recorded.as_ref().map_or(0, |skipped| *skipped);
            report.skipped_no_lead += skipped;
            let signals = count(observations.len()).saturating_sub(skipped);
            let outcome = match &recorded {
                Ok(_) => RunOutcome::Succeeded { volume, signals },
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
        }
        match first_error {
            Some(error) => Err(error.into()),
            None => Ok(report),
        }
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
