use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use common::RobotsPolicy;

#[derive(Debug, Default)]
struct GateState {
    ignore: AtomicBool,
    ignored: AtomicU64,
}

#[derive(Debug, Clone, Default)]
pub struct RobotsGate {
    state: Arc<GateState>,
}

impl RobotsGate {
    pub fn new(policy: RobotsPolicy) -> Self {
        let gate = Self::default();
        gate.set(policy);
        gate
    }

    pub fn policy(&self) -> RobotsPolicy {
        if self.state.ignore.load(Ordering::Relaxed) {
            RobotsPolicy::Ignore
        } else {
            RobotsPolicy::Respect
        }
    }

    pub fn set(&self, policy: RobotsPolicy) -> RobotsPolicy {
        let previous = self
            .state
            .ignore
            .swap(policy.ignores_robots(), Ordering::Relaxed);
        if previous {
            RobotsPolicy::Ignore
        } else {
            RobotsPolicy::Respect
        }
    }

    pub fn robots_ignored(&self) -> u64 {
        self.state.ignored.load(Ordering::Relaxed)
    }

    pub(crate) fn record_ignored(&self) {
        self.state.ignored.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_the_policy_and_the_counter() {
        let gate = RobotsGate::default();
        let shared = gate.clone();
        assert_eq!(shared.set(RobotsPolicy::Ignore), RobotsPolicy::Respect);
        assert_eq!(gate.policy(), RobotsPolicy::Ignore);
        gate.record_ignored();
        assert_eq!(shared.robots_ignored(), 1);
    }
}
