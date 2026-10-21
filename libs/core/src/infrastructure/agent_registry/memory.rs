use std::sync::Arc;

use tokio::sync::watch;

use crate::domain::{
    agent_work::{AgentWork, ProfileAgents, WorkSnapshot, WorkTarget, ports::AgentRegistry},
    profile::{ActiveProfile, ProfileId, RevisionNumber},
};

#[derive(Debug)]
pub struct InMemoryAgentRegistry {
    sender: watch::Sender<Arc<WorkSnapshot>>,
}

impl InMemoryAgentRegistry {
    pub fn new() -> Self {
        Self {
            sender: watch::Sender::new(Arc::new(WorkSnapshot::default())),
        }
    }
}

impl Default for InMemoryAgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRegistry for InMemoryAgentRegistry {
    fn publish(&self, current: &ActiveProfile) {
        let agents = ProfileAgents::of(current);
        self.sender
            .send_if_modified(|snapshot| Arc::make_mut(snapshot).apply(current.profile.id, agents));
    }

    fn delete(&self, profile: ProfileId) {
        self.sender
            .send_if_modified(|snapshot| Arc::make_mut(snapshot).delete(profile));
    }

    fn refresh(&self, current: &[ActiveProfile]) {
        self.sender
            .send_if_modified(|snapshot| Arc::make_mut(snapshot).refresh(current));
    }

    fn agents(&self, profile: ProfileId) -> Option<ProfileAgents> {
        self.sender.borrow().agents(profile).cloned()
    }

    fn token(&self, profile: ProfileId, revision: RevisionNumber, target: WorkTarget) -> AgentWork {
        AgentWork::new(profile, revision, target, self.sender.subscribe())
    }
}
