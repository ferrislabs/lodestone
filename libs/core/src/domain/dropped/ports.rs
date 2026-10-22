use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    UserId,
    dropped::{DropReview, DroppedCandidate, DroppedFilter, DroppedId, RecallRow},
    profile::ProfileId,
};

#[cfg_attr(test, mockall::automock)]
pub trait DroppedCandidateRepository: Send {
    fn record(
        &mut self,
        candidates: &[DroppedCandidate],
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn list(
        &mut self,
        profile_id: ProfileId,
        filter: &DroppedFilter,
        limit: u32,
        random: bool,
    ) -> impl Future<Output = Result<Vec<DroppedCandidate>, CoreError>> + Send;

    fn find(
        &mut self,
        id: DroppedId,
    ) -> impl Future<Output = Result<Option<DroppedCandidate>, CoreError>> + Send;

    fn review(
        &mut self,
        id: DroppedId,
        review: DropReview,
        by: Option<UserId>,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn purge(
        &mut self,
        before: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn recall(
        &mut self,
        profile_id: Option<ProfileId>,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> impl Future<Output = Result<Vec<RecallRow>, CoreError>> + Send;
}
