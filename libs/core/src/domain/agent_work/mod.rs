use std::{collections::HashMap, future::pending, sync::Arc};

use chrono::{DateTime, Utc};
use tokio::sync::watch;

use crate::domain::{
    enrichment::EnrichmentStep,
    lead::str_enum,
    profile::{
        ActiveProfile, ProfileId, RevisionNumber,
        definition::{Agent, AgentId, ProfileDefinition, SourceTarget},
    },
};

use self::ports::KeptAgents;

pub mod ports;

pub const AGGREGATOR_AGENT_ID: &str = "aggregator";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkKind {
    Sourcing,
    Enrichment,
    Qualification,
    Scoring,
    Aggregator,
}

str_enum!(WorkKind {
    Sourcing => "sourcing",
    Enrichment => "enrichment",
    Qualification => "qualification",
    Scoring => "scoring",
    Aggregator => "aggregator",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CancelReason {
    AgentRemoved,
    AgentDisabled,
    ProfileDeactivated,
    ProfileDeleted,
}

str_enum!(CancelReason {
    AgentRemoved => "agent_removed",
    AgentDisabled => "agent_disabled",
    ProfileDeactivated => "profile_deactivated",
    ProfileDeleted => "profile_deleted",
});

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkTarget {
    pub kind: WorkKind,
    pub agent: AgentId,
}

impl WorkTarget {
    pub fn new(kind: WorkKind, agent: &AgentId) -> Self {
        Self {
            kind,
            agent: agent.clone(),
        }
    }

    pub fn enrichment(step: EnrichmentStep) -> Self {
        Self {
            kind: WorkKind::Enrichment,
            agent: AgentId::new(step.as_str()),
        }
    }

    pub fn aggregator() -> Self {
        Self {
            kind: WorkKind::Aggregator,
            agent: AgentId::new(AGGREGATOR_AGENT_ID),
        }
    }

    pub fn of_source(target: &SourceTarget<'_>) -> Self {
        match target {
            SourceTarget::Agent(agent) => Self::new(WorkKind::Sourcing, agent.id()),
            SourceTarget::HttpProbe(_) => Self::enrichment(EnrichmentStep::HttpProbe),
            SourceTarget::CompanyJobs(_) => Self::enrichment(EnrichmentStep::CompanyJobs),
        }
    }
}

pub fn work_targets(definition: &ProfileDefinition) -> HashMap<WorkTarget, bool> {
    let mut targets = HashMap::new();
    for agent in &definition.agents {
        targets.insert(
            WorkTarget::new(WorkKind::Sourcing, agent.id()),
            agent.enabled(),
        );
    }
    for step in EnrichmentStep::ALL {
        targets.insert(
            WorkTarget::enrichment(step),
            step.enabled_in(&definition.enrichment),
        );
    }
    for agent in &definition.qualification_agents {
        targets.insert(
            WorkTarget::new(WorkKind::Qualification, &agent.id),
            agent.enabled,
        );
    }
    for agent in &definition.scoring_agents {
        targets.insert(WorkTarget::new(WorkKind::Scoring, &agent.id), agent.enabled);
    }
    targets.insert(WorkTarget::aggregator(), true);
    targets
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileAgents {
    pub revision: RevisionNumber,
    pub updated_at: DateTime<Utc>,
    pub active: bool,
    targets: HashMap<WorkTarget, bool>,
}

impl ProfileAgents {
    pub fn of(current: &ActiveProfile) -> Self {
        Self {
            revision: current.revision.number,
            updated_at: current.profile.updated_at,
            active: current.profile.active,
            targets: work_targets(current.revision.definition.definition()),
        }
    }

    fn is_newer_than(&self, other: &Self) -> bool {
        (self.revision, self.updated_at) >= (other.revision, other.updated_at)
    }

    pub fn check(&self, target: &WorkTarget) -> Option<CancelReason> {
        if !self.active {
            return Some(CancelReason::ProfileDeactivated);
        }
        match self.targets.get(target) {
            None => Some(CancelReason::AgentRemoved),
            Some(false) => Some(CancelReason::AgentDisabled),
            Some(true) => None,
        }
    }

    pub fn enabled(&self, kind: WorkKind) -> Vec<&AgentId> {
        let mut enabled: Vec<&AgentId> = self
            .targets
            .iter()
            .filter(|(target, enabled)| target.kind == kind && **enabled)
            .map(|(target, _)| &target.agent)
            .collect();
        enabled.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        enabled
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProfileEntry {
    Known(ProfileAgents),
    Missing,
    Deleted,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkSnapshot {
    profiles: HashMap<ProfileId, ProfileEntry>,
}

impl WorkSnapshot {
    pub fn check(&self, profile: ProfileId, target: &WorkTarget) -> Option<CancelReason> {
        match self.profiles.get(&profile) {
            None => None,
            Some(ProfileEntry::Known(agents)) => agents.check(target),
            Some(ProfileEntry::Missing | ProfileEntry::Deleted) => {
                Some(CancelReason::ProfileDeleted)
            }
        }
    }

    pub fn agents(&self, profile: ProfileId) -> Option<&ProfileAgents> {
        match self.profiles.get(&profile) {
            Some(ProfileEntry::Known(agents)) => Some(agents),
            _ => None,
        }
    }

    pub fn apply(&mut self, profile: ProfileId, agents: ProfileAgents) -> bool {
        let replace = match self.profiles.get(&profile) {
            Some(ProfileEntry::Deleted) => false,
            Some(ProfileEntry::Known(known)) => agents.is_newer_than(known) && *known != agents,
            Some(ProfileEntry::Missing) | None => true,
        };
        if replace {
            self.profiles.insert(profile, ProfileEntry::Known(agents));
        }
        replace
    }

    pub fn delete(&mut self, profile: ProfileId) -> bool {
        self.profiles.insert(profile, ProfileEntry::Deleted) != Some(ProfileEntry::Deleted)
    }

    pub fn refresh(&mut self, current: &[ActiveProfile]) -> bool {
        let mut changed = false;
        for profile in current {
            changed |= self.apply(profile.profile.id, ProfileAgents::of(profile));
        }
        for (id, entry) in &mut self.profiles {
            if matches!(entry, ProfileEntry::Known(_))
                && !current.iter().any(|profile| profile.profile.id == *id)
            {
                *entry = ProfileEntry::Missing;
                changed = true;
            }
        }
        changed
    }
}

#[derive(Debug, Clone)]
pub struct AgentWork {
    profile: ProfileId,
    revision: RevisionNumber,
    target: WorkTarget,
    snapshot: watch::Receiver<Arc<WorkSnapshot>>,
}

impl AgentWork {
    pub fn new(
        profile: ProfileId,
        revision: RevisionNumber,
        target: WorkTarget,
        snapshot: watch::Receiver<Arc<WorkSnapshot>>,
    ) -> Self {
        Self {
            profile,
            revision,
            target,
            snapshot,
        }
    }

    pub fn profile(&self) -> ProfileId {
        self.profile
    }

    pub fn revision(&self) -> RevisionNumber {
        self.revision
    }

    pub fn target(&self) -> &WorkTarget {
        &self.target
    }

    pub fn stop_reason(&self) -> Option<CancelReason> {
        self.snapshot.borrow().check(self.profile, &self.target)
    }

    pub fn is_live(&self) -> bool {
        self.stop_reason().is_none()
    }

    pub async fn cancelled(&self) -> CancelReason {
        let mut snapshot = self.snapshot.clone();
        loop {
            let reason = snapshot
                .borrow_and_update()
                .check(self.profile, &self.target);
            if let Some(reason) = reason {
                return reason;
            }
            if snapshot.changed().await.is_err() {
                return pending().await;
            }
        }
    }
}

pub fn kept_agents(agents: &ProfileAgents) -> KeptAgents {
    let names = |kind| {
        agents
            .enabled(kind)
            .into_iter()
            .map(|agent| agent.as_str().to_owned())
            .collect()
    };
    KeptAgents {
        qualification: names(WorkKind::Qualification),
        scoring: names(WorkKind::Scoring),
    }
}

pub fn only_removes_agents(previous: &ProfileDefinition, current: &ProfileDefinition) -> bool {
    if previous == current {
        return false;
    }
    let mut before = previous.clone();
    let mut after = current.clone();
    before.agents.retain(|agent| {
        agent.enabled()
            && current
                .agents
                .iter()
                .any(|kept| kept.id() == agent.id() && kept.enabled())
    });
    after.agents.retain(Agent::enabled);
    before.qualification_agents.retain(|agent| {
        agent.enabled
            && current
                .qualification_agents
                .iter()
                .any(|kept| kept.id == agent.id && kept.enabled)
    });
    after.qualification_agents.retain(|agent| agent.enabled);
    before.scoring_agents.retain(|agent| {
        agent.enabled
            && current
                .scoring_agents
                .iter()
                .any(|kept| kept.id == agent.id && kept.enabled)
    });
    after.scoring_agents.retain(|agent| agent.enabled);
    if !current.enrichment.http_probe.enabled {
        before.enrichment.http_probe = current.enrichment.http_probe.clone();
    }
    if !current.enrichment.company_jobs.enabled {
        before.enrichment.company_jobs = current.enrichment.company_jobs.clone();
    }
    before == after
}

pub fn first_stop(works: &[AgentWork]) -> Option<CancelReason> {
    works.iter().find_map(AgentWork::stop_reason)
}

pub async fn all_cancelled(works: &[AgentWork]) -> CancelReason {
    loop {
        let mut last = None;
        for work in works {
            last = Some(work.cancelled().await);
        }
        if works.iter().all(|work| !work.is_live()) {
            return match last {
                Some(reason) => reason,
                None => pending().await,
            };
        }
    }
}

#[cfg(test)]
mod tests;
