use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    NormalizedDomain,
    lead::LeadId,
    timeline::{TimelineEntry, TimelineEvent},
};

pub trait TimelineRepository: Send {
    fn record(
        &mut self,
        lead: LeadId,
        entry: &TimelineEntry,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn record_for_domain(
        &mut self,
        domain: &NormalizedDomain,
        entry: &TimelineEntry,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn record_exclusions(
        &mut self,
        reason: &str,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn list(
        &mut self,
        lead: LeadId,
    ) -> impl Future<Output = Result<Vec<TimelineEvent>, CoreError>> + Send;

    fn purge(
        &mut self,
        before: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn backfill(&mut self) -> impl Future<Output = Result<u64, CoreError>> + Send;
}
