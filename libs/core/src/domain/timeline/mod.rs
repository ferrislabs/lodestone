use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::{
    lead::{LeadId, str_enum},
    profile::ProfileId,
};

pub mod pipeline;
pub mod ports;

pub const ROUND_STARTED: &str = "round_started";
pub const SPAN_START: &str = ":start";
pub const SPAN_END: &str = ":end";
pub const DEFAULT_RETENTION_DAYS: u32 = 90;
pub const RECONSTRUCTED: &str = "reconstructed";

pub mod kind {
    pub const SIGNAL: &str = "signal";
    pub const EXCLUDED: &str = "excluded";
    pub const EXCLUSION_LIFTED: &str = "exclusion_lifted";
    pub const MODULE: &str = "module";
    pub const AGENT: &str = "agent";
    pub const SAMPLE_INVALID: &str = "sample_invalid";
    pub const CALL_FAILED: &str = "call_failed";
    pub const ESCALATED: &str = "escalated";
    pub const COMBINED: &str = "combined";
    pub const SUMMARY: &str = "summary";
    pub const SUMMARY_FAILED: &str = "summary_failed";
    pub const DRAFT: &str = "draft";
    pub const QUALITY_WARNINGS: &str = "quality_warnings";
    pub const PLANNED: &str = "planned";
    pub const REEVALUATION_REQUESTED: &str = "reevaluation_requested";
    pub const AGENT_STOPPED: &str = "agent_stopped";
    pub const STATUS_CHANGED: &str = "status_changed";
    pub const DECISION: &str = "decision";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimelineStep {
    Detection,
    Exclusion,
    Inspection,
    Qualification,
    Escalation,
    Scoring,
    Combination,
    Aggregation,
    Requalification,
    Stop,
    Decision,
}

str_enum!(TimelineStep {
    Detection => "detection",
    Exclusion => "exclusion",
    Inspection => "inspection",
    Qualification => "qualification",
    Escalation => "escalation",
    Scoring => "scoring",
    Combination => "combination",
    Aggregation => "aggregation",
    Requalification => "requalification",
    Stop => "stop",
    Decision => "decision",
});

impl TimelineStep {
    pub const ALL: [Self; 11] = [
        Self::Detection,
        Self::Exclusion,
        Self::Inspection,
        Self::Qualification,
        Self::Escalation,
        Self::Scoring,
        Self::Combination,
        Self::Aggregation,
        Self::Requalification,
        Self::Stop,
        Self::Decision,
    ];

    pub fn slot(self) -> RoundSlot {
        match self {
            Self::Detection | Self::Exclusion | Self::Inspection | Self::Requalification => {
                RoundSlot::Next
            }
            Self::Qualification
            | Self::Escalation
            | Self::Scoring
            | Self::Combination
            | Self::Aggregation
            | Self::Stop
            | Self::Decision => RoundSlot::Current,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventStatus {
    Started,
    Succeeded,
    Failed,
    Skipped,
    Waiting,
}

str_enum!(EventStatus {
    Started => "started",
    Succeeded => "succeeded",
    Failed => "failed",
    Skipped => "skipped",
    Waiting => "waiting",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundSlot {
    Current,
    Next,
}

impl RoundSlot {
    pub fn offset(self) -> i32 {
        match self {
            Self::Current => 0,
            Self::Next => 1,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventActor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl EventActor {
    pub fn agent(id: &str, name: &str) -> Self {
        Self {
            agent_id: Some(id.to_owned()),
            agent_name: Some(name.to_owned()),
            ..Self::default()
        }
    }

    pub fn module(module: &str) -> Self {
        Self {
            module: Some(module.to_owned()),
            ..Self::default()
        }
    }

    pub fn source(source: &str) -> Self {
        Self {
            source: Some(source.to_owned()),
            ..Self::default()
        }
    }

    pub fn with_model(mut self, provider: &str, model: &str) -> Self {
        self.provider = Some(provider.to_owned()).filter(|value| !value.is_empty());
        self.model = Some(model.to_owned()).filter(|value| !value.is_empty());
        self
    }

    pub fn with_agent_id(mut self, agent: Option<&str>) -> Self {
        self.agent_id = agent.map(str::to_owned);
        self
    }

    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineEntry {
    pub step: TimelineStep,
    pub kind: &'static str,
    pub status: EventStatus,
    pub actor: EventActor,
    pub detail: Value,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub key: String,
    pub slot: RoundSlot,
}

impl TimelineEntry {
    pub fn new(
        step: TimelineStep,
        kind: &'static str,
        status: EventStatus,
        at: DateTime<Utc>,
        key: String,
    ) -> Self {
        Self {
            step,
            kind,
            status,
            actor: EventActor::default(),
            detail: json!({}),
            started_at: at,
            finished_at: None,
            error: None,
            key,
            slot: step.slot(),
        }
    }

    pub fn started(step: TimelineStep, kind: &'static str, at: DateTime<Utc>, span: &str) -> Self {
        Self::new(
            step,
            kind,
            EventStatus::Started,
            at,
            format!("{span}{SPAN_START}"),
        )
    }

    pub fn ended(
        step: TimelineStep,
        kind: &'static str,
        status: EventStatus,
        span: (&str, DateTime<Utc>),
        finished_at: DateTime<Utc>,
    ) -> Self {
        let (span, started_at) = span;
        Self {
            finished_at: Some(finished_at),
            ..Self::new(step, kind, status, started_at, format!("{span}{SPAN_END}"))
        }
    }

    pub fn round_started(
        at: DateTime<Utc>,
        trigger: &str,
        previous: Option<DateTime<Utc>>,
    ) -> Self {
        let previous =
            previous.map_or_else(|| "none".to_owned(), |at| at.timestamp_micros().to_string());
        Self {
            detail: json!({ "trigger": trigger }),
            slot: RoundSlot::Next,
            ..Self::new(
                TimelineStep::Qualification,
                ROUND_STARTED,
                EventStatus::Started,
                at,
                format!("round:{previous}"),
            )
        }
    }

    pub fn actor(mut self, actor: EventActor) -> Self {
        self.actor = actor;
        self
    }

    pub fn detail(mut self, detail: Value) -> Self {
        self.detail = detail;
        self
    }

    pub fn error(mut self, error: Option<&str>) -> Self {
        self.error = error.map(bounded);
        self
    }

    pub fn dedupe_key(&self, lead: LeadId) -> String {
        format!("{}:{}", lead.0, self.key)
    }
}

const ERROR_MAX_CHARS: usize = 400;

fn bounded(error: &str) -> String {
    error.chars().take(ERROR_MAX_CHARS).collect()
}

pub fn micros(at: DateTime<Utc>) -> String {
    at.timestamp_micros().to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineEvent {
    pub id: Uuid,
    pub lead_id: LeadId,
    pub profile_id: ProfileId,
    pub round: u32,
    pub step: TimelineStep,
    pub kind: String,
    pub status: EventStatus,
    pub actor: EventActor,
    pub detail: Value,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub dedupe_key: String,
}

impl TimelineEvent {
    pub fn reconstructed(&self) -> bool {
        self.detail
            .get(RECONSTRUCTED)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn at(&self) -> DateTime<Utc> {
        self.finished_at
            .unwrap_or(self.started_at)
            .max(self.started_at)
    }

    fn span(&self) -> Option<&str> {
        self.dedupe_key
            .strip_suffix(SPAN_START)
            .or_else(|| self.dedupe_key.strip_suffix(SPAN_END))
    }

    fn opens_span(&self) -> bool {
        self.dedupe_key.ends_with(SPAN_START)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InFlightStep {
    pub step: TimelineStep,
    pub actor: EventActor,
    pub started_at: DateTime<Utc>,
}

pub fn newest_first(events: &mut [TimelineEvent]) {
    events.sort_by(|left, right| {
        right
            .at()
            .cmp(&left.at())
            .then_with(|| right.started_at.cmp(&left.started_at))
            .then_with(|| right.id.cmp(&left.id))
    });
}

pub fn collapse(events: Vec<TimelineEvent>) -> Vec<TimelineEvent> {
    let ended: HashSet<String> = events
        .iter()
        .filter(|event| !event.opens_span())
        .filter_map(|event| event.span().map(str::to_owned))
        .collect();
    let mut kept: Vec<TimelineEvent> = events
        .into_iter()
        .filter(|event| {
            !(event.opens_span() && event.span().is_some_and(|span| ended.contains(span)))
        })
        .collect();
    newest_first(&mut kept);
    kept
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundTrigger {
    Detection,
    NewInformation,
    Manual,
    Requalification,
    Pending,
}

str_enum!(RoundTrigger {
    Detection => "detection",
    NewInformation => "new_information",
    Manual => "manual",
    Requalification => "requalification",
    Pending => "pending",
});

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineRound {
    pub number: u32,
    pub trigger: RoundTrigger,
    pub sources: Vec<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub reconstructed: bool,
    pub events: Vec<TimelineEvent>,
}

fn trigger_of(number: u32, events: &[TimelineEvent]) -> (RoundTrigger, Option<DateTime<Utc>>) {
    let started = events.iter().find(|event| event.kind == ROUND_STARTED);
    let manual = events
        .iter()
        .any(|event| event.kind == kind::REEVALUATION_REQUESTED)
        || started
            .and_then(|event| event.detail.get("trigger"))
            .and_then(Value::as_str)
            == Some(RoundTrigger::Manual.as_str());
    let informed = events.iter().any(|event| {
        matches!(
            event.step,
            TimelineStep::Detection | TimelineStep::Inspection
        ) && event.status != EventStatus::Started
    });
    let trigger = if manual {
        RoundTrigger::Manual
    } else if started.is_none() {
        RoundTrigger::Pending
    } else if number <= 1 {
        RoundTrigger::Detection
    } else if informed {
        RoundTrigger::NewInformation
    } else {
        RoundTrigger::Requalification
    };
    (trigger, started.map(|event| event.started_at))
}

fn sources_of(events: &[TimelineEvent]) -> Vec<String> {
    let mut sources: Vec<String> = Vec::new();
    let mut ordered: Vec<&TimelineEvent> = events
        .iter()
        .filter(|event| {
            matches!(
                event.step,
                TimelineStep::Detection | TimelineStep::Inspection
            ) && event.status != EventStatus::Started
        })
        .collect();
    ordered.sort_by_key(|event| event.started_at);
    for event in ordered {
        let label = event.actor.source.as_ref().or(event.actor.module.as_ref());
        if let Some(label) = label
            && !sources.contains(label)
        {
            sources.push(label.clone());
        }
    }
    sources
}

pub fn rounds(events: Vec<TimelineEvent>) -> Vec<TimelineRound> {
    let mut grouped: BTreeMap<u32, Vec<TimelineEvent>> = BTreeMap::new();
    for event in events {
        grouped.entry(event.round).or_default().push(event);
    }
    grouped
        .into_iter()
        .rev()
        .map(|(number, members)| {
            let (trigger, started_at) = trigger_of(number, &members);
            let sources = sources_of(&members);
            let reconstructed = members.iter().all(TimelineEvent::reconstructed);
            TimelineRound {
                number,
                trigger,
                sources,
                started_at,
                reconstructed,
                events: collapse(members),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
