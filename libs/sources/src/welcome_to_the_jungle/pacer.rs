use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
    time::Duration,
};

use tokio::time::{Instant, sleep_until};

const MINUTE: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
pub struct Pacer {
    next: Mutex<HashMap<String, Instant>>,
}

pub fn spacing(requests_per_minute: u32) -> Duration {
    MINUTE / requests_per_minute.max(1)
}

impl Pacer {
    pub async fn wait(&self, host: &str, requests_per_minute: u32) {
        let now = Instant::now();
        let at = {
            let mut next = self.next.lock().unwrap_or_else(PoisonError::into_inner);
            let at = next.get(host).map_or(now, |slot| (*slot).max(now));
            next.insert(host.to_owned(), at + spacing(requests_per_minute));
            at
        };
        sleep_until(at).await;
    }
}
