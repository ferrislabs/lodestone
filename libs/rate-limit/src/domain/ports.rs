use std::time::Instant;

use crate::{Quota, RateLimitDecision, RateLimitError, RateLimitKey};

#[cfg_attr(test, mockall::automock)]
pub trait RateLimiter: Send + Sync {
    fn check(
        &self,
        key: &RateLimitKey,
        quota: Quota,
    ) -> impl Future<Output = Result<RateLimitDecision, RateLimitError>> + Send;
}

pub trait HasRateLimiter {
    type Limiter: RateLimiter;

    fn rate_limiter(&self) -> &Self::Limiter;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}
