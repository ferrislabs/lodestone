use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use texting_robots::Robot;

const AGENT: &str = "Lodestone";
const FAILURE_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_ENTRIES: usize = 4096;

pub(crate) enum RobotsRule {
    AllowAll,
    DisallowAll,
    Rules(Robot),
}

impl RobotsRule {
    pub(crate) fn from_body(body: &[u8]) -> Self {
        match Robot::new(AGENT, body) {
            Ok(robot) => Self::Rules(robot),
            Err(_) => Self::DisallowAll,
        }
    }

    pub(crate) fn allows(&self, url: &str) -> bool {
        match self {
            Self::AllowAll => true,
            Self::DisallowAll => false,
            Self::Rules(robot) => robot.allowed(url),
        }
    }
}

struct Entry {
    rule: Arc<RobotsRule>,
    expires: Instant,
}

#[derive(Default)]
pub(crate) struct RobotsCache {
    entries: Mutex<HashMap<String, Entry>>,
}

impl RobotsCache {
    pub(crate) fn get(&self, origin: &str) -> Option<Arc<RobotsRule>> {
        let entries = self.entries.lock().ok()?;
        entries
            .get(origin)
            .filter(|entry| entry.expires > Instant::now())
            .map(|entry| Arc::clone(&entry.rule))
    }

    pub(crate) fn insert(&self, origin: &str, rule: Arc<RobotsRule>, ttl: Duration) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        let now = Instant::now();
        entries.retain(|_, entry| entry.expires > now);
        if entries.len() >= MAX_ENTRIES {
            entries.clear();
        }
        entries.insert(
            origin.to_owned(),
            Entry {
                rule,
                expires: now + ttl,
            },
        );
    }

    pub(crate) fn failure_ttl() -> Duration {
        FAILURE_TTL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_apply_to_lodestone_agent() {
        let rule = RobotsRule::from_body(b"User-agent: Lodestone\nDisallow: /private\n");
        assert!(!rule.allows("https://example.com/private/x"));
        assert!(rule.allows("https://example.com/public"));
    }

    #[test]
    fn wildcard_group_applies() {
        let rule = RobotsRule::from_body(b"User-agent: *\nDisallow: /\n");
        assert!(!rule.allows("https://example.com/"));
    }

    #[test]
    fn cache_expires_entries() {
        let cache = RobotsCache::default();
        cache.insert("o", Arc::new(RobotsRule::AllowAll), Duration::ZERO);
        assert!(cache.get("o").is_none());
        cache.insert("o", Arc::new(RobotsRule::AllowAll), Duration::from_secs(60));
        assert!(cache.get("o").is_some());
    }
}
