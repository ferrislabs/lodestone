use common::CoreError;

use crate::domain::quality::{DecisionLabel, ExclusionCount, LabelFilter, VerdictCounts};

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
}
