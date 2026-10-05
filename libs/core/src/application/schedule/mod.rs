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
pub const QUALIFICATION_DEDUP_KEY: &str = "llm:qualify";

pub fn schedule_key(source: SourceName) -> String {
    match source {
        SourceName::LlmQualification => QUALIFICATION_DEDUP_KEY.to_owned(),
        other => format!("source:{}", other.as_str()),
    }
}

pub fn source_of_schedule_key(dedup_key: &str) -> Option<SourceName> {
    if dedup_key == QUALIFICATION_DEDUP_KEY {
        return Some(SourceName::LlmQualification);
    }
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
    fn qualification_uses_its_own_key() {
        assert_eq!(schedule_key(SourceName::LlmQualification), "llm:qualify");
        assert_eq!(schedule_key(SourceName::Boamp), "source:boamp");
        assert_eq!(source_of_schedule_key("source:nope"), None);
    }
}
