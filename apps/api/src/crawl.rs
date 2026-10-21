use std::time::Duration;

use common::RobotsPolicy;
use fetch::RobotsGate;
use lodestone_core::LodestoneUseCase;
use tokio::{
    select,
    sync::watch::Receiver,
    time::{MissedTickBehavior, interval},
};
use tracing::{info, warn};

pub const ROBOTS_POLICY_REFRESH: Duration = Duration::from_secs(60);

pub async fn refresh_robots_policy(
    usecase: &LodestoneUseCase,
    gate: &RobotsGate,
    seed: RobotsPolicy,
) -> RobotsPolicy {
    match usecase.crawl_settings(seed).await {
        Ok(settings) => {
            let previous = gate.set(settings.robots_policy);
            if previous != settings.robots_policy {
                info!(
                    robots_policy = settings.robots_policy.as_str(),
                    "crawl robots policy changed"
                );
            }
            settings.robots_policy
        }
        Err(error) => {
            warn!(%error, "crawl settings unreadable, the current robots policy is kept");
            gate.policy()
        }
    }
}

pub async fn keep_robots_policy_fresh(
    usecase: LodestoneUseCase,
    gate: RobotsGate,
    seed: RobotsPolicy,
    mut shutdown: Receiver<bool>,
) {
    let mut ticker = interval(ROBOTS_POLICY_REFRESH);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut reported = gate.robots_ignored();
    loop {
        select! {
            _ = ticker.tick() => {}
            _ = shutdown.changed() => return,
        }
        if *shutdown.borrow() {
            return;
        }
        let policy = refresh_robots_policy(&usecase, &gate, seed).await;
        let ignored = gate.robots_ignored();
        if ignored > reported {
            info!(
                robots_policy = policy.as_str(),
                robots_ignored = ignored - reported,
                robots_ignored_total = ignored,
                "requests read although robots.txt refused them"
            );
            reported = ignored;
        }
    }
}
