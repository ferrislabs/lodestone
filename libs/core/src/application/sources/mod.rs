use std::num::NonZeroU16;

use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::transactional;
use serde_json::json;

use crate::{
    application::{LodestoneUseCase, jobs::JobError},
    domain::{
        job::{Job, JobKind, NewJob, ports::Clock},
        lead::{
            Signal, SignalId,
            ports::{LeadRepository, SignalRepository},
        },
        profile::{ActiveProfile, ProfileId},
        run::{Run, RunId, RunOutcome, ports::RunRepository},
        source::{Observation, Source, SourceError},
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
        let name = source.name();
        let profiles: Vec<ActiveProfile> = self
            .list_active_profiles()
            .await?
            .into_iter()
            .filter(|active| active.revision.definition.definition().enables(name))
            .collect();
        if profiles.is_empty() {
            return Ok(SourceRunReport {
                runs: Vec::new(),
                items: 0,
                signals: 0,
            });
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
        self.insert_runs(&runs).await?;

        let items = match source.collect(&profiles, started_at).await {
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

        let volume = count(items.len());
        let mut total_signals = 0;
        for (active, run) in profiles.iter().zip(&runs) {
            let observations: Vec<Observation> = items
                .iter()
                .flat_map(|item| source.evaluate(item, active))
                .collect();
            let signals = count(observations.len());
            let recorded = self
                .record_observations(active.profile.id, name, &observations)
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
            items: volume,
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

    #[transactional(lead, signal)]
    pub async fn record_observations(
        &self,
        profile_id: ProfileId,
        source: SourceName,
        observations: &[Observation],
    ) -> Result<(), CoreError> {
        let mut leads = lead_repository;
        let mut signals = signal_repository;
        for observation in observations {
            let lead = leads
                .record_seen(profile_id, &observation.domain, observation.observed_at)
                .await?;
            signals
                .insert(&Signal {
                    id: SignalId(generate_uuid_v7()),
                    lead_id: lead.id,
                    kind: observation.kind.clone(),
                    value: observation.value.clone(),
                    source,
                    raw: observation.raw.clone(),
                    provenance: observation.provenance.clone(),
                    strength: observation.strength,
                    observed_at: observation.observed_at,
                })
                .await?;
        }
        Ok(())
    }
}
