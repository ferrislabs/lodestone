use std::time::Duration;

use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::fetch::{FetchError, FetchRequest, FetchResponse};

pub trait Fetcher: Send + Sync {
    fn fetch(
        &self,
        request: &FetchRequest,
    ) -> impl Future<Output = Result<FetchResponse, FetchError>> + Send;
}

#[cfg_attr(test, mockall::automock)]
pub trait HostVisitRepository: Send {
    fn reserve_probe(
        &mut self,
        host: &str,
        now: DateTime<Utc>,
        revisit_interval: Duration,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;
}
