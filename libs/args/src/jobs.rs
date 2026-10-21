use std::{num::NonZeroU16, time::Duration};

use common::{JobsConfig, SettingsError};

#[derive(clap::Args, Debug, Clone)]
pub struct JobsArgs {
    #[arg(
        long = "jobs-workers",
        env = "JOBS_WORKERS",
        name = "JOBS_WORKERS",
        default_value = "4",
        long_help = "Number of concurrent job workers"
    )]
    pub workers: NonZeroU16,

    #[arg(
        long = "jobs-lease-seconds",
        env = "JOBS_LEASE_SECONDS",
        name = "JOBS_LEASE_SECONDS",
        default_value_t = 300,
        long_help = "Seconds a worker holds a job before another worker may reclaim it"
    )]
    pub lease_seconds: u64,

    #[arg(
        long = "jobs-poll-interval-ms",
        env = "JOBS_POLL_INTERVAL_MS",
        name = "JOBS_POLL_INTERVAL_MS",
        default_value_t = 1000,
        long_help = "Milliseconds an idle worker waits before polling the queue again"
    )]
    pub poll_interval_ms: u64,

    #[arg(
        long = "jobs-max-attempts",
        env = "JOBS_MAX_ATTEMPTS",
        name = "JOBS_MAX_ATTEMPTS",
        default_value = "5",
        long_help = "Attempts before a failing job is marked as failed for good"
    )]
    pub max_attempts: NonZeroU16,

    #[arg(
        long = "jobs-retry-base-seconds",
        env = "JOBS_RETRY_BASE_SECONDS",
        name = "JOBS_RETRY_BASE_SECONDS",
        default_value_t = 10,
        long_help = "Base delay of the exponential backoff between attempts"
    )]
    pub retry_base_seconds: u64,

    #[arg(
        long = "jobs-shutdown-grace-seconds",
        env = "JOBS_SHUTDOWN_GRACE_SECONDS",
        name = "JOBS_SHUTDOWN_GRACE_SECONDS",
        default_value_t = 20,
        long_help = "Seconds running jobs get to finish on shutdown before their lease is released"
    )]
    pub shutdown_grace_seconds: u64,

    #[arg(
        long = "jobs-source-interval-minutes",
        env = "JOBS_SOURCE_INTERVAL_MINUTES",
        name = "JOBS_SOURCE_INTERVAL_MINUTES",
        default_value_t = 60,
        long_help = "Minutes between two runs of the same source"
    )]
    pub source_interval_minutes: u64,

    #[arg(
        long = "lead-events-retention-days",
        env = "LEAD_EVENTS_RETENTION_DAYS",
        name = "LEAD_EVENTS_RETENTION_DAYS",
        default_value = "90",
        long_help = "Days the processing events of a lead are kept on its timeline; a daily job removes older ones"
    )]
    pub lead_events_retention_days: NonZeroU16,
}

impl TryFrom<JobsArgs> for JobsConfig {
    type Error = SettingsError;

    fn try_from(value: JobsArgs) -> Result<Self, Self::Error> {
        JobsConfig {
            workers: value.workers,
            lease: Duration::from_secs(value.lease_seconds),
            poll_interval: Duration::from_millis(value.poll_interval_ms),
            max_attempts: value.max_attempts,
            retry_base_delay: Duration::from_secs(value.retry_base_seconds),
            shutdown_grace: Duration::from_secs(value.shutdown_grace_seconds),
            source_interval: Duration::from_secs(value.source_interval_minutes.saturating_mul(60)),
        }
        .validated()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        jobs: JobsArgs,
    }

    #[test]
    fn defaults_build_a_valid_config() {
        let cmd = Cmd::try_parse_from(["cmd"]).unwrap();
        let config = JobsConfig::try_from(cmd.jobs).unwrap();
        assert_eq!(config.workers.get(), 4);
        assert_eq!(config.lease, Duration::from_secs(300));
        assert_eq!(config.poll_interval, Duration::from_millis(1000));
        assert_eq!(config.max_attempts.get(), 5);
        assert_eq!(cmd_retention(), 90);
    }

    fn cmd_retention() -> u16 {
        Cmd::try_parse_from(["cmd"])
            .unwrap()
            .jobs
            .lead_events_retention_days
            .get()
    }

    #[test]
    fn the_lead_events_retention_is_configurable_and_never_zero() {
        let cmd = Cmd::try_parse_from(["cmd", "--lead-events-retention-days", "30"]).unwrap();
        assert_eq!(cmd.jobs.lead_events_retention_days.get(), 30);
        assert!(Cmd::try_parse_from(["cmd", "--lead-events-retention-days", "0"]).is_err());
    }

    #[test]
    fn zero_workers_is_rejected_at_parse_time() {
        assert!(Cmd::try_parse_from(["cmd", "--jobs-workers", "0"]).is_err());
    }

    #[test]
    fn lease_shorter_than_poll_is_rejected() {
        let cmd = Cmd::try_parse_from([
            "cmd",
            "--jobs-lease-seconds",
            "1",
            "--jobs-poll-interval-ms",
            "5000",
        ])
        .unwrap();
        assert_eq!(
            JobsConfig::try_from(cmd.jobs).unwrap_err(),
            SettingsError::LeaseNotLongerThanPoll
        );
    }
}
