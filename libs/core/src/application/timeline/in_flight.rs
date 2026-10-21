use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use chrono::{DateTime, Utc};

use crate::{
    application::llm::limits::locked,
    domain::{
        NormalizedDomain,
        lead::LeadId,
        timeline::{EventActor, InFlightStep, TimelineStep},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    Lead(LeadId),
    Domain(NormalizedDomain),
}

#[derive(Debug, Clone)]
struct Running {
    subject: Subject,
    step: InFlightStep,
}

#[derive(Debug, Default)]
pub struct InFlightRegistry {
    next: AtomicU64,
    running: Mutex<HashMap<u64, Running>>,
}

#[derive(Debug)]
pub struct InFlight<'a> {
    registry: &'a InFlightRegistry,
    id: u64,
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        locked(&self.registry.running).remove(&self.id);
    }
}

impl InFlightRegistry {
    pub fn enter(
        &self,
        subject: Subject,
        step: TimelineStep,
        actor: EventActor,
        started_at: DateTime<Utc>,
    ) -> InFlight<'_> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        locked(&self.running).insert(
            id,
            Running {
                subject,
                step: InFlightStep {
                    step,
                    actor,
                    started_at,
                },
            },
        );
        InFlight { registry: self, id }
    }

    pub fn of_lead(&self, lead: LeadId, domain: &NormalizedDomain) -> Vec<InFlightStep> {
        let mut steps: Vec<InFlightStep> = locked(&self.running)
            .values()
            .filter(|running| match &running.subject {
                Subject::Lead(id) => *id == lead,
                Subject::Domain(name) => name == domain,
            })
            .map(|running| running.step.clone())
            .collect();
        steps.sort_by_key(|step| step.started_at);
        steps
    }

    pub fn len(&self) -> usize {
        locked(&self.running).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn a_step_is_listed_while_its_guard_lives() {
        let registry = InFlightRegistry::default();
        let lead = LeadId(Uuid::nil());
        let other = LeadId(Uuid::max());
        let domain = NormalizedDomain::from_host("example.com").unwrap();
        let at = Utc.with_ymd_and_hms(2026, 10, 9, 9, 0, 0).unwrap();
        let agent = registry.enter(
            Subject::Lead(lead),
            TimelineStep::Qualification,
            EventActor::agent("stack", "Stack"),
            at,
        );
        let module = registry.enter(
            Subject::Domain(domain.clone()),
            TimelineStep::Inspection,
            EventActor::module("website_stack"),
            at,
        );
        let foreign = registry.enter(
            Subject::Lead(other),
            TimelineStep::Scoring,
            EventActor::default(),
            at,
        );
        assert_eq!(registry.of_lead(lead, &domain).len(), 2);
        drop(agent);
        let left = registry.of_lead(lead, &domain);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].step, TimelineStep::Inspection);
        drop(module);
        drop(foreign);
        assert!(registry.is_empty());
    }
}
