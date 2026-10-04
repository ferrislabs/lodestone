use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::profile::{ActiveProfile, Profile, ProfileId, ProfileRevision, RevisionNumber};

#[cfg_attr(test, mockall::automock)]
pub trait ProfileRepository: Send {
    fn insert_profile(
        &mut self,
        profile: &Profile,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn insert_revision(
        &mut self,
        revision: &ProfileRevision,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn find(
        &mut self,
        id: ProfileId,
    ) -> impl Future<Output = Result<Option<Profile>, CoreError>> + Send;

    fn lock_latest_revision_number(
        &mut self,
        id: ProfileId,
    ) -> impl Future<Output = Result<Option<RevisionNumber>, CoreError>> + Send;

    fn set_active(
        &mut self,
        id: ProfileId,
        active: bool,
        updated_at: DateTime<Utc>,
    ) -> impl Future<Output = Result<Option<Profile>, CoreError>> + Send;

    fn list(&mut self) -> impl Future<Output = Result<Vec<Profile>, CoreError>> + Send;

    fn list_revisions(
        &mut self,
        id: ProfileId,
    ) -> impl Future<Output = Result<Vec<ProfileRevision>, CoreError>> + Send;

    fn list_active(&mut self)
    -> impl Future<Output = Result<Vec<ActiveProfile>, CoreError>> + Send;
}
