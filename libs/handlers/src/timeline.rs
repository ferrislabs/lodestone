use axum::{
    Extension,
    extract::{Path, State},
};
use chrono::{DateTime, Utc};
use lodestone_core::{
    EcosystemModule, EventActor, EventStatus, InFlightStep, Pipeline, RoundTrigger, Stage,
    StageState, StageStatus, TimelineEvent, TimelineRound, TimelineStep, timeline::LeadTimeline,
};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use crate::{
    auth::AuthenticatedUser,
    errors::ApiError,
    leads::lead_id,
    response::{DataEnvelope, Response},
    state::AppState,
};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimelineStepDto {
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

impl From<TimelineStep> for TimelineStepDto {
    fn from(step: TimelineStep) -> Self {
        match step {
            TimelineStep::Detection => Self::Detection,
            TimelineStep::Exclusion => Self::Exclusion,
            TimelineStep::Inspection => Self::Inspection,
            TimelineStep::Qualification => Self::Qualification,
            TimelineStep::Escalation => Self::Escalation,
            TimelineStep::Scoring => Self::Scoring,
            TimelineStep::Combination => Self::Combination,
            TimelineStep::Aggregation => Self::Aggregation,
            TimelineStep::Requalification => Self::Requalification,
            TimelineStep::Stop => Self::Stop,
            TimelineStep::Decision => Self::Decision,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventStatusDto {
    Started,
    Succeeded,
    Failed,
    Skipped,
    Waiting,
}

impl From<EventStatus> for EventStatusDto {
    fn from(status: EventStatus) -> Self {
        match status {
            EventStatus::Started => Self::Started,
            EventStatus::Succeeded => Self::Succeeded,
            EventStatus::Failed => Self::Failed,
            EventStatus::Skipped => Self::Skipped,
            EventStatus::Waiting => Self::Waiting,
        }
    }
}

/// Who did the work: an agent and its model, an inspection module, or a source.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct EventActorDto {
    /// French name of the inspection module, when the actor is one.
    pub module_label: Option<String>,
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub module: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub source: Option<String>,
}

impl From<&EventActor> for EventActorDto {
    fn from(actor: &EventActor) -> Self {
        Self {
            module_label: actor.module.as_deref().and_then(module_label),
            agent_id: actor.agent_id.clone(),
            agent_name: actor.agent_name.clone(),
            module: actor.module.clone(),
            provider: actor.provider.clone(),
            model: actor.model.clone(),
            source: actor.source.clone(),
        }
    }
}

fn module_label(module: &str) -> Option<String> {
    EcosystemModule::parse(module).map(|module| module.label().to_owned())
}

fn instant(at: DateTime<Utc>) -> String {
    at.to_rfc3339()
}

/// One step of the processing of the lead.
#[derive(Debug, Clone, Serialize, PartialEq, ToSchema)]
pub struct TimelineEventDto {
    pub id: String,
    pub round: u32,
    pub step: TimelineStepDto,
    /// What happened within the step, for example `agent`, `module`, `signal`, `call_failed`.
    pub kind: String,
    pub status: EventStatusDto,
    pub actor: EventActorDto,
    /// Facts of the event; `reconstructed` is true for events rebuilt from the data stored before the timeline existed.
    #[schema(value_type = Object)]
    pub detail: Value,
    pub started_at: String,
    pub finished_at: Option<String>,
    /// Milliseconds between the start and the end; null for an instant event.
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
    pub reconstructed: bool,
}

impl From<&TimelineEvent> for TimelineEventDto {
    fn from(event: &TimelineEvent) -> Self {
        Self {
            id: event.id.to_string(),
            round: event.round,
            step: event.step.into(),
            kind: event.kind.clone(),
            status: event.status.into(),
            actor: EventActorDto::from(&event.actor),
            detail: event.detail.clone(),
            started_at: instant(event.started_at),
            finished_at: event.finished_at.map(instant),
            duration_ms: event
                .finished_at
                .map(|finished| (finished - event.started_at).num_milliseconds().max(0)),
            error: event.error.clone(),
            reconstructed: event.reconstructed(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoundTriggerDto {
    Detection,
    NewInformation,
    Manual,
    Requalification,
    Pending,
}

impl From<RoundTrigger> for RoundTriggerDto {
    fn from(trigger: RoundTrigger) -> Self {
        match trigger {
            RoundTrigger::Detection => Self::Detection,
            RoundTrigger::NewInformation => Self::NewInformation,
            RoundTrigger::Manual => Self::Manual,
            RoundTrigger::Requalification => Self::Requalification,
            RoundTrigger::Pending => Self::Pending,
        }
    }
}

/// An evaluation round: the information that triggered it, then its work, newest event first.
#[derive(Debug, Clone, Serialize, PartialEq, ToSchema)]
pub struct TimelineRoundDto {
    pub number: u32,
    pub trigger: RoundTriggerDto,
    /// Sources and inspection modules whose information fed the round, in order of arrival.
    pub sources: Vec<String>,
    /// Same list with the French name of each inspection module; source keys are kept as they are.
    pub source_labels: Vec<String>,
    pub started_at: Option<String>,
    pub reconstructed: bool,
    pub events: Vec<TimelineEventDto>,
}

impl From<&TimelineRound> for TimelineRoundDto {
    fn from(round: &TimelineRound) -> Self {
        Self {
            number: round.number,
            trigger: round.trigger.into(),
            sources: round.sources.clone(),
            source_labels: round
                .sources
                .iter()
                .map(|source| module_label(source).unwrap_or_else(|| source.clone()))
                .collect(),
            started_at: round.started_at.map(instant),
            reconstructed: round.reconstructed,
            events: round.events.iter().map(TimelineEventDto::from).collect(),
        }
    }
}

/// A step running right now for the lead.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct InFlightDto {
    pub step: TimelineStepDto,
    pub actor: EventActorDto,
    pub started_at: String,
}

impl From<&InFlightStep> for InFlightDto {
    fn from(step: &InFlightStep) -> Self {
        Self {
            step: step.step.into(),
            actor: EventActorDto::from(&step.actor),
            started_at: instant(step.started_at),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StageDto {
    Detection,
    Inspection,
    Qualification,
    Scoring,
    Synthesis,
    Draft,
    Decision,
}

impl From<Stage> for StageDto {
    fn from(stage: Stage) -> Self {
        match stage {
            Stage::Detection => Self::Detection,
            Stage::Inspection => Self::Inspection,
            Stage::Qualification => Self::Qualification,
            Stage::Scoring => Self::Scoring,
            Stage::Synthesis => Self::Synthesis,
            Stage::Draft => Self::Draft,
            Stage::Decision => Self::Decision,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StageStatusDto {
    Done,
    Running,
    Waiting,
    Blocked,
    Skipped,
}

impl From<StageStatus> for StageStatusDto {
    fn from(status: StageStatus) -> Self {
        match status {
            StageStatus::Done => Self::Done,
            StageStatus::Running => Self::Running,
            StageStatus::Waiting => Self::Waiting,
            StageStatus::Blocked => Self::Blocked,
            StageStatus::Skipped => Self::Skipped,
        }
    }
}

/// Where a stage of the pipeline stands, why, and what comes next.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct PipelineStageDto {
    pub stage: StageDto,
    pub status: StageStatusDto,
    /// French name of the stage.
    pub label: String,
    /// Machine reason, for example `certificate_only`, `call_failure_backoff`, `requalification_planned`, `excluded`.
    pub reason_code: Option<String>,
    /// French sentence explaining the status.
    pub reason: Option<String>,
    pub since: Option<String>,
    /// French description of the next expected action.
    pub next_action: Option<String>,
    pub next_at: Option<String>,
}

impl From<&StageState> for PipelineStageDto {
    fn from(state: &StageState) -> Self {
        Self {
            stage: state.stage.into(),
            status: state.status.into(),
            label: state.stage.label().to_owned(),
            reason_code: state.reason_code.map(|code| code.as_str().to_owned()),
            reason: state.reason.clone(),
            since: state.since.map(instant),
            next_action: state.next_action.clone(),
            next_at: state.next_at.map(instant),
        }
    }
}

/// Détection, Inspection, Qualification, Scoring, Synthèse, Brouillon, Décision, in that order.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct PipelineDto {
    /// Stage to highlight: the running one, else the first one waiting or blocked.
    pub current: Option<StageDto>,
    pub stages: Vec<PipelineStageDto>,
}

impl From<&Pipeline> for PipelineDto {
    fn from(pipeline: &Pipeline) -> Self {
        Self {
            current: pipeline.current.map(StageDto::from),
            stages: pipeline.stages.iter().map(PipelineStageDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, ToSchema)]
pub struct LeadTimelineDto {
    pub lead_id: String,
    /// Evaluation rounds, newest first.
    pub rounds: Vec<TimelineRoundDto>,
    pub in_flight: Vec<InFlightDto>,
    pub pipeline: PipelineDto,
}

impl From<&LeadTimeline> for LeadTimelineDto {
    fn from(timeline: &LeadTimeline) -> Self {
        Self {
            lead_id: timeline.lead.id.to_string(),
            rounds: timeline.rounds.iter().map(TimelineRoundDto::from).collect(),
            in_flight: timeline.in_flight.iter().map(InFlightDto::from).collect(),
            pipeline: PipelineDto::from(&timeline.pipeline),
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/leads/{lead_id}/timeline",
    tag = "leads",
    security(("bearer_auth" = [])),
    params(("lead_id" = String, Path, description = "Lead identifier (UUID)")),
    responses(
        (status = 200, description = "Processing events of the lead grouped by evaluation round, the steps running now and the pipeline summary", body = DataEnvelope<LeadTimelineDto>),
        (status = 400, description = "Malformed lead identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown lead, or its domain is suppressed"),
    )
)]
pub async fn lead_timeline(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(lead_id_raw): Path<String>,
) -> Result<Response<LeadTimelineDto>, ApiError> {
    if user.is_none() {
        return Err(ApiError::Forbidden);
    }
    let timeline = state.usecase.lead_timeline(lead_id(&lead_id_raw)?).await?;
    Ok(Response::OK(LeadTimelineDto::from(&timeline)))
}
