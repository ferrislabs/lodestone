use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::job::{JobId, NewJob};

#[cfg_attr(test, mockall::automock)]
pub trait JobOutbox: Send {
    fn enqueue(
        &mut self,
        job: &NewJob,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Option<JobId>, CoreError>> + Send;
}
