use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{UserId, settings::StoredSetting};

#[cfg_attr(test, mockall::automock)]
pub trait OrganisationSettingRepository: Send {
    fn find(
        &mut self,
        key: &str,
    ) -> impl Future<Output = Result<Option<StoredSetting>, CoreError>> + Send;

    fn save(
        &mut self,
        key: &str,
        value: &str,
        updated_by: UserId,
        updated_at: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;
}
