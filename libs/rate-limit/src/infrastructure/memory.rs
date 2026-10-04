use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{Clock, Quota, RateLimitDecision, RateLimitError, RateLimitKey, RateLimiter};

const SWEEP_THRESHOLD: usize = 10_000;

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

#[derive(Default)]
struct State {
    entries: HashMap<String, VecDeque<Instant>>,
    max_window: Duration,
}

fn trim(hits: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while let Some(oldest) = hits.front() {
        if now.saturating_duration_since(*oldest) >= window {
            hits.pop_front();
        } else {
            break;
        }
    }
}

impl State {
    fn sweep(&mut self, now: Instant) {
        let window = self.max_window;
        self.entries.retain(|_, hits| {
            trim(hits, now, window);
            !hits.is_empty()
        });
    }
}

pub struct InMemoryRateLimiter<C: Clock = SystemClock> {
    state: Arc<Mutex<State>>,
    clock: Arc<C>,
}

impl<C: Clock> Clone for InMemoryRateLimiter<C> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
            clock: Arc::clone(&self.clock),
        }
    }
}

impl InMemoryRateLimiter<SystemClock> {
    pub fn new() -> Self {
        Self::with_clock(SystemClock)
    }
}

impl Default for InMemoryRateLimiter<SystemClock> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Clock> InMemoryRateLimiter<C> {
    pub fn with_clock(clock: C) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            clock: Arc::new(clock),
        }
    }

    fn evaluate(&self, key: String, quota: Quota) -> Result<RateLimitDecision, RateLimitError> {
        let now = self.clock.now();
        let mut state = self.state.lock().map_err(|e| RateLimitError::Internal {
            message: e.to_string(),
        })?;
        state.max_window = state.max_window.max(quota.window);

        let hits = state.entries.entry(key.clone()).or_default();
        trim(hits, now, quota.window);
        let count = hits.len() as u64;

        if count < u64::from(quota.limit) {
            hits.push_back(now);
            let remaining = (u64::from(quota.limit) - count - 1) as u32;
            if state.entries.len() > SWEEP_THRESHOLD {
                state.sweep(now);
            }
            return Ok(RateLimitDecision {
                allowed: true,
                limit: quota.limit,
                remaining,
                retry_after: None,
            });
        }

        let retry_after = hits
            .front()
            .map(|oldest| (*oldest + quota.window).saturating_duration_since(now))
            .filter(|d| !d.is_zero());
        if hits.is_empty() {
            state.entries.remove(&key);
        }
        if state.entries.len() > SWEEP_THRESHOLD {
            state.sweep(now);
        }
        Ok(RateLimitDecision {
            allowed: false,
            limit: quota.limit,
            remaining: 0,
            retry_after,
        })
    }
}

impl<C: Clock> RateLimiter for InMemoryRateLimiter<C> {
    async fn check(
        &self,
        key: &RateLimitKey,
        quota: Quota,
    ) -> Result<RateLimitDecision, RateLimitError> {
        self.evaluate(key.to_string(), quota)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    #[derive(Clone)]
    struct FakeClock {
        base: Instant,
        offset_ms: Arc<AtomicU64>,
    }

    impl FakeClock {
        fn new() -> Self {
            Self {
                base: Instant::now(),
                offset_ms: Arc::new(AtomicU64::new(0)),
            }
        }

        fn advance(&self, d: Duration) {
            self.offset_ms
                .fetch_add(d.as_millis() as u64, Ordering::SeqCst);
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Instant {
            self.base + Duration::from_millis(self.offset_ms.load(Ordering::SeqCst))
        }
    }

    fn setup() -> (InMemoryRateLimiter<FakeClock>, FakeClock) {
        let clock = FakeClock::new();
        (InMemoryRateLimiter::with_clock(clock.clone()), clock)
    }

    fn quota(limit: u32) -> Quota {
        Quota {
            limit,
            window: Duration::from_secs(60),
        }
    }

    fn entries(limiter: &InMemoryRateLimiter<FakeClock>) -> usize {
        limiter.state.lock().unwrap().entries.len()
    }

    #[tokio::test]
    async fn admits_up_to_limit_with_decreasing_remaining() {
        let (limiter, _) = setup();
        let key = RateLimitKey::ip("1.1.1.1");
        for expected in [2, 1, 0] {
            let d = limiter.check(&key, quota(3)).await.unwrap();
            assert!(d.allowed);
            assert_eq!(d.limit, 3);
            assert_eq!(d.remaining, expected);
            assert_eq!(d.retry_after, None);
        }
    }

    #[tokio::test]
    async fn blocks_next_with_retry_after() {
        let (limiter, clock) = setup();
        let key = RateLimitKey::ip("1.1.1.1");
        limiter.check(&key, quota(2)).await.unwrap();
        clock.advance(Duration::from_secs(10));
        limiter.check(&key, quota(2)).await.unwrap();
        clock.advance(Duration::from_secs(5));
        let d = limiter.check(&key, quota(2)).await.unwrap();
        assert!(!d.allowed);
        assert_eq!(d.remaining, 0);
        assert_eq!(d.retry_after, Some(Duration::from_secs(45)));
    }

    #[tokio::test]
    async fn blocked_requests_do_not_extend_window() {
        let (limiter, clock) = setup();
        let key = RateLimitKey::ip("1.1.1.1");
        limiter.check(&key, quota(1)).await.unwrap();
        clock.advance(Duration::from_secs(30));
        assert!(!limiter.check(&key, quota(1)).await.unwrap().allowed);
        clock.advance(Duration::from_secs(30));
        assert!(limiter.check(&key, quota(1)).await.unwrap().allowed);
    }

    #[tokio::test]
    async fn admits_again_after_window_slides() {
        let (limiter, clock) = setup();
        let key = RateLimitKey::ip("1.1.1.1");
        limiter.check(&key, quota(1)).await.unwrap();
        assert!(!limiter.check(&key, quota(1)).await.unwrap().allowed);
        clock.advance(Duration::from_secs(60));
        let d = limiter.check(&key, quota(1)).await.unwrap();
        assert!(d.allowed);
        assert_eq!(d.remaining, 0);
    }

    #[tokio::test]
    async fn keys_are_independent() {
        let (limiter, _) = setup();
        let a = RateLimitKey::ip("1.1.1.1");
        let b = RateLimitKey::ip("2.2.2.2");
        limiter.check(&a, quota(1)).await.unwrap();
        assert!(!limiter.check(&a, quota(1)).await.unwrap().allowed);
        assert!(limiter.check(&b, quota(1)).await.unwrap().allowed);
    }

    #[tokio::test]
    async fn zero_limit_blocks_and_leaves_no_entry() {
        let (limiter, _) = setup();
        let key = RateLimitKey::ip("1.1.1.1");
        let d = limiter.check(&key, quota(0)).await.unwrap();
        assert!(!d.allowed);
        assert_eq!(d.retry_after, None);
        assert_eq!(entries(&limiter), 0);
    }

    #[tokio::test]
    async fn sweep_bounds_map_size() {
        let (limiter, clock) = setup();
        for i in 0..=SWEEP_THRESHOLD {
            let key = RateLimitKey::ip(format!("host-{i}"));
            limiter.check(&key, quota(1)).await.unwrap();
        }
        assert_eq!(entries(&limiter), SWEEP_THRESHOLD + 1);
        clock.advance(Duration::from_secs(61));
        let key = RateLimitKey::ip("fresh");
        limiter.check(&key, quota(1)).await.unwrap();
        assert_eq!(entries(&limiter), 1);
    }

    #[tokio::test]
    async fn clones_share_state() {
        let (limiter, _) = setup();
        let other = limiter.clone();
        let key = RateLimitKey::ip("1.1.1.1");
        limiter.check(&key, quota(1)).await.unwrap();
        assert!(!other.check(&key, quota(1)).await.unwrap().allowed);
    }
}
