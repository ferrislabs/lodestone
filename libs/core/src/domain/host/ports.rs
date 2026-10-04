use chrono::{DateTime, Utc};
use common::CoreError;
use thiserror::Error;

use crate::domain::{
    host::{CandidateHost, Hostname},
    values::SourceName,
};

pub trait CandidateHosts: Send + Sync {
    fn upsert(
        &self,
        hosts: &[CandidateHost],
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn list_due(
        &self,
        source: SourceName,
        checked_before: DateTime<Utc>,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<CandidateHost>, CoreError>> + Send;

    fn mark_checked(
        &self,
        hosts: &[Hostname],
        source: SourceName,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DnsError {
    #[error("dns timeout")]
    Timeout,
    #[error("dns failure: {0}")]
    Failure(String),
}

pub trait DnsResolver: Send + Sync {
    fn cname_chain(
        &self,
        host: &Hostname,
    ) -> impl Future<Output = Result<Vec<Hostname>, DnsError>> + Send;
}

pub trait CtCursors: Send + Sync {
    fn next_index(
        &self,
        log_id: &str,
    ) -> impl Future<Output = Result<Option<u64>, CoreError>> + Send;

    fn advance(
        &self,
        log_id: &str,
        next_index: u64,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;
}

pub trait MxLookup: Send + Sync {
    fn has_mx(
        &self,
        domain: &crate::domain::NormalizedDomain,
    ) -> impl Future<Output = Result<bool, DnsError>> + Send;
}
