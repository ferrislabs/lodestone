use axum::{
    Extension,
    body::Bytes,
    extract::{Path, State},
};
use chrono::Utc;
use lodestone_core::{
    NewTrial, ProfileId, ProfileTrial, SampleSize, TrialBudget, TrialId, TrialLead, TrialSummary,
    UserId, VersionSummary,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::{
    auth::AuthenticatedUser,
    errors::{ApiError, ErrorBody},
    leads::VerdictDto,
    profiles::validate,
    response::Response,
    state::AppState,
};

fn actor(user: Option<Extension<AuthenticatedUser>>) -> Result<UserId, ApiError> {
    match user {
        Some(Extension(AuthenticatedUser(user))) => Ok(user),
        None => Err(ApiError::Forbidden),
    }
}

fn profile_id(raw: &str) -> Result<ProfileId, ApiError> {
    raw.parse()
        .map_err(|_| ApiError::BadRequest("profile_id: must be a UUID".to_owned()))
}

fn trial_id(raw: &str) -> Result<TrialId, ApiError> {
    raw.parse()
        .map_err(|_| ApiError::BadRequest("trial_id: must be a UUID".to_owned()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTrialRequest {
    #[schema(value_type = Object)]
    pub definition: Value,
    pub sample_size: Option<u32>,
    pub budget_cents: Option<u32>,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct TrialDto {
    pub id: String,
    pub profile_id: String,
    pub base_revision: u32,
    pub status: String,
    pub sample_size: u8,
    pub budget_cents: u32,
    pub spent_cents: f64,
    pub leads_tried: u32,
    pub error: Option<String>,
    #[schema(format = DateTime)]
    pub created_at: String,
    #[schema(format = DateTime)]
    pub started_at: Option<String>,
    #[schema(format = DateTime)]
    pub finished_at: Option<String>,
}

impl From<&ProfileTrial> for TrialDto {
    fn from(trial: &ProfileTrial) -> Self {
        Self {
            id: trial.id.to_string(),
            profile_id: trial.profile_id.to_string(),
            base_revision: trial.base_revision.get(),
            status: trial.status.as_str().to_owned(),
            sample_size: trial.sample_size.get(),
            budget_cents: trial.budget.cents().get(),
            spent_cents: trial.spent_cents,
            leads_tried: trial.leads_tried,
            error: trial.error.clone(),
            created_at: trial.created_at.to_rfc3339(),
            started_at: trial.started_at.map(|at| at.to_rfc3339()),
            finished_at: trial.finished_at.map(|at| at.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct VersionSummaryDto {
    pub reliable: u64,
    pub reliable_validated: u64,
    pub precision: Option<f64>,
    pub rejected: u64,
    pub rejected_discarded: u64,
}

impl From<&VersionSummary> for VersionSummaryDto {
    fn from(summary: &VersionSummary) -> Self {
        Self {
            reliable: summary.reliable,
            reliable_validated: summary.reliable_validated,
            precision: summary.precision(),
            rejected: summary.rejected,
            rejected_discarded: summary.rejected_discarded,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct TrialSummaryDto {
    pub current: VersionSummaryDto,
    pub trial: VersionSummaryDto,
    pub changed: u64,
    pub errors: u64,
}

impl From<&TrialSummary> for TrialSummaryDto {
    fn from(summary: &TrialSummary) -> Self {
        Self {
            current: VersionSummaryDto::from(&summary.current),
            trial: VersionSummaryDto::from(&summary.trial),
            changed: summary.changed,
            errors: summary.errors,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct TrialLeadDto {
    pub lead_id: String,
    pub domain: String,
    pub label: String,
    pub current_verdict: Option<VerdictDto>,
    pub current_confidence: Option<f64>,
    pub trial_verdict: Option<VerdictDto>,
    pub trial_confidence: Option<f64>,
    pub error: Option<String>,
}

impl From<&TrialLead> for TrialLeadDto {
    fn from(lead: &TrialLead) -> Self {
        Self {
            lead_id: lead.lead_id.to_string(),
            domain: lead.domain.to_string(),
            label: lead.label.as_str().to_owned(),
            current_verdict: lead.current_verdict.map(VerdictDto::from),
            current_confidence: lead.current_confidence,
            trial_verdict: lead.trial_verdict.map(VerdictDto::from),
            trial_confidence: lead.trial_confidence,
            error: lead.error.clone(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct TrialDetailDto {
    #[serde(flatten)]
    pub trial: TrialDto,
    pub summary: TrialSummaryDto,
    pub leads: Vec<TrialLeadDto>,
}

#[utoipa::path(
    post,
    path = "/api/v1/profiles/{profile_id}/trials",
    tag = "trials",
    security(("bearer_auth" = [])),
    params(("profile_id" = String, Path, description = "Profile identifier (UUID)")),
    request_body = CreateTrialRequest,
    responses(
        (status = 202, description = "The trial was queued; it runs the draft on a sample of decided leads and writes nothing to the leads", body = crate::response::DataEnvelope<TrialDto>),
        (status = 400, description = "Malformed request body, malformed profile identifier, or a limit out of range"),
        (status = 422, description = "The draft definition is invalid; `details` lists the faulty field paths", body = ErrorBody),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown profile"),
        (status = 409, description = "A trial is already pending or running for the profile, or the profile has no decided lead"),
    )
)]
pub async fn create_trial(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
    body: Bytes,
) -> Result<Response<TrialDto>, ApiError> {
    let actor = actor(user)?;
    let profile_id = profile_id(&raw)?;
    let request: CreateTrialRequest = serde_json::from_slice(&body)
        .map_err(|error| ApiError::BadRequest(format!("malformed request: {error}")))?;
    let sample_size = request
        .sample_size
        .map_or(Ok(SampleSize::DEFAULT), SampleSize::new)
        .map_err(|error| ApiError::BadRequest(format!("sample_size: {error}")))?;
    let budget = request
        .budget_cents
        .map_or(Ok(TrialBudget::DEFAULT), TrialBudget::new)
        .map_err(|error| ApiError::BadRequest(format!("budget_cents: {error}")))?;
    let definition = validate(request.definition)?;
    let trial = state
        .usecase
        .create_profile_trial(
            NewTrial {
                profile_id,
                definition,
                sample_size,
                budget,
                created_by: Some(actor),
            },
            Utc::now(),
        )
        .await?;
    Ok(Response::Accepted(TrialDto::from(&trial)))
}

#[utoipa::path(
    get,
    path = "/api/v1/profiles/{profile_id}/trials",
    tag = "trials",
    security(("bearer_auth" = [])),
    params(("profile_id" = String, Path, description = "Profile identifier (UUID)")),
    responses(
        (status = 200, description = "The trials of the profile, newest first, at most 20", body = crate::response::DataEnvelope<Vec<TrialDto>>),
        (status = 400, description = "Malformed profile identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown profile"),
    )
)]
pub async fn list_trials(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
) -> Result<Response<Vec<TrialDto>>, ApiError> {
    actor(user)?;
    let trials = state.usecase.list_profile_trials(profile_id(&raw)?).await?;
    Ok(Response::OK(trials.iter().map(TrialDto::from).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/trials/{trial_id}",
    tag = "trials",
    security(("bearer_auth" = [])),
    params(("trial_id" = String, Path, description = "Trial identifier (UUID)")),
    responses(
        (status = 200, description = "The trial with its summary and the lead by lead comparison of the current and trial verdicts with the human label", body = crate::response::DataEnvelope<TrialDetailDto>),
        (status = 400, description = "Malformed trial identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown trial"),
    )
)]
pub async fn get_trial(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
) -> Result<Response<TrialDetailDto>, ApiError> {
    actor(user)?;
    let report = state.usecase.profile_trial_report(trial_id(&raw)?).await?;
    Ok(Response::OK(TrialDetailDto {
        trial: TrialDto::from(&report.trial),
        summary: TrialSummaryDto::from(&report.summary),
        leads: report.leads.iter().map(TrialLeadDto::from).collect(),
    }))
}
