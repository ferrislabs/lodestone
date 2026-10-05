use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::NormalizedDomain;

pub trait AtsTargets: Send + Sync {
    fn list_due(
        &self,
        checked_before: DateTime<Utc>,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<NormalizedDomain>, CoreError>> + Send;

    fn mark_checked(
        &self,
        domains: &[NormalizedDomain],
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;
}
