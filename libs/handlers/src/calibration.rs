use axum::{Extension, extract::State};
use chrono::Utc;
use lodestone_core::{
    AgentCalibration, MIN_PER_CLASS, MIN_SAMPLES, ProfileCalibration, ReliabilityBin,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    auth::AuthenticatedUser,
    errors::ApiError,
    response::{DataEnvelope, Response},
    state::AppState,
};

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ReliabilityBinDto {
    /// Labelled leads whose predicted probability falls in the bin.
    pub n: u64,
    /// Mean probability predicted by the agent in the bin, between 0 and 1.
    pub predicted: f64,
    /// Share of the bin that humans validated, between 0 and 1.
    pub observed: f64,
}

impl From<&ReliabilityBin> for ReliabilityBinDto {
    fn from(bin: &ReliabilityBin) -> Self {
        Self {
            n: bin.n,
            predicted: bin.predicted,
            observed: bin.observed,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct AgentCalibrationDto {
    /// One of `scoring`, `qualification`.
    pub kind: String,
    pub agent_id: String,
    pub name: String,
    /// Labelled leads the agent answered validly, latest validate or reject decision of each.
    pub n: u64,
    pub positives: u64,
    /// Brier score of the raw probabilities, absent without labelled leads.
    pub brier_before: Option<f64>,
    /// Brier score of the calibrated probabilities on the same leads, absent while the calibration is not active.
    pub brier_after: Option<f64>,
    /// One of `active`, `insufficient`, `disabled`.
    pub status: String,
    /// Raw predictions against observed outcomes, ten bins from 0 to 1, empty bins omitted.
    pub reliability: Vec<ReliabilityBinDto>,
    #[schema(format = DateTime)]
    pub fitted_at: Option<String>,
}

impl From<&AgentCalibration> for AgentCalibrationDto {
    fn from(agent: &AgentCalibration) -> Self {
        Self {
            kind: agent.kind.as_str().to_owned(),
            agent_id: agent.agent_id.clone(),
            name: agent.name.clone(),
            n: agent.n,
            positives: agent.positives,
            brier_before: agent.brier_before,
            brier_after: agent.brier_after,
            status: agent.status.as_str().to_owned(),
            reliability: agent
                .reliability
                .iter()
                .map(ReliabilityBinDto::from)
                .collect(),
            fitted_at: agent.fitted_at.map(|at| at.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ProfileCalibrationDto {
    pub profile_id: String,
    pub name: String,
    pub agents: Vec<AgentCalibrationDto>,
}

impl From<&ProfileCalibration> for ProfileCalibrationDto {
    fn from(profile: &ProfileCalibration) -> Self {
        Self {
            profile_id: profile.profile_id.to_string(),
            name: profile.name.clone(),
            agents: profile
                .agents
                .iter()
                .map(AgentCalibrationDto::from)
                .collect(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct CalibrationDto {
    /// Labelled leads an agent needs before its calibration is applied.
    pub required: u64,
    /// Labelled leads needed in each class, validated and rejected.
    pub required_per_class: u64,
    pub profiles: Vec<ProfileCalibrationDto>,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct CalibrationRunDto {
    /// False when a manual run was already waiting for the worker.
    pub queued: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/quality/calibration",
    tag = "quality",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "State of the learned calibration of every agent of every profile", body = DataEnvelope<CalibrationDto>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
    )
)]
pub async fn calibration(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
) -> Result<Response<CalibrationDto>, ApiError> {
    if user.is_none() {
        return Err(ApiError::Forbidden);
    }
    let report = state.usecase.calibration_report().await?;
    Ok(Response::OK(CalibrationDto {
        required: MIN_SAMPLES,
        required_per_class: MIN_PER_CLASS,
        profiles: report.iter().map(ProfileCalibrationDto::from).collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/quality/calibration/run",
    tag = "quality",
    security(("bearer_auth" = [])),
    responses(
        (status = 202, description = "The refit is queued; the worker picks it up within its poll interval", body = DataEnvelope<CalibrationRunDto>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
    )
)]
pub async fn run_calibration(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
) -> Result<Response<CalibrationRunDto>, ApiError> {
    if user.is_none() {
        return Err(ApiError::Forbidden);
    }
    let queued = state.usecase.enqueue_calibration(Utc::now()).await?;
    Ok(Response::Accepted(CalibrationRunDto { queued }))
}
