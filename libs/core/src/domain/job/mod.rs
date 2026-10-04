use std::{num::NonZeroU16, time::Duration};

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::lead::{str_enum, uuid_id};

pub mod ports;

uuid_id!(JobId);
uuid_id!(LeaseToken);

#[derive(Debug, Error, PartialEq, Eq)]
#[error("job kind must be lowercase dotted or snake_case, 1 to 64 characters")]
pub struct InvalidJobKind;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JobKind(Box<str>);

impl JobKind {
    pub fn parse(raw: &str) -> Result<Self, InvalidJobKind> {
        let valid = !raw.is_empty()
            && raw.len() <= 64
            && raw.starts_with(|c: char| c.is_ascii_lowercase())
            && raw
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.');
        if valid {
            Ok(Self(raw.into()))
        } else {
            Err(InvalidJobKind)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
}

str_enum!(JobStatus {
    Pending => "pending",
    Running => "running",
    Succeeded => "succeeded",
    Failed => "failed",
});

#[derive(Debug, Clone, PartialEq)]
pub struct NewJob {
    pub kind: JobKind,
    pub payload: serde_json::Value,
    pub run_at: DateTime<Utc>,
    pub max_attempts: NonZeroU16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: JobId,
    pub kind: JobKind,
    pub payload: serde_json::Value,
    pub status: JobStatus,
    pub attempts: u16,
    pub max_attempts: NonZeroU16,
    pub run_at: DateTime<Utc>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedJob {
    pub job: Job,
    pub token: LeaseToken,
    pub lease_until: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub base_delay: Duration,
    pub max_delay: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureDecision {
    RetryAt(DateTime<Utc>),
    GiveUp,
}

impl RetryPolicy {
    pub const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(3600);

    pub fn delay_after(&self, attempt: u16) -> Duration {
        let exponent = u32::from(attempt.saturating_sub(1)).min(31);
        let factor = 1u32 << exponent;
        self.base_delay
            .checked_mul(factor)
            .unwrap_or(self.max_delay)
            .min(self.max_delay)
    }

    pub fn decide(&self, job: &Job, now: DateTime<Utc>) -> FailureDecision {
        if job.attempts >= job.max_attempts.get() {
            return FailureDecision::GiveUp;
        }
        let delay = chrono::Duration::from_std(self.delay_after(job.attempts))
            .unwrap_or(chrono::Duration::MAX);
        FailureDecision::RetryAt(
            now.checked_add_signed(delay)
                .unwrap_or(DateTime::<Utc>::MAX_UTC),
        )
    }
}

pub fn new_lease_token() -> LeaseToken {
    LeaseToken(Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(attempts: u16, max_attempts: u16) -> Job {
        let now = Utc::now();
        Job {
            id: JobId(Uuid::nil()),
            kind: JobKind::parse("probe.http").unwrap(),
            payload: serde_json::Value::Null,
            status: JobStatus::Running,
            attempts,
            max_attempts: NonZeroU16::new(max_attempts).unwrap(),
            run_at: now,
            last_error: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn policy() -> RetryPolicy {
        RetryPolicy {
            base_delay: Duration::from_secs(10),
            max_delay: Duration::from_secs(60),
        }
    }

    #[test]
    fn delay_doubles_and_is_capped() {
        let policy = policy();
        assert_eq!(policy.delay_after(1), Duration::from_secs(10));
        assert_eq!(policy.delay_after(2), Duration::from_secs(20));
        assert_eq!(policy.delay_after(3), Duration::from_secs(40));
        assert_eq!(policy.delay_after(4), Duration::from_secs(60));
        assert_eq!(policy.delay_after(u16::MAX), Duration::from_secs(60));
    }

    #[test]
    fn last_attempt_gives_up() {
        let now = Utc::now();
        assert_eq!(policy().decide(&job(3, 3), now), FailureDecision::GiveUp);
        assert_eq!(
            policy().decide(&job(1, 3), now),
            FailureDecision::RetryAt(now + chrono::Duration::seconds(10))
        );
    }

    #[test]
    fn job_kind_format() {
        assert!(JobKind::parse("probe.http").is_ok());
        assert!(JobKind::parse("source_run").is_ok());
        assert_eq!(JobKind::parse("Probe"), Err(InvalidJobKind));
        assert_eq!(JobKind::parse(""), Err(InvalidJobKind));
        assert_eq!(JobKind::parse("a b"), Err(InvalidJobKind));
    }
}
