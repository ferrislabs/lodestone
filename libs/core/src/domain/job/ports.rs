use std::time::Duration;

use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::job::{ClaimedJob, Job, JobId, LeaseToken, NewJob};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobFailure {
    RetryAt { at: DateTime<Utc>, error: String },
    GiveUp { error: String },
}

pub trait JobQueue: Send + Sync {
    fn enqueue(
        &self,
        job: &NewJob,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<JobId, CoreError>> + Send;

    fn claim(
        &self,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> impl Future<Output = Result<Option<ClaimedJob>, CoreError>> + Send;

    fn extend_lease(
        &self,
        id: JobId,
        token: LeaseToken,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn complete(
        &self,
        id: JobId,
        token: LeaseToken,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn fail(
        &self,
        id: JobId,
        token: LeaseToken,
        failure: &JobFailure,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn release(
        &self,
        id: JobId,
        token: LeaseToken,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn find(&self, id: JobId) -> impl Future<Output = Result<Option<Job>, CoreError>> + Send;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}
