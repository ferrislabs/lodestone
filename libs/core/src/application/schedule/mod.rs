use chrono::Utc;
use common::CoreError;

use crate::{
    application::LodestoneUseCase,
    domain::{
        job::{
            JobStatus,
            schedule::{JobSchedule, ScheduledJob, SourceSchedule},
        },
        values::SourceName,
    },
    infrastructure::job::postgres::PgJobSchedule,
};

pub const LATEST_RUNS_PER_SOURCE: u32 = 5;

pub fn schedule_key(source: SourceName) -> String {
    format!("source:{}", source.as_str())
}

pub fn source_of_schedule_key(dedup_key: &str) -> Option<SourceName> {
    dedup_key
        .strip_prefix("source:")
        .and_then(|name| name.parse().ok())
}

impl LodestoneUseCase {
    pub async fn list_source_schedule(&self) -> Result<SourceSchedule, CoreError> {
        let schedule = PgJobSchedule::new(self.pool.clone());
        let jobs = schedule.list().await?;
        let runs = schedule.latest_runs(LATEST_RUNS_PER_SOURCE).await?;
        Ok(SourceSchedule { jobs, runs })
    }

    pub async fn run_job_now(&self, dedup_key: &str) -> Result<ScheduledJob, CoreError> {
        let schedule = PgJobSchedule::new(self.pool.clone());
        if let Some(job) = schedule.run_now(dedup_key, Utc::now()).await? {
            return Ok(job);
        }
        match schedule.find(dedup_key).await? {
            None => Err(CoreError::NotFound),
            Some(job) if job.status == JobStatus::Running => {
                Err(CoreError::Conflict("source is already running".to_owned()))
            }
            Some(_) => Err(CoreError::Conflict("source is not scheduled".to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_keys_round_trip_for_every_source() {
        for source in SourceName::ALL {
            assert_eq!(source_of_schedule_key(&schedule_key(source)), Some(source));
        }
    }

    #[test]
    fn keys_are_prefixed_and_unknown_sources_are_refused() {
        assert_eq!(schedule_key(SourceName::Boamp), "source:boamp");
        assert_eq!(source_of_schedule_key("source:nope"), None);
        assert_eq!(source_of_schedule_key("llm:qualify"), None);
    }
}
