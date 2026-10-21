use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::transactional;
use serde_json::json;

use crate::{
    application::{LodestoneUseCase, sources::SourceRunError},
    domain::{
        job::ports::Clock,
        profile::{
            ActiveProfile,
            definition::{Agent, KeywordSource},
        },
        run::{Run, RunId, RunOutcome, ports::RunRepository},
        source::{Observation, SourceError},
        triage::{Candidate, Route, route},
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

fn observation(
    candidate: &Candidate,
    agent: &Agent,
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
        agent: Some(agent.id.clone()),
    })
}

fn decide(
    agent: &Agent,
    rules: &KeywordSource,
    candidates: &[Candidate],
    report: &mut TriageReport,
) -> Result<Vec<Observation>, CoreError> {
    let mut observations = Vec::new();
    for candidate in candidates {
        match route(&candidate_text(candidate), rules) {
            Route::Reject => report.rejected_by_rules += 1,
            Route::Accept { matched } => {
                report.accepted_by_rules += 1;
                observations.push(observation(
                    candidate,
                    agent,
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
                    agent,
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
                active
                    .revision
                    .definition
                    .definition()
                    .enabled_keyword_rules(name)
                    .next()
                    .is_some()
            })
            .collect();
        let targets: Vec<(&ActiveProfile, &Agent, &KeywordSource)> = profiles
            .iter()
            .flat_map(|active| {
                active
                    .revision
                    .definition
                    .definition()
                    .enabled_agents(name)
                    .filter_map(move |agent| {
                        agent
                            .source
                            .keyword_rules()
                            .map(|rules| (active, agent, rules))
                    })
            })
            .collect();
        if targets.is_empty() {
            return Ok(TriageReport::default());
        }

        let started_at = clock.now();
        let runs: Vec<Run> = targets
            .iter()
            .map(|(active, agent, _)| {
                Run::start(
                    RunId(generate_uuid_v7()),
                    active.profile.id,
                    name,
                    Some(agent.id.clone()),
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
        for ((active, agent, rules), run) in targets.iter().zip(&runs) {
            let observations = match decide(agent, rules, &candidates, &mut report) {
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
            let signals = count(observations.len());
            let recorded = self
                .record_observations_at(active, name, &observations, clock.now())
                .await;
            let outcome = match &recorded {
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
