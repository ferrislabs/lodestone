use chrono::{Duration, TimeZone};
use common::generate_uuid_v7;
use serde_json::json;

use super::{
    pipeline::{Pipeline, PipelineInput, Stage, StageReason, StageStatus, pipeline},
    *,
};
use crate::domain::{
    NormalizedDomain,
    agent_work::{CancelReason, WorkKind, WorkTarget, ports::AgentStop},
    ecosystem::{EcosystemModule, EcosystemReadiness},
    enrichment::{EvaluationStage, EvaluationStatus, Requalification},
    lead::{
        Assessment, AssessmentId, Lead, LeadId, LeadStatus, Score, Signal, SignalId,
        draft::DraftReview,
    },
    llm::RunProvenance,
    profile::{ProfileId, RevisionNumber, definition::AgentId},
    prompt::DraftMessage,
    qualification::{AgentFailure, AgentKind, SummaryState},
    values::{Provenance, SignalKind, SourceName, UnitInterval},
};

fn at(minute: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 9, 9, 0, 0).unwrap() + Duration::minutes(minute)
}

fn event(
    round: u32,
    step: TimelineStep,
    key: &str,
    started: i64,
    finished: Option<i64>,
) -> TimelineEvent {
    TimelineEvent {
        id: generate_uuid_v7(),
        lead_id: LeadId(Uuid::nil()),
        profile_id: ProfileId(Uuid::nil()),
        round,
        step,
        kind: kind::AGENT.to_owned(),
        status: if finished.is_some() {
            EventStatus::Succeeded
        } else {
            EventStatus::Started
        },
        actor: EventActor::default(),
        detail: json!({}),
        started_at: at(started),
        finished_at: finished.map(at),
        error: None,
        dedupe_key: key.to_owned(),
    }
}

#[test]
fn an_ended_span_hides_its_start_event() {
    let events = vec![
        event(1, TimelineStep::Qualification, "x:agent:a:start", 0, None),
        event(1, TimelineStep::Qualification, "x:agent:a:end", 0, Some(3)),
        event(1, TimelineStep::Qualification, "x:agent:b:start", 1, None),
    ];
    let kept = collapse(events);
    let keys: Vec<&str> = kept.iter().map(|event| event.dedupe_key.as_str()).collect();
    assert_eq!(keys, vec!["x:agent:a:end", "x:agent:b:start"]);
}

#[test]
fn events_are_ordered_by_their_timestamps_not_their_arrival() {
    let events = vec![
        event(1, TimelineStep::Inspection, "x:late", 10, Some(12)),
        event(1, TimelineStep::Qualification, "x:early", 2, Some(4)),
        event(1, TimelineStep::Scoring, "x:middle", 5, Some(8)),
    ];
    let ordered = collapse(events);
    let keys: Vec<&str> = ordered
        .iter()
        .map(|event| event.dedupe_key.as_str())
        .collect();
    assert_eq!(keys, vec!["x:late", "x:middle", "x:early"]);
}

fn round_started(round: u32, minute: i64, trigger: &str) -> TimelineEvent {
    TimelineEvent {
        kind: ROUND_STARTED.to_owned(),
        detail: json!({ "trigger": trigger }),
        ..event(
            round,
            TimelineStep::Qualification,
            &format!("x:round:{round}"),
            minute,
            None,
        )
    }
}

#[test]
fn rounds_are_newest_first_with_their_trigger() {
    let mut probe = event(2, TimelineStep::Detection, "x:signal:2", 20, None);
    probe.status = EventStatus::Succeeded;
    probe.actor = EventActor::source("http_probe");
    let mut first = event(1, TimelineStep::Detection, "x:signal:1", 0, None);
    first.status = EventStatus::Succeeded;
    first.actor = EventActor::source("ct");
    let mut manual = event(3, TimelineStep::Requalification, "x:manual", 40, None);
    manual.kind = kind::REEVALUATION_REQUESTED.to_owned();
    manual.status = EventStatus::Succeeded;
    let grouped = rounds(vec![
        first,
        round_started(1, 1, "first"),
        probe,
        round_started(2, 21, "requalification"),
        manual,
        round_started(3, 41, "manual"),
    ]);
    let summary: Vec<(u32, RoundTrigger)> = grouped
        .iter()
        .map(|round| (round.number, round.trigger))
        .collect();
    assert_eq!(
        summary,
        vec![
            (3, RoundTrigger::Manual),
            (2, RoundTrigger::NewInformation),
            (1, RoundTrigger::Detection),
        ]
    );
    assert_eq!(grouped[1].sources, vec!["http_probe".to_owned()]);
    assert_eq!(grouped[2].started_at, Some(at(1)));
}

#[test]
fn a_round_without_a_start_is_pending() {
    let mut signal = event(4, TimelineStep::Detection, "x:signal", 0, None);
    signal.status = EventStatus::Succeeded;
    let grouped = rounds(vec![signal]);
    assert_eq!(grouped[0].trigger, RoundTrigger::Pending);
}

#[test]
fn reconstructed_rounds_are_flagged() {
    let mut rebuilt = event(1, TimelineStep::Detection, "x:backfill", 0, None);
    rebuilt.detail = json!({ "reconstructed": true });
    assert!(rounds(vec![rebuilt])[0].reconstructed);
}

#[test]
fn entries_feed_the_next_round_only_for_inputs() {
    assert_eq!(TimelineStep::Detection.slot(), RoundSlot::Next);
    assert_eq!(TimelineStep::Inspection.slot(), RoundSlot::Next);
    assert_eq!(TimelineStep::Qualification.slot(), RoundSlot::Current);
    assert_eq!(TimelineStep::Decision.slot(), RoundSlot::Current);
    let round = TimelineEntry::round_started(at(0), "first", None);
    assert_eq!(round.slot, RoundSlot::Next);
    assert_eq!(round.key, "round:none");
    let lead = LeadId(Uuid::nil());
    let span = TimelineEntry::started(TimelineStep::Scoring, kind::AGENT, at(0), "scoring:a");
    assert_eq!(
        span.dedupe_key(lead),
        format!("{}:scoring:a:start", Uuid::nil())
    );
    let ended = TimelineEntry::ended(
        TimelineStep::Scoring,
        kind::AGENT,
        EventStatus::Succeeded,
        ("scoring:a", at(0)),
        at(2),
    );
    assert_eq!(ended.started_at, at(0));
    assert_eq!(ended.finished_at, Some(at(2)));
    assert!(ended.key.ends_with(SPAN_END));
    for step in TimelineStep::ALL {
        assert_eq!(TimelineStep::parse(step.as_str()), Some(step));
    }
}

fn lead(status: LeadStatus, qualified: Option<i64>) -> Lead {
    Lead {
        id: LeadId(Uuid::nil()),
        profile_id: ProfileId(Uuid::nil()),
        domain: NormalizedDomain::from_host("example.com").unwrap(),
        status,
        score: Score::default(),
        confidence: None,
        verdict: None,
        llm_score: None,
        qualified_at: qualified.map(at),
        first_seen_at: at(0),
        last_seen_at: at(0),
        updated_at: at(90),
    }
}

fn signal(kind: &str) -> Signal {
    Signal {
        id: SignalId(Uuid::nil()),
        lead_id: LeadId(Uuid::nil()),
        kind: SignalKind::parse(kind).unwrap(),
        value: None,
        source: SourceName::HttpProbe,
        raw: json!(null),
        provenance: Provenance::parse("https://example.com").unwrap(),
        strength: UnitInterval::new(0.3).unwrap(),
        observed_at: at(0),
        agent: None,
    }
}

fn status(stage: EvaluationStage, done: u32, available: u32) -> EvaluationStatus {
    EvaluationStatus {
        stage,
        evaluated_at: None,
        modules_done: done,
        modules_available: available,
        requalify_at: None,
        requalify_forced: false,
        changed_modules: Vec::new(),
    }
}

fn assessment(draft: bool) -> Assessment {
    Assessment {
        id: AssessmentId(Uuid::nil()),
        lead_id: LeadId(Uuid::nil()),
        profile_revision: RevisionNumber::new(1).unwrap(),
        summary: "Synthèse".to_owned(),
        verdict_narrative: String::new(),
        approach_angle: String::new(),
        evidence: Vec::new(),
        evidence_verified: true,
        quality_warnings: Vec::new(),
        provenance: RunProvenance::unresolved(None, String::new()),
        draft: draft.then(|| DraftMessage {
            subject: "Objet".to_owned(),
            body: "Corps".to_owned(),
        }),
        draft_review: draft.then(DraftReview::pending),
        created_at: at(30),
    }
}

struct Case {
    lead: Lead,
    profile_active: bool,
    excluded: Option<&'static str>,
    signals: Vec<Signal>,
    evaluation: EvaluationStatus,
    readiness: EcosystemReadiness,
    requalification: Option<Requalification>,
    summary: Option<SummaryState>,
    assessment: Option<Assessment>,
    failures: Vec<AgentFailure>,
    stops: Vec<AgentStop>,
    in_flight: Vec<InFlightStep>,
}

impl Case {
    fn new(lead: Lead) -> Self {
        Self {
            lead,
            profile_active: true,
            excluded: None,
            signals: vec![signal("product_detected")],
            evaluation: status(EvaluationStage::InspectionComplete, 7, 7),
            readiness: EcosystemReadiness::default(),
            requalification: None,
            summary: None,
            assessment: None,
            failures: Vec::new(),
            stops: Vec::new(),
            in_flight: Vec::new(),
        }
    }

    fn run(&self) -> Pipeline {
        pipeline(&PipelineInput {
            lead: &self.lead,
            excluded: self.excluded,
            profile_active: self.profile_active,
            signals: &self.signals,
            evaluation: &self.evaluation,
            readiness: &self.readiness,
            requalification: self.requalification.as_ref(),
            max_per_day: 3,
            qualification_agents: 2,
            scoring_agents: 0,
            scoring_results: &[],
            scoring: None,
            summary_state: self.summary.as_ref(),
            assessment: self.assessment.as_ref(),
            agent_failures: &self.failures,
            agent_stops: &self.stops,
            in_flight: &self.in_flight,
        })
    }
}

fn stage(pipeline: &Pipeline, stage: Stage) -> &pipeline::StageState {
    pipeline
        .stages
        .iter()
        .find(|state| state.stage == stage)
        .unwrap()
}

#[test]
fn the_stages_are_ordered_and_labelled_in_french() {
    let result = Case::new(lead(LeadStatus::New, None)).run();
    let labels: Vec<&str> = result
        .stages
        .iter()
        .map(|state| state.stage.label())
        .collect();
    assert_eq!(
        labels,
        vec![
            "Détection",
            "Inspection",
            "Qualification",
            "Scoring",
            "Synthèse",
            "Brouillon",
            "Décision"
        ]
    );
}

#[test]
fn a_certificate_only_lead_waits_for_a_business_signal() {
    let mut case = Case::new(lead(LeadStatus::New, None));
    case.signals = vec![signal("certificate_seen")];
    case.evaluation = status(EvaluationStage::CertificateOnly, 0, 7);
    let result = case.run();
    let detection = stage(&result, Stage::Detection);
    assert_eq!(detection.status, StageStatus::Done);
    assert_eq!(detection.reason_code, Some(StageReason::CertificateOnly));
    let qualification = stage(&result, Stage::Qualification);
    assert_eq!(qualification.status, StageStatus::Waiting);
    assert_eq!(
        qualification.reason_code,
        Some(StageReason::CertificateOnly)
    );
    assert_eq!(result.current, Some(Stage::Qualification));
}

#[test]
fn a_running_inspection_is_the_current_stage() {
    let mut case = Case::new(lead(LeadStatus::New, None));
    case.evaluation = status(EvaluationStage::InspectionRunning, 2, 7);
    case.in_flight = vec![InFlightStep {
        step: TimelineStep::Inspection,
        actor: EventActor::module("website_stack"),
        started_at: at(5),
    }];
    let result = case.run();
    let inspection = stage(&result, Stage::Inspection);
    assert_eq!(inspection.status, StageStatus::Running);
    assert_eq!(inspection.reason_code, Some(StageReason::ModuleRunning));
    assert_eq!(inspection.since, Some(at(5)));
    assert!(
        inspection
            .reason
            .as_deref()
            .unwrap()
            .contains("website_stack")
    );
    assert_eq!(result.current, Some(Stage::Inspection));
}

#[test]
fn a_module_without_a_ready_provider_is_named() {
    let mut case = Case::new(lead(LeadStatus::New, None));
    case.evaluation = status(EvaluationStage::NotEvaluated, 6, 6);
    case.readiness = EcosystemReadiness::with_not_ready(vec![EcosystemModule::LegalLife]);
    let inspection = stage(&case.run(), Stage::Inspection).clone();
    assert_eq!(inspection.status, StageStatus::Done);
    assert_eq!(inspection.reason_code, Some(StageReason::ProviderNotReady));
    assert!(inspection.reason.unwrap().contains("legal_life"));
}

#[test]
fn an_agent_call_failure_waits_for_its_backoff() {
    let mut case = Case::new(lead(LeadStatus::New, None));
    case.failures = vec![AgentFailure {
        kind: AgentKind::Qualification,
        profile_revision: RevisionNumber::new(1).unwrap(),
        agent_id: "stack".to_owned(),
        provider_id: None,
        attempts: 1,
        next_attempt_at: Some(at(12)),
        error: "timeout".to_owned(),
    }];
    let result = case.run();
    let qualification = stage(&result, Stage::Qualification);
    assert_eq!(qualification.status, StageStatus::Waiting);
    assert_eq!(
        qualification.reason_code,
        Some(StageReason::CallFailureBackoff)
    );
    assert_eq!(qualification.next_at, Some(at(12)));
    assert_eq!(result.current, Some(Stage::Qualification));
}

#[test]
fn a_planned_requalification_shows_its_time() {
    let mut case = Case::new(lead(LeadStatus::Qualified, Some(10)));
    case.requalification = Some(Requalification {
        after: Some(at(15)),
        forced: false,
        daily_count: 0,
        counted_on: None,
    });
    let qualification = stage(&case.run(), Stage::Qualification).clone();
    assert_eq!(qualification.status, StageStatus::Waiting);
    assert_eq!(
        qualification.reason_code,
        Some(StageReason::RequalificationPlanned)
    );
    assert_eq!(qualification.next_at, Some(at(15)));
}

#[test]
fn a_capped_requalification_waits_for_the_next_day() {
    let mut case = Case::new(lead(LeadStatus::Qualified, Some(10)));
    case.requalification = Some(Requalification {
        after: Some(at(15)),
        forced: false,
        daily_count: 3,
        counted_on: Some(at(15).date_naive()),
    });
    let qualification = stage(&case.run(), Stage::Qualification).clone();
    assert_eq!(
        qualification.reason_code,
        Some(StageReason::RequalificationCapped)
    );
    assert!(qualification.next_at.unwrap() > at(15));
}

#[test]
fn a_complete_lead_waits_for_the_decision_on_its_draft() {
    let mut case = Case::new(lead(LeadStatus::Qualified, Some(10)));
    case.assessment = Some(assessment(true));
    let result = case.run();
    for done in [
        Stage::Detection,
        Stage::Inspection,
        Stage::Qualification,
        Stage::Synthesis,
        Stage::Draft,
    ] {
        assert_eq!(stage(&result, done).status, StageStatus::Done, "{done:?}");
    }
    assert_eq!(stage(&result, Stage::Scoring).status, StageStatus::Skipped);
    let decision = stage(&result, Stage::Decision);
    assert_eq!(decision.status, StageStatus::Waiting);
    assert_eq!(
        decision.reason_code,
        Some(StageReason::DraftAwaitingDecision)
    );
    assert_eq!(result.current, Some(Stage::Decision));
}

#[test]
fn a_decided_lead_is_done() {
    let mut case = Case::new(lead(LeadStatus::Validated, Some(10)));
    case.assessment = Some(assessment(true));
    let result = case.run();
    assert_eq!(stage(&result, Stage::Decision).status, StageStatus::Done);
    assert_eq!(result.current, None);
}

#[test]
fn an_excluded_lead_is_blocked_after_its_detection() {
    let mut case = Case::new(lead(LeadStatus::New, None));
    case.excluded = Some("hors des pays ciblés");
    let result = case.run();
    assert_eq!(stage(&result, Stage::Detection).status, StageStatus::Done);
    let inspection = stage(&result, Stage::Inspection);
    assert_eq!(inspection.status, StageStatus::Blocked);
    assert_eq!(inspection.reason_code, Some(StageReason::Excluded));
    for later in [
        Stage::Qualification,
        Stage::Scoring,
        Stage::Synthesis,
        Stage::Draft,
        Stage::Decision,
    ] {
        assert_eq!(stage(&result, later).status, StageStatus::Skipped);
    }
    assert_eq!(result.current, Some(Stage::Inspection));
}

#[test]
fn a_stopped_agent_blocks_its_stage() {
    let mut case = Case::new(lead(LeadStatus::New, None));
    case.stops = vec![AgentStop {
        lead_id: LeadId(Uuid::nil()),
        target: WorkTarget::new(WorkKind::Qualification, &AgentId::new("stack".to_owned())),
        revision: RevisionNumber::new(1).unwrap(),
        reason: CancelReason::AgentDisabled,
        stopped_at: at(4),
    }];
    let qualification = stage(&case.run(), Stage::Qualification).clone();
    assert_eq!(qualification.status, StageStatus::Blocked);
    assert_eq!(qualification.reason_code, Some(StageReason::AgentStopped));
}

#[test]
fn a_failing_summary_waits_then_blocks() {
    let mut case = Case::new(lead(LeadStatus::New, Some(10)));
    case.summary = Some(SummaryState {
        attempts: 1,
        error: Some("timeout".to_owned()),
        next_attempt_at: Some(at(20)),
    });
    let synthesis = stage(&case.run(), Stage::Synthesis).clone();
    assert_eq!(synthesis.status, StageStatus::Waiting);
    assert_eq!(synthesis.next_at, Some(at(20)));
    case.summary = Some(SummaryState {
        attempts: 10,
        error: None,
        next_attempt_at: None,
    });
    assert_eq!(
        stage(&case.run(), Stage::Synthesis).status,
        StageStatus::Blocked
    );
}

#[test]
fn an_inactive_profile_blocks_the_qualification() {
    let mut case = Case::new(lead(LeadStatus::New, None));
    case.profile_active = false;
    let qualification = stage(&case.run(), Stage::Qualification).clone();
    assert_eq!(qualification.status, StageStatus::Blocked);
    assert_eq!(
        qualification.reason_code,
        Some(StageReason::ProfileInactive)
    );
}
