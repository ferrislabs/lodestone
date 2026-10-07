use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use common::Secret;

struct Cached {
    value: Secret,
    expires_at: Instant,
}

#[derive(Default)]
struct State {
    entries: HashMap<String, Cached>,
    generations: HashMap<String, u64>,
}

#[derive(Default)]
pub struct TokenCache {
    state: Mutex<State>,
}

impl TokenCache {
    pub fn get(&self, key: &str, now: Instant, margin: Duration) -> Option<Secret> {
        let state = self.state.lock().ok()?;
        let cached = state.entries.get(key)?;
        (now + margin < cached.expires_at).then(|| cached.value.clone())
    }

    pub fn generation(&self, key: &str) -> u64 {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.generations.get(key).copied())
            .unwrap_or(0)
    }

    pub fn put(&self, key: &str, generation: u64, value: Secret, expires_at: Instant) {
        if let Ok(mut state) = self.state.lock()
            && state.generations.get(key).copied().unwrap_or(0) == generation
        {
            state
                .entries
                .insert(key.to_owned(), Cached { value, expires_at });
        }
    }

    pub fn forget(&self, key: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.entries.remove(key);
            *state.generations.entry(key.to_owned()).or_insert(0) += 1;
        }
    }
}
