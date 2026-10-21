use std::num::NonZeroU16;

use chrono::{DateTime, Duration, Utc};
use common::CoreError;
use lodestone_macros::transactional;
use serde_json::{Value, json};

use crate::{
    UserId,
    application::LodestoneUseCase,
    domain::{
        NormalizedDomain,
        agent_work::{
            ProfileAgents, WorkKind,
            ports::{AgentRegistry, AgentStop, AgentWorkRepository},
        },
        ecosystem::{
            EcosystemModule, EcosystemReadiness, ModuleOutcome, hosting_platform,
            ports::EcosystemRepository,
        },
        enrichment::{InspectionRecord, ListedEnrichment, evaluation_status, only_certificates},
        job::{JobKind, NewJob},
        lead::{
            Assessment, Lead, LeadId, LeadStatus, Signal,
            ports::{AssessmentRepository, LeadRepository, SignalRepository},
        },
        llm::RunProvenance,
        profile::definition::AgentId,
        qualification::{AgentFailure, LeadScoring, SummaryState, ports::QualificationRepository},
        quality::{DecisionKind, Rejection},
        scoring_agent::{ScoringResult, ports::ScoringRepository},
        suppression::ports::SuppressionRepository,
        timeline::{
            EventActor, EventStatus, InFlightStep, TimelineEntry, TimelineEvent, TimelineRound,
            TimelineStep, kind, micros,
            pipeline::{Pipeline, PipelineInput, pipeline},
            ports::TimelineRepository,
            rounds,
        },
    },
};

pub mod in_flight;

use in_flight::{InFlight, InFlightRegistry, Subject};

pub const TIMELINE_PURGE_JOB_KIND: &str = "timeline.purge";
pub(crate) const BUDGET_EXHAUSTED: &str = "budget du passage épuisé";
const PURGE_DEDUP_KEY: &str = "timeline:purge";
const PURGE_MAX_ATTEMPTS: NonZeroU16 = match NonZeroU16::new(3) {
    Some(attempts) => attempts,
    None => NonZeroU16::MIN,
};
const PURGE_INTERVAL_HOURS: i64 = 24;

pub fn timeline_purge_job(run_at: DateTime<Utc>) -> Result<NewJob, CoreError> {
    let kind = JobKind::parse(TIMELINE_PURGE_JOB_KIND)
        .map_err(|_| CoreError::Internal("invalid timeline purge job kind".to_owned()))?;
    Ok(NewJob {
        kind,
        payload: json!({}),
        run_at,
        max_attempts: PURGE_MAX_ATTEMPTS,
        dedup_key: Some(PURGE_DEDUP_KEY.to_owned()),
    })
}

pub fn next_timeline_purge(now: DateTime<Utc>) -> DateTime<Utc> {
    now + Duration::hours(PURGE_INTERVAL_HOURS)
}

pub fn retention_cutoff(now: DateTime<Utc>, retention_days: u32) -> DateTime<Utc> {
    now - Duration::days(i64::from(retention_days))
}

pub(crate) fn signal_entry(signal: &Signal, at: DateTime<Utc>) -> TimelineEntry {
    TimelineEntry::new(
        TimelineStep::Detection,
        kind::SIGNAL,
        EventStatus::Succeeded,
        at,
        format!("signal:{}", signal.fingerprint()),
    )
    .actor(
        EventActor::source(signal.source.as_str())
            .with_agent_id(signal.agent.as_ref().map(AgentId::as_str)),
    )
    .detail(json!({
        "signal_kind": signal.kind.as_str(),
        "value": signal.value,
        "provenance": signal.provenance.as_str(),
        "strength": signal.strength.get(),
        "observed_at": signal.observed_at,
    }))
}

pub(crate) fn exclusion_lifted(reason: &str, at: DateTime<Utc>) -> TimelineEntry {
    TimelineEntry::new(
        TimelineStep::Exclusion,
        kind::EXCLUSION_LIFTED,
        EventStatus::Succeeded,
        at,
        format!("exclusion_lifted:{}", micros(at)),
    )
    .detail(json!({ "reason": reason }))
}

pub(crate) fn requalification_planned(
    after: DateTime<Utc>,
    at: DateTime<Utc>,
    cause: &str,
    module: Option<&str>,
) -> TimelineEntry {
    let entry = TimelineEntry::new(
        TimelineStep::Requalification,
        kind::PLANNED,
        EventStatus::Waiting,
        at,
        format!(
            "requalification:{cause}:{}:{}",
            module.unwrap_or_default(),
            micros(at)
        ),
    )
    .detail(json!({ "cause": cause, "requalify_after": after }));
    match module {
        Some(module) => entry.actor(EventActor::module(module)),
        None => entry,
    }
}

pub(crate) fn module_span(module: EcosystemModule, at: DateTime<Utc>) -> String {
    format!("module:{}:{}", module.as_str(), micros(at))
}

pub(crate) fn module_ended(
    module: EcosystemModule,
    outcome: &ModuleOutcome,
    span: (&str, DateTime<Utc>),
    finished_at: DateTime<Utc>,
    changed: bool,
) -> TimelineEntry {
    let status = match outcome {
        ModuleOutcome::Done(_) => EventStatus::Succeeded,
        ModuleOutcome::NotApplicable(_) => EventStatus::Skipped,
        ModuleOutcome::Failed(_) => EventStatus::Failed,
    };
    let entry = TimelineEntry::ended(
        TimelineStep::Inspection,
        kind::MODULE,
        status,
        span,
        finished_at,
    )
    .actor(EventActor::module(module.as_str()))
    .detail(json!({
        "module_status": outcome.status().as_str(),
        "findings": outcome.findings().len(),
        "changed": changed,
    }));
    match outcome {
        ModuleOutcome::Failed(error) => entry.error(Some(error)),
        ModuleOutcome::NotApplicable(Some(note)) => entry.detail(json!({
            "module_status": outcome.status().as_str(),
            "findings": 0,
            "changed": changed,
            "note": note,
        })),
        ModuleOutcome::Done(_) | ModuleOutcome::NotApplicable(None) => entry,
    }
}

pub(crate) fn round_key(previous: Option<DateTime<Utc>>) -> String {
    previous.map_or_else(|| "none".to_owned(), micros)
}

pub(crate) fn agent_span(
    step: TimelineStep,
    agent: &str,
    round: &str,
    started: DateTime<Utc>,
) -> String {
    format!("{}:{agent}:{round}:{}", step.as_str(), micros(started))
}

pub(crate) fn agent_interrupted(
    step: TimelineStep,
    agent: (&str, &str),
    span: (&str, DateTime<Utc>),
    finished_at: DateTime<Utc>,
    (status, error): (EventStatus, Option<&str>),
) -> TimelineEntry {
    TimelineEntry::ended(step, kind::AGENT, status, span, finished_at)
        .actor(EventActor::agent(agent.0, agent.1))
        .error(error)
}

pub(crate) fn agent_started(
    step: TimelineStep,
    agent: (&str, &str),
    span: &str,
    at: DateTime<Utc>,
) -> TimelineEntry {
    TimelineEntry::started(step, kind::AGENT, at, span).actor(EventActor::agent(agent.0, agent.1))
}

pub(crate) struct AgentTrace<'a> {
    pub(crate) step: TimelineStep,
    pub(crate) agent: (&'a str, &'a str),
    pub(crate) provenance: &'a RunProvenance,
    pub(crate) valid: bool,
    pub(crate) error: Option<&'a str>,
    pub(crate) raw_samples: &'a Value,
    pub(crate) detail: Value,
    pub(crate) span: &'a str,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) finished_at: DateTime<Utc>,
}

pub(crate) fn agent_entries(trace: AgentTrace<'_>) -> Vec<TimelineEntry> {
    let actor = EventActor::agent(trace.agent.0, trace.agent.1)
        .with_model(&trace.provenance.provider_name, &trace.provenance.model);
    let status = if trace.valid {
        EventStatus::Succeeded
    } else {
        EventStatus::Failed
    };
    let mut entries: Vec<TimelineEntry> = trace
        .raw_samples
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|sample| sample.get("valid").and_then(Value::as_bool) == Some(false))
        .map(|sample| {
            let index = sample
                .get("index")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            TimelineEntry::new(
                trace.step,
                kind::SAMPLE_INVALID,
                EventStatus::Failed,
                trace.finished_at,
                format!("{}:sample:{index}", trace.span),
            )
            .actor(actor.clone())
            .detail(json!({ "sample": index }))
            .error(sample.get("error").and_then(Value::as_str))
        })
        .collect();
    entries.push(
        TimelineEntry::ended(
            trace.step,
            kind::AGENT,
            status,
            (trace.span, trace.started_at),
            trace.finished_at,
        )
        .actor(actor)
        .detail(trace.detail)
        .error(trace.error),
    );
    entries
}

pub(crate) fn call_failed(
    step: TimelineStep,
    agent: &str,
    attempts: u32,
    next_attempt_at: DateTime<Utc>,
    error: &str,
    at: DateTime<Utc>,
) -> TimelineEntry {
    TimelineEntry::new(
        step,
        kind::CALL_FAILED,
        EventStatus::Waiting,
        at,
        format!("call_failed:{}:{agent}:{}", step.as_str(), micros(at)),
    )
    .actor(EventActor::agent(agent, agent))
    .detail(json!({ "attempts": attempts, "next_attempt_at": next_attempt_at }))
    .error(Some(error))
}

pub(crate) fn combined(
    source: &str,
    round: &str,
    at: DateTime<Utc>,
    detail: Value,
) -> TimelineEntry {
    TimelineEntry::new(
        TimelineStep::Combination,
        kind::COMBINED,
        EventStatus::Succeeded,
        at,
        format!("combined:{source}:{round}"),
    )
    .detail(detail)
}

pub(crate) fn agent_stopped(stop: &AgentStop) -> TimelineEntry {
    let agent = stop.target.agent.as_str();
    TimelineEntry::new(
        TimelineStep::Stop,
        kind::AGENT_STOPPED,
        EventStatus::Skipped,
        stop.stopped_at,
        format!(
            "stop:{}:{agent}:{}",
            stop.target.kind.as_str(),
            micros(stop.stopped_at)
        ),
    )
    .actor(EventActor::agent(agent, agent))
    .detail(json!({
        "work": stop.target.kind.as_str(),
        "reason": stop.reason.as_str(),
        "revision": stop.revision.get(),
    }))
}

pub(crate) fn summary_entries(
    assessment: &Assessment,
    span: (&str, DateTime<Utc>),
    finished_at: DateTime<Utc>,
    actor: EventActor,
) -> Vec<TimelineEntry> {
    let warnings: Vec<&str> = assessment
        .quality_warnings
        .iter()
        .map(|warning| warning.as_str())
        .collect();
    let mut entries = vec![
        TimelineEntry::ended(
            TimelineStep::Aggregation,
            kind::SUMMARY,
            EventStatus::Succeeded,
            span,
            finished_at,
        )
        .actor(actor)
        .detail(json!({
            "evidence_verified": assessment.evidence_verified,
            "evidence": assessment.evidence.len(),
            "quality_warnings": warnings,
        })),
    ];
    if let Some(draft) = &assessment.draft {
        entries.push(
            TimelineEntry::new(
                TimelineStep::Aggregation,
                kind::DRAFT,
                EventStatus::Succeeded,
                finished_at,
                format!("draft:{}", assessment.id.0),
            )
            .detail(json!({ "subject": draft.subject, "assessment_id": assessment.id.0 })),
        );
    }
    if !warnings.is_empty() {
        entries.push(
            TimelineEntry::new(
                TimelineStep::Aggregation,
                kind::QUALITY_WARNINGS,
                EventStatus::Waiting,
                finished_at,
                format!("quality_warnings:{}", assessment.id.0),
            )
            .detail(json!({ "warnings": warnings })),
        );
    }
    entries
}

pub(crate) fn summary_failed(
    qualified_at: DateTime<Utc>,
    attempts: u32,
    next_attempt_at: DateTime<Utc>,
    reason: &str,
    at: DateTime<Utc>,
) -> TimelineEntry {
    TimelineEntry::new(
        TimelineStep::Aggregation,
        kind::SUMMARY_FAILED,
        EventStatus::Waiting,
        at,
        format!("summary_failed:{}:{attempts}", micros(qualified_at)),
    )
    .detail(json!({ "attempts": attempts, "next_attempt_at": next_attempt_at }))
    .error(Some(reason))
}

pub(crate) fn status_changed(
    from: LeadStatus,
    to: LeadStatus,
    at: DateTime<Utc>,
    detail: Option<Value>,
) -> TimelineEntry {
    let mut payload = json!({ "from": from.as_str(), "to": to.as_str() });
    if let (Some(Value::Object(extra)), Some(target)) = (detail, payload.as_object_mut()) {
        target.extend(extra);
    }
    TimelineEntry::new(
        TimelineStep::Decision,
        kind::STATUS_CHANGED,
        EventStatus::Succeeded,
        at,
        format!("status:{}:{}", to.as_str(), micros(at)),
    )
    .detail(payload)
}

pub(crate) fn decision_detail(
    kinds: &[DecisionKind],
    actor: UserId,
    rejection: Option<&Rejection>,
) -> Value {
    json!({
        "decisions": kinds.iter().map(|kind| kind.as_str()).collect::<Vec<_>>(),
        "user_id": actor.0,
        "reason": rejection.map(|rejection| rejection.reason().as_str()),
        "note": rejection.and_then(Rejection::note),
    })
}

pub(crate) fn reevaluation_requested(at: DateTime<Utc>) -> TimelineEntry {
    TimelineEntry::new(
        TimelineStep::Requalification,
        kind::REEVALUATION_REQUESTED,
        EventStatus::Waiting,
        at,
        format!("reevaluate:{}", micros(at)),
    )
    .detail(json!({ "requalify_after": at }))
}

pub(crate) async fn record_signal<T: TimelineRepository>(
    timeline: &mut T,
    signal: &Signal,
    at: DateTime<Utc>,
) -> Result<(), CoreError> {
    timeline
        .record(signal.lead_id, &signal_entry(signal, at))
        .await
        .map(drop)
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeadTimeline {
    pub lead: Lead,
    pub rounds: Vec<TimelineRound>,
    pub in_flight: Vec<InFlightStep>,
    pub pipeline: Pipeline,
}

struct Snapshot {
    lead: Lead,
    excluded: Option<String>,
    signals: Vec<Signal>,
    listed: ListedEnrichment,
    scoring_results: Vec<ScoringResult>,
    scoring: Option<LeadScoring>,
    summary_state: Option<SummaryState>,
    assessment: Option<Assessment>,
    failures: Vec<AgentFailure>,
    stops: Vec<AgentStop>,
    events: Vec<TimelineEvent>,
}

impl LodestoneUseCase {
    pub fn in_flight(&self) -> &InFlightRegistry {
        &self.in_flight
    }

    pub(crate) fn enter_step(
        &self,
        subject: Subject,
        step: TimelineStep,
        actor: EventActor,
        at: DateTime<Utc>,
    ) -> InFlight<'_> {
        self.in_flight.enter(subject, step, actor, at)
    }

    #[transactional(timeline)]
    pub(crate) async fn record_timeline(
        &self,
        lead: LeadId,
        entry: &TimelineEntry,
    ) -> Result<(), CoreError> {
        let mut timeline = timeline_repository;
        timeline.record(lead, entry).await.map(drop)
    }

    #[transactional(timeline)]
    pub(crate) async fn record_domain_timeline(
        &self,
        domain: &NormalizedDomain,
        entry: &TimelineEntry,
    ) -> Result<(), CoreError> {
        let mut timeline = timeline_repository;
        timeline.record_for_domain(domain, entry).await.map(drop)
    }

    #[transactional(timeline)]
    pub async fn purge_lead_events(
        &self,
        now: DateTime<Utc>,
        retention_days: u32,
    ) -> Result<u64, CoreError> {
        let mut timeline = timeline_repository;
        timeline.purge(retention_cutoff(now, retention_days)).await
    }

    #[transactional(timeline)]
    pub async fn backfill_lead_events(&self) -> Result<u64, CoreError> {
        let mut timeline = timeline_repository;
        timeline.backfill().await
    }

    pub async fn lead_timeline(&self, lead_id: LeadId) -> Result<LeadTimeline, CoreError> {
        let snapshot = self.timeline_snapshot(lead_id).await?;
        let profile = snapshot.lead.profile_id;
        let agents = match self.agents.agents(profile) {
            Some(agents) => Some(agents),
            None => self
                .list_active_profiles()
                .await?
                .iter()
                .find(|active| active.profile.id == profile)
                .map(ProfileAgents::of),
        };
        let (profile_active, qualification_agents, scoring_agents) =
            agents.as_ref().map_or((false, 0, 0), |agents| {
                (
                    agents.active,
                    agents.enabled(WorkKind::Qualification).len(),
                    agents.enabled(WorkKind::Scoring).len(),
                )
            });
        let evaluation = evaluation_status(&snapshot.listed, self.requalify.max_per_day);
        let in_flight = self
            .in_flight
            .of_lead(snapshot.lead.id, &snapshot.lead.domain);
        let pipeline = pipeline(&PipelineInput {
            lead: &snapshot.lead,
            excluded: snapshot.excluded.as_deref(),
            profile_active,
            signals: &snapshot.signals,
            evaluation: &evaluation,
            readiness: &snapshot.listed.record.readiness,
            requalification: snapshot.listed.requalification.as_ref(),
            max_per_day: self.requalify.max_per_day,
            qualification_agents,
            scoring_agents,
            scoring_results: &snapshot.scoring_results,
            scoring: snapshot.scoring.as_ref(),
            summary_state: snapshot.summary_state.as_ref(),
            assessment: snapshot.assessment.as_ref(),
            agent_failures: &snapshot.failures,
            agent_stops: &snapshot.stops,
            in_flight: &in_flight,
        });
        Ok(LeadTimeline {
            lead: snapshot.lead,
            rounds: rounds(snapshot.events),
            in_flight,
            pipeline,
        })
    }

    #[transactional(
        lead,
        suppression,
        signal,
        ecosystem,
        scoring_agent,
        qualification,
        assessment,
        agent_work,
        timeline
    )]
    async fn timeline_snapshot(&self, lead_id: LeadId) -> Result<Snapshot, CoreError> {
        let mut leads = lead_repository;
        let mut suppression = suppression_repository;
        let mut signals = signal_repository;
        let mut ecosystems = ecosystem_repository;
        let mut scoring_agents = scoring_agent_repository;
        let mut qualification = qualification_repository;
        let mut assessments = assessment_repository;
        let mut stops = agent_work_repository;
        let mut timeline = timeline_repository;
        let lead = leads.find(lead_id).await?.ok_or(CoreError::NotFound)?;
        if suppression.is_domain_suppressed(&lead.domain).await? {
            return Err(CoreError::NotFound);
        }
        let excluded = suppression.lead_exclusion(lead_id).await?;
        let lead_signals = signals.list_for_lead(lead_id).await?;
        let listed = match ecosystems.listed(&[lead_id]).await?.remove(&lead_id) {
            Some(listed) => listed,
            None => ListedEnrichment {
                record: InspectionRecord {
                    enabled: true,
                    hosting_platform: hosting_platform(&lead.domain),
                    modules: ecosystems.of_domain(&lead.domain).await?.modules,
                    readiness: EcosystemReadiness::default(),
                },
                only_certificates: only_certificates(&lead_signals),
                evaluated_at: lead.qualified_at,
                requalification: None,
            },
        };
        Ok(Snapshot {
            excluded,
            signals: lead_signals,
            listed,
            scoring_results: scoring_agents.current_results(lead_id).await?,
            scoring: qualification.scorings(&[lead_id]).await?.remove(&lead_id),
            summary_state: qualification.summary_state(lead_id).await?,
            assessment: assessments.list_for_lead(lead_id).await?.into_iter().next(),
            failures: qualification.agent_failures(lead_id).await?,
            stops: stops.stops(lead_id).await?,
            events: timeline.list(lead_id).await?,
            lead,
        })
    }
}
