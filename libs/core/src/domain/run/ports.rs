use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::run::{Run, RunId, RunOutcome};

#[cfg_attr(test, mockall::automock)]
pub trait RunRepository: Send {
    fn insert(&mut self, run: &Run) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn finish(
        &mut self,
        id: RunId,
        outcome: &RunOutcome,
        finished_at: DateTime<Utc>,
    ) -> impl Future<Output = Result<Option<Run>, CoreError>> + Send;

    fn find(&mut self, id: RunId) -> impl Future<Output = Result<Option<Run>, CoreError>> + Send;

    fn list_recent(
        &mut self,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<Run>, CoreError>> + Send;
}
