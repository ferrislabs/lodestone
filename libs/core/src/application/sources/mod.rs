use std::num::NonZeroU16;

use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::transactional;
use serde_json::json;

use crate::{
    application::{LodestoneUseCase, jobs::JobError},
    domain::{
        NormalizedDomain,
        facts::independent_signals,
        job::{Job, JobKind, NewJob, ports::Clock},
        lead::{
            LeadId, Signal, SignalId,
            ports::{LeadRepository, SignalRepository},
        },
        profile::{
            ActiveProfile,
            definition::{Agent, EvidenceSettings},
        },
        run::{Run, RunId, RunOutcome, ports::RunRepository},
        scoring::{signal_strength, strength_score},
        source::{Observation, Source, SourceError},
        suppression::ports::SuppressionRepository,
        values::SourceName,
    },
};

pub const SOURCE_JOB_KIND: &str = "source.run";
const SOURCE_JOB_MAX_ATTEMPTS: NonZeroU16 = NonZeroU16::new(3).unwrap();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRunReport {
    pub runs: Vec<RunId>,
    pub items: u64,
    pub signals: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum SourceRunError {
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Core(#[from] CoreError),
}

impl From<SourceRunError> for JobError {
    fn from(error: SourceRunError) -> Self {
        JobError::Retryable(error.to_string())
    }
}

pub fn source_job(source: SourceName, run_at: DateTime<Utc>) -> Result<NewJob, CoreError> {
    let kind = JobKind::parse(SOURCE_JOB_KIND)
        .map_err(|_| CoreError::Internal("invalid source job kind".to_owned()))?;
    Ok(NewJob {
        kind,
        payload: json!({ "source": source.as_str() }),
        run_at,
        max_attempts: SOURCE_JOB_MAX_ATTEMPTS,
        dedup_key: Some(format!("source:{}", source.as_str())),
    })
}

pub fn source_of_job(job: &Job) -> Result<SourceName, JobError> {
    job.payload
        .get("source")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Permanent("source job without a source".to_owned()))?
        .parse()
        .map_err(|error: crate::domain::values::ValueError| JobError::Permanent(error.to_string()))
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunRecording {
    Always,
    WhenProcessed,
}

impl LodestoneUseCase {
    pub async fn run_source<S, C>(
        &self,
        source: &S,
        clock: &C,
    ) -> Result<SourceRunReport, SourceRunError>
    where
        S: Source,
        C: Clock,
    {
        self.run_source_recording(source, clock, RunRecording::Always)
            .await
    }

    pub async fn listen_source<S, C>(
        &self,
        source: &S,
        clock: &C,
    ) -> Result<SourceRunReport, SourceRunError>
    where
        S: Source,
        C: Clock,
    {
        self.run_source_recording(source, clock, RunRecording::WhenProcessed)
            .await
    }

    async fn run_source_recording<S, C>(
        &self,
        source: &S,
        clock: &C,
        recording: RunRecording,
    ) -> Result<SourceRunReport, SourceRunError>
    where
        S: Source,
        C: Clock,
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
                    .enabled_agents(name)
                    .next()
                    .is_some()
            })
            .collect();
        let targets: Vec<(&ActiveProfile, &Agent)> = profiles
            .iter()
            .flat_map(|active| {
                active
                    .revision
                    .definition
                    .definition()
                    .enabled_agents(name)
                    .map(move |agent| (active, agent))
            })
            .collect();
        if targets.is_empty() {
            return Ok(SourceRunReport {
                runs: Vec::new(),
                items: 0,
                signals: 0,
            });
        }

        let started_at = clock.now();
        let runs: Vec<Run> = targets
            .iter()
            .map(|(active, agent)| {
                Run::start(
                    RunId(generate_uuid_v7()),
                    active.profile.id,
                    name,
                    Some(agent.id().clone()),
                    started_at,
                )
            })
            .collect();
        if recording == RunRecording::Always {
            self.insert_runs(&runs).await?;
        }

        let collected = source.collect(&profiles, started_at).await;
        if recording == RunRecording::WhenProcessed
            && !matches!(&collected, Ok(items) if items.is_empty())
        {
            self.insert_runs(&runs).await?;
        }
        let items = match collected {
            Ok(items) => items,
            Err(error) => {
                let outcome = RunOutcome::Failed {
                    volume: 0,
                    signals: 0,
                    error: error.to_string(),
                };
                for run in &runs {
                    self.finish_run(run.id, &outcome, clock.now()).await?;
                }
                return Err(error.into());
            }
        };

        if items.is_empty() && recording == RunRecording::WhenProcessed {
            return Ok(SourceRunReport {
                runs: Vec::new(),
                items: 0,
                signals: 0,
            });
        }

        let mut total_signals = 0;
        for ((active, agent), run) in targets.iter().zip(&runs) {
            let served: Vec<&S::Item> = items
                .iter()
                .filter(|item| source.serves(item, agent))
                .collect();
            let volume = count(served.len());
            let observations: Vec<Observation> = served
                .iter()
                .flat_map(|item| source.evaluate(item, active, agent))
                .map(|observation| Observation {
                    agent: Some(agent.id().clone()),
                    ..observation
                })
                .collect();
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
            self.finish_run(run.id, &outcome, clock.now()).await?;
            recorded?;
            total_signals += signals;
        }

        Ok(SourceRunReport {
            runs: runs.iter().map(|run| run.id).collect(),
            items: count(items.len()),
            signals: total_signals,
        })
    }

    #[transactional(run)]
    async fn insert_runs(&self, runs: &[Run]) -> Result<(), CoreError> {
        let mut repo = run_repository;
        for run in runs {
            repo.insert(run).await?;
        }
        Ok(())
    }

    #[transactional(run)]
    async fn finish_run(
        &self,
        id: RunId,
        outcome: &RunOutcome,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut repo = run_repository;
        repo.finish(id, outcome, at).await?;
        Ok(())
    }

    pub async fn record_observations(
        &self,
        active: &ActiveProfile,
        source: SourceName,
        observations: &[Observation],
    ) -> Result<(), CoreError> {
        self.record_observations_at(active, source, observations, Utc::now())
            .await
    }

    #[transactional(lead, signal, suppression)]
    pub async fn record_observations_at(
        &self,
        active: &ActiveProfile,
        source: SourceName,
        observations: &[Observation],
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut leads = lead_repository;
        let mut signals = signal_repository;
        let mut suppressions = suppression_repository;
        let mut touched = Vec::new();
        let mut suppressed: Vec<(NormalizedDomain, bool)> = Vec::new();
        for observation in observations {
            let blocked = match suppressed
                .iter()
                .find(|(domain, _)| *domain == observation.domain)
            {
                Some((_, blocked)) => *blocked,
                None => {
                    let blocked = suppressions
                        .is_domain_suppressed(&observation.domain)
                        .await?;
                    suppressed.push((observation.domain.clone(), blocked));
                    blocked
                }
            };
            if blocked {
                continue;
            }
            let lead = leads
                .record_seen(
                    active.profile.id,
                    &observation.domain,
                    observation.observed_at,
                )
                .await?;
            if !touched.contains(&lead.id) {
                touched.push(lead.id);
            }
            signals
                .record(&Signal {
                    id: SignalId(generate_uuid_v7()),
                    lead_id: lead.id,
                    kind: observation.kind.clone(),
                    value: observation.value.clone(),
                    source,
                    raw: observation.raw.clone(),
                    provenance: observation.provenance.clone(),
                    strength: observation.strength,
                    observed_at: observation.observed_at,
                    agent: observation.agent.clone(),
                })
                .await?;
        }
        let evidence = &active.revision.definition.definition().evidence;
        for lead_id in touched {
            rescore(&mut leads, &mut signals, lead_id, evidence, now).await?;
        }
        Ok(())
    }
}

pub(crate) async fn rescore<L, S>(
    leads: &mut L,
    signals: &mut S,
    lead_id: LeadId,
    evidence: &EvidenceSettings,
    at: DateTime<Utc>,
) -> Result<(), CoreError>
where
    L: LeadRepository,
    S: SignalRepository,
{
    let current = signals.list_for_lead(lead_id).await?;
    let strength = signal_strength(&independent_signals(current.iter()), evidence, at);
    leads
        .update_score(lead_id, strength_score(strength), at)
        .await?;
    Ok(())
}
