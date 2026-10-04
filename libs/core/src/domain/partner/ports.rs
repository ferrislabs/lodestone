use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    partner::{Partner, PartnerAward},
    profile::ProfileId,
};

#[cfg_attr(test, mockall::automock)]
pub trait PartnerRepository: Send {
    fn record_award(
        &mut self,
        profile_id: ProfileId,
        award: &PartnerAward,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn list_for_profile(
        &mut self,
        profile_id: ProfileId,
    ) -> impl Future<Output = Result<Vec<Partner>, CoreError>> + Send;
}
