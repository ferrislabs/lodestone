use chrono::{DateTime, Utc};

use crate::domain::{
    agent_work::{WorkKind, ports::AgentStop},
    ecosystem::{EcosystemModule, EcosystemReadiness},
    enrichment::{EvaluationStage, EvaluationStatus, Requalification},
    lead::{Assessment, Lead, LeadStatus, Signal, draft::DraftStatus, str_enum},
    qualification::{AgentFailure, AgentKind, LeadScoring, SummaryState, VerdictReason},
    scoring_agent::ScoringResult,
    timeline::{InFlightStep, TimelineStep},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    Detection,
    Inspection,
    Qualification,
    Scoring,
    Synthesis,
    Draft,
    Decision,
}

str_enum!(Stage {
    Detection => "detection",
    Inspection => "inspection",
    Qualification => "qualification",
    Scoring => "scoring",
    Synthesis => "synthesis",
    Draft => "draft",
    Decision => "decision",
});

impl Stage {
    pub const ALL: [Self; 7] = [
        Self::Detection,
        Self::Inspection,
        Self::Qualification,
        Self::Scoring,
        Self::Synthesis,
        Self::Draft,
        Self::Decision,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Detection => "Détection",
            Self::Inspection => "Inspection",
            Self::Qualification => "Qualification",
            Self::Scoring => "Scoring",
            Self::Synthesis => "Synthèse",
            Self::Draft => "Brouillon",
            Self::Decision => "Décision",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageStatus {
    Done,
    Running,
    Waiting,
    Blocked,
    Skipped,
}

str_enum!(StageStatus {
    Done => "done",
    Running => "running",
    Waiting => "waiting",
    Blocked => "blocked",
    Skipped => "skipped",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageReason {
    Signals,
    CertificateOnly,
    NoSignal,
    Excluded,
    InspectionDisabled,
    ModulesPending,
    ModuleRunning,
    ProviderNotReady,
    Queued,
    AwaitingQualification,
    AwaitingScoring,
    AwaitingSynthesis,
    CallFailureBackoff,
    CallFailed,
    AgentStopped,
    NoAgent,
    ProfileInactive,
    RequalificationPlanned,
    ReevaluationRequested,
    RequalificationCapped,
    AgentRunning,
    SummaryBackoff,
    SummaryFailed,
    NoDraft,
    DraftAwaitingDecision,
    DraftApproved,
    DraftRejected,
    Decided,
    AwaitingDecision,
}

str_enum!(StageReason {
    Signals => "signals",
    CertificateOnly => "certificate_only",
    NoSignal => "no_signal",
    Excluded => "excluded",
    InspectionDisabled => "inspection_disabled",
    ModulesPending => "modules_pending",
    ModuleRunning => "module_running",
    ProviderNotReady => "provider_not_ready",
    Queued => "queued",
    AwaitingQualification => "awaiting_qualification",
    AwaitingScoring => "awaiting_scoring",
    AwaitingSynthesis => "awaiting_synthesis",
    CallFailureBackoff => "call_failure_backoff",
    CallFailed => "call_failed",
    AgentStopped => "agent_stopped",
    NoAgent => "no_agent",
    ProfileInactive => "profile_inactive",
    RequalificationPlanned => "requalification_planned",
    ReevaluationRequested => "reevaluation_requested",
    RequalificationCapped => "requalification_capped",
    AgentRunning => "agent_running",
    SummaryBackoff => "summary_backoff",
    SummaryFailed => "summary_failed",
    NoDraft => "no_draft",
    DraftAwaitingDecision => "draft_awaiting_decision",
    DraftApproved => "draft_approved",
    DraftRejected => "draft_rejected",
    Decided => "decided",
    AwaitingDecision => "awaiting_decision",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageState {
    pub stage: Stage,
    pub status: StageStatus,
    pub reason_code: Option<StageReason>,
    pub reason: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub next_action: Option<String>,
    pub next_at: Option<DateTime<Utc>>,
}

impl StageState {
    fn new(stage: Stage, status: StageStatus) -> Self {
        Self {
            stage,
            status,
            reason_code: None,
            reason: None,
            since: None,
            next_action: None,
            next_at: None,
        }
    }

    fn because(mut self, code: StageReason, reason: String) -> Self {
        self.reason_code = Some(code);
        self.reason = Some(reason);
        self
    }

    fn since(mut self, at: Option<DateTime<Utc>>) -> Self {
        self.since = at;
        self
    }

    fn next(mut self, action: &str, at: Option<DateTime<Utc>>) -> Self {
        self.next_action = Some(action.to_owned());
        self.next_at = at;
        self
    }

    fn settled(&self) -> bool {
        matches!(self.status, StageStatus::Done | StageStatus::Skipped)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pipeline {
    pub stages: Vec<StageState>,
    pub current: Option<Stage>,
}

#[derive(Debug, Clone, Copy)]
pub struct PipelineInput<'a> {
    pub lead: &'a Lead,
    pub excluded: Option<&'a str>,
    pub profile_active: bool,
    pub signals: &'a [Signal],
    pub evaluation: &'a EvaluationStatus,
    pub readiness: &'a EcosystemReadiness,
    pub requalification: Option<&'a Requalification>,
    pub max_per_day: u32,
    pub qualification_agents: usize,
    pub scoring_agents: usize,
    pub scoring_results: &'a [ScoringResult],
    pub scoring: Option<&'a LeadScoring>,
    pub summary_state: Option<&'a SummaryState>,
    pub assessment: Option<&'a Assessment>,
    pub agent_failures: &'a [AgentFailure],
    pub agent_stops: &'a [AgentStop],
    pub in_flight: &'a [InFlightStep],
}

fn running<'a>(input: &PipelineInput<'a>, steps: &[TimelineStep]) -> Option<&'a InFlightStep> {
    input
        .in_flight
        .iter()
        .filter(|step| steps.contains(&step.step))
        .min_by_key(|step| step.started_at)
}

fn running_state(stage: Stage, step: &InFlightStep) -> StageState {
    let actor = step
        .actor
        .agent_name
        .as_ref()
        .or(step.actor.agent_id.as_ref())
        .or(step.actor.module.as_ref());
    let (code, reason) = match (stage, actor) {
        (Stage::Inspection, Some(module)) => (
            StageReason::ModuleRunning,
            format!("Module {module} en cours"),
        ),
        (_, Some(agent)) => (StageReason::AgentRunning, format!("{agent} en cours")),
        (_, None) => (StageReason::AgentRunning, "En cours".to_owned()),
    };
    StageState::new(stage, StageStatus::Running)
        .because(code, reason)
        .since(Some(step.started_at))
}

fn stopped(input: &PipelineInput<'_>, stage: Stage, kind: WorkKind) -> Option<StageState> {
    let stop = input
        .agent_stops
        .iter()
        .find(|stop| stop.target.kind == kind)?;
    Some(
        StageState::new(stage, StageStatus::Blocked)
            .because(
                StageReason::AgentStopped,
                format!(
                    "Agent {} arrêté ({})",
                    stop.target.agent.as_str(),
                    stop_label(stop.reason.as_str())
                ),
            )
            .since(Some(stop.stopped_at))
            .next("Réactiver l'agent ou relancer l'évaluation", None),
    )
}

fn stop_label(reason: &str) -> &'static str {
    match reason {
        "agent_removed" => "agent supprimé",
        "agent_disabled" => "agent désactivé",
        "profile_deactivated" => "profil désactivé",
        "profile_deleted" => "profil supprimé",
        _ => "arrêt",
    }
}

fn failing(input: &PipelineInput<'_>, stage: Stage, kind: AgentKind) -> Option<StageState> {
    let failure = input
        .agent_failures
        .iter()
        .find(|failure| failure.kind == kind)?;
    let state = if failure.is_failed() {
        StageState::new(stage, StageStatus::Blocked)
            .because(
                StageReason::CallFailed,
                format!(
                    "Appel au modèle de l'agent {} en échec après {} tentatives",
                    failure.agent_id, failure.attempts
                ),
            )
            .next("Relancer les agents", None)
    } else {
        StageState::new(stage, StageStatus::Waiting)
            .because(
                StageReason::CallFailureBackoff,
                format!(
                    "Échec de l'appel au modèle de l'agent {} (tentative {})",
                    failure.agent_id, failure.attempts
                ),
            )
            .next("Nouvel essai de l'appel", failure.next_attempt_at)
    };
    Some(state)
}

fn detection(input: &PipelineInput<'_>) -> StageState {
    if input.signals.is_empty() {
        return StageState::new(Stage::Detection, StageStatus::Waiting)
            .because(StageReason::NoSignal, "Aucun signal enregistré".to_owned());
    }
    let state =
        StageState::new(Stage::Detection, StageStatus::Done).since(Some(input.lead.first_seen_at));
    if input.evaluation.stage == EvaluationStage::CertificateOnly {
        state.because(
            StageReason::CertificateOnly,
            "Seulement un certificat observé".to_owned(),
        )
    } else {
        let count = input.signals.len();
        let noun = if count > 1 { "signaux" } else { "signal" };
        state.because(StageReason::Signals, format!("{count} {noun}"))
    }
}

fn not_ready_modules(input: &PipelineInput<'_>) -> Vec<&'static str> {
    EcosystemModule::AVAILABLE
        .into_iter()
        .filter(|module| !input.readiness.is_ready(*module))
        .map(EcosystemModule::as_str)
        .collect()
}

fn inspection(input: &PipelineInput<'_>) -> StageState {
    if let Some(step) = running(input, &[TimelineStep::Inspection]) {
        return running_state(Stage::Inspection, step);
    }
    let evaluation = input.evaluation;
    if evaluation.modules_available == 0 {
        return StageState::new(Stage::Inspection, StageStatus::Skipped).because(
            StageReason::InspectionDisabled,
            "Aucun module d'inspection actif".to_owned(),
        );
    }
    let not_ready = not_ready_modules(input);
    if evaluation.modules_done >= evaluation.modules_available {
        let state = StageState::new(Stage::Inspection, StageStatus::Done);
        return if not_ready.is_empty() {
            state.because(
                StageReason::ModulesPending,
                format!(
                    "{}/{} modules terminés",
                    evaluation.modules_done, evaluation.modules_available
                ),
            )
        } else {
            state.because(
                StageReason::ProviderNotReady,
                format!("Fournisseur non prêt : {}", not_ready.join(", ")),
            )
        };
    }
    StageState::new(Stage::Inspection, StageStatus::Waiting)
        .because(
            StageReason::ModulesPending,
            format!(
                "{}/{} modules terminés",
                evaluation.modules_done, evaluation.modules_available
            ),
        )
        .next("Inspection des modules restants", None)
}

fn qualification(input: &PipelineInput<'_>) -> StageState {
    if let Some(step) = running(
        input,
        &[TimelineStep::Qualification, TimelineStep::Escalation],
    ) {
        return running_state(Stage::Qualification, step);
    }
    if let Some(state) = stopped(input, Stage::Qualification, WorkKind::Qualification) {
        return state;
    }
    if let Some(state) = failing(input, Stage::Qualification, AgentKind::Qualification) {
        return state;
    }
    if !input.profile_active {
        return StageState::new(Stage::Qualification, StageStatus::Blocked)
            .because(StageReason::ProfileInactive, "Profil inactif".to_owned())
            .next("Activer le profil", None);
    }
    if input.qualification_agents == 0 {
        return StageState::new(Stage::Qualification, StageStatus::Skipped).because(
            StageReason::NoAgent,
            "Aucun agent de qualification actif".to_owned(),
        );
    }
    if input.evaluation.stage == EvaluationStage::CertificateOnly {
        return StageState::new(Stage::Qualification, StageStatus::Waiting)
            .because(
                StageReason::CertificateOnly,
                "Seulement un certificat : la qualification attend un signal métier".to_owned(),
            )
            .next("Attente d'un signal au-delà du certificat", None);
    }
    let Some(qualified_at) = input.lead.qualified_at else {
        return StageState::new(Stage::Qualification, StageStatus::Waiting)
            .because(
                StageReason::Queued,
                "En file pour la première évaluation".to_owned(),
            )
            .next("Première évaluation", None);
    };
    if let Some(requalification) = input.requalification
        && let (Some(after), Some(due)) = (
            requalification.after,
            requalification.due_at(input.max_per_day),
        )
    {
        let state =
            StageState::new(Stage::Qualification, StageStatus::Waiting).since(Some(qualified_at));
        return if due > after {
            state
                .because(
                    StageReason::RequalificationCapped,
                    format!(
                        "Plafond de {} requalifications par jour atteint",
                        input.max_per_day
                    ),
                )
                .next("Requalification", Some(due))
        } else if requalification.forced {
            state
                .because(
                    StageReason::ReevaluationRequested,
                    "Réévaluation demandée".to_owned(),
                )
                .next("Réévaluation", Some(due))
        } else {
            state
                .because(
                    StageReason::RequalificationPlanned,
                    "Nouvelles informations : requalification planifiée".to_owned(),
                )
                .next("Requalification", Some(due))
        };
    }
    StageState::new(Stage::Qualification, StageStatus::Done).since(Some(qualified_at))
}

fn scoring(input: &PipelineInput<'_>, requalifying: bool) -> StageState {
    if input.scoring_agents == 0 {
        return StageState::new(Stage::Scoring, StageStatus::Skipped).because(
            StageReason::NoAgent,
            "Aucun agent de scoring actif".to_owned(),
        );
    }
    if let Some(step) = running(input, &[TimelineStep::Scoring, TimelineStep::Combination]) {
        return running_state(Stage::Scoring, step);
    }
    if let Some(state) = stopped(input, Stage::Scoring, WorkKind::Scoring) {
        return state;
    }
    if let Some(state) = failing(input, Stage::Scoring, AgentKind::Scoring) {
        return state;
    }
    if requalifying || input.lead.qualified_at.is_none() {
        return StageState::new(Stage::Scoring, StageStatus::Waiting).because(
            StageReason::AwaitingQualification,
            "En attente de la qualification".to_owned(),
        );
    }
    let awaiting = input
        .scoring
        .is_some_and(|scoring| scoring.reason == Some(VerdictReason::AwaitingScoring));
    if awaiting || input.scoring_results.is_empty() {
        return StageState::new(Stage::Scoring, StageStatus::Waiting)
            .because(StageReason::Queued, "En file pour le scoring".to_owned())
            .next("Scoring des agents", None);
    }
    StageState::new(Stage::Scoring, StageStatus::Done).since(
        input
            .scoring_results
            .iter()
            .map(|result| result.created_at)
            .max(),
    )
}

fn synthesis(input: &PipelineInput<'_>, scoring: &StageState) -> StageState {
    if let Some(step) = running(input, &[TimelineStep::Aggregation]) {
        return running_state(Stage::Synthesis, step);
    }
    if let Some(state) = stopped(input, Stage::Synthesis, WorkKind::Aggregator) {
        return state;
    }
    let Some(qualified_at) = input.lead.qualified_at else {
        return StageState::new(Stage::Synthesis, StageStatus::Waiting).because(
            StageReason::AwaitingQualification,
            "En attente de la qualification".to_owned(),
        );
    };
    if let Some(assessment) = input.assessment
        && assessment.created_at >= qualified_at
    {
        return StageState::new(Stage::Synthesis, StageStatus::Done)
            .since(Some(assessment.created_at));
    }
    if let Some(summary) = input.summary_state
        && summary.attempts > 0
    {
        let error = summary.error.as_deref().unwrap_or("erreur inconnue");
        return if summary.is_failed() {
            StageState::new(Stage::Synthesis, StageStatus::Blocked)
                .because(
                    StageReason::SummaryFailed,
                    format!("Synthèse en échec : {error}"),
                )
                .next("Relancer la synthèse", None)
        } else {
            StageState::new(Stage::Synthesis, StageStatus::Waiting)
                .because(
                    StageReason::SummaryBackoff,
                    format!("Synthèse en échec ({error})"),
                )
                .next("Nouvel essai de la synthèse", summary.next_attempt_at)
        };
    }
    if !scoring.settled() {
        return StageState::new(Stage::Synthesis, StageStatus::Waiting).because(
            StageReason::AwaitingScoring,
            "En attente du scoring".to_owned(),
        );
    }
    StageState::new(Stage::Synthesis, StageStatus::Waiting)
        .because(StageReason::Queued, "En file pour la synthèse".to_owned())
        .next("Synthèse", None)
}

fn draft(input: &PipelineInput<'_>) -> StageState {
    let Some(assessment) = input.assessment else {
        return StageState::new(Stage::Draft, StageStatus::Waiting).because(
            StageReason::AwaitingSynthesis,
            "En attente de la synthèse".to_owned(),
        );
    };
    if assessment.draft.is_none() {
        return StageState::new(Stage::Draft, StageStatus::Skipped)
            .because(StageReason::NoDraft, "Aucun brouillon proposé".to_owned());
    }
    let status = assessment
        .draft_review
        .as_ref()
        .map_or(DraftStatus::Pending, |review| review.status);
    let state = StageState::new(Stage::Draft, StageStatus::Done).since(Some(assessment.created_at));
    match status {
        DraftStatus::Pending => state.because(
            StageReason::DraftAwaitingDecision,
            "Brouillon prêt, en attente de relecture".to_owned(),
        ),
        DraftStatus::Approved => {
            state.because(StageReason::DraftApproved, "Brouillon approuvé".to_owned())
        }
        DraftStatus::Rejected => {
            state.because(StageReason::DraftRejected, "Brouillon rejeté".to_owned())
        }
    }
}

fn status_label(status: LeadStatus) -> &'static str {
    match status {
        LeadStatus::New => "nouveau",
        LeadStatus::Qualified => "qualifié",
        LeadStatus::Validated => "validé",
        LeadStatus::Rejected => "rejeté",
        LeadStatus::Contacted => "contacté",
        LeadStatus::Uncertain => "incertain",
    }
}

fn decision(input: &PipelineInput<'_>) -> StageState {
    let status = input.lead.status;
    if !matches!(status, LeadStatus::New | LeadStatus::Qualified) {
        return StageState::new(Stage::Decision, StageStatus::Done)
            .because(
                StageReason::Decided,
                format!("Lead {}", status_label(status)),
            )
            .since(Some(input.lead.updated_at));
    }
    let pending_draft = input.assessment.is_some_and(|assessment| {
        assessment.draft.is_some()
            && assessment
                .draft_review
                .as_ref()
                .is_none_or(|review| review.status == DraftStatus::Pending)
    });
    if pending_draft {
        return StageState::new(Stage::Decision, StageStatus::Waiting)
            .because(
                StageReason::DraftAwaitingDecision,
                "Brouillon en attente de décision".to_owned(),
            )
            .next("Valider ou rejeter le brouillon", None);
    }
    StageState::new(Stage::Decision, StageStatus::Waiting).because(
        StageReason::AwaitingDecision,
        "En attente d'une décision".to_owned(),
    )
}

fn excluded(stages: &mut [StageState], reason: &str) {
    let mut first = true;
    for state in stages.iter_mut().skip(1) {
        let status = if first {
            StageStatus::Blocked
        } else {
            StageStatus::Skipped
        };
        first = false;
        *state = StageState::new(state.stage, status)
            .because(StageReason::Excluded, format!("Lead exclu : {reason}"));
    }
}

pub fn pipeline(input: &PipelineInput<'_>) -> Pipeline {
    let detection = detection(input);
    let inspection = inspection(input);
    let qualification = qualification(input);
    let scoring = scoring(input, qualification.status == StageStatus::Running);
    let synthesis = synthesis(input, &scoring);
    let draft = draft(input);
    let decision = decision(input);
    let mut stages = vec![
        detection,
        inspection,
        qualification,
        scoring,
        synthesis,
        draft,
        decision,
    ];
    if let Some(reason) = input.excluded {
        excluded(&mut stages, reason);
    }
    let current = stages
        .iter()
        .find(|state| state.status == StageStatus::Running)
        .or_else(|| {
            stages
                .iter()
                .find(|state| state.stage != Stage::Inspection && !state.settled())
        })
        .or_else(|| stages.iter().find(|state| !state.settled()))
        .map(|state| state.stage);
    Pipeline { stages, current }
}
