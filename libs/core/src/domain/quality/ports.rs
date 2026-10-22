use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    profile::ProfileId,
    quality::{DecisionLabel, ExclusionCount, LabelFilter, SourcingRow, VerdictCounts},
};

#[cfg_attr(test, mockall::automock)]
pub trait DecisionRepository: Send {
    fn record(
        &mut self,
        label: &DecisionLabel,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn list(
        &mut self,
        filter: &LabelFilter,
    ) -> impl Future<Output = Result<Vec<DecisionLabel>, CoreError>> + Send;

    fn verdict_counts(
        &mut self,
        filter: &LabelFilter,
    ) -> impl Future<Output = Result<VerdictCounts, CoreError>> + Send;

    fn exclusion_counts(
        &mut self,
        filter: &LabelFilter,
    ) -> impl Future<Output = Result<Vec<ExclusionCount>, CoreError>> + Send;

    fn sourcing_rows(
        &mut self,
        profile_id: Option<ProfileId>,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> impl Future<Output = Result<Vec<SourcingRow>, CoreError>> + Send;
}
