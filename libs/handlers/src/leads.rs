use std::str::FromStr;

use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use lodestone_core::{
    Assessment, ContactEmail, DraftMessage, DraftReview, DraftStatus, Lead, LeadDetail, LeadId,
    LeadQuery, LeadSort, LeadStatus, Profile, ProfileId, ScoreContribution, Signal, SignalKind,
};
use pagination::{MAX_PAGE, MAX_PER_PAGE, Page, PaginationMetadata, PaginationParams};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    auth::AuthenticatedUser, errors::ApiError, profiles::ProfileRefDto, response::Response,
    state::AppState,
};

const MAX_DOMAIN_FILTER_LENGTH: usize = 253;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LeadStatusDto {
    New,
    Qualified,
    Validated,
    Rejected,
    Contacted,
    Uncertain,
}

impl From<LeadStatus> for LeadStatusDto {
    fn from(status: LeadStatus) -> Self {
        match status {
            LeadStatus::New => Self::New,
            LeadStatus::Qualified => Self::Qualified,
            LeadStatus::Validated => Self::Validated,
            LeadStatus::Rejected => Self::Rejected,
            LeadStatus::Contacted => Self::Contacted,
            LeadStatus::Uncertain => Self::Uncertain,
        }
    }
}

impl From<LeadStatusDto> for LeadStatus {
    fn from(status: LeadStatusDto) -> Self {
        match status {
            LeadStatusDto::New => Self::New,
            LeadStatusDto::Qualified => Self::Qualified,
            LeadStatusDto::Validated => Self::Validated,
            LeadStatusDto::Rejected => Self::Rejected,
            LeadStatusDto::Contacted => Self::Contacted,
            LeadStatusDto::Uncertain => Self::Uncertain,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct LeadDto {
    pub id: String,
    pub profile: ProfileRefDto,
    pub domain: String,
    pub status: LeadStatusDto,
    pub score: f64,
    #[schema(format = DateTime)]
    pub first_seen_at: String,
    #[schema(format = DateTime)]
    pub last_seen_at: String,
    #[schema(format = DateTime)]
    pub updated_at: String,
}

impl LeadDto {
    fn new(lead: &Lead, profile: &Profile) -> Self {
        Self {
            id: lead.id.to_string(),
            profile: ProfileRefDto::from(profile),
            domain: lead.domain.as_str().to_owned(),
            status: lead.status.into(),
            score: lead.score.get(),
            first_seen_at: lead.first_seen_at.to_rfc3339(),
            last_seen_at: lead.last_seen_at.to_rfc3339(),
            updated_at: lead.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct SignalDto {
    pub id: String,
    pub kind: String,
    pub value: Option<String>,
    pub source: String,
    pub provenance: String,
    pub strength: f64,
    #[schema(format = DateTime)]
    pub observed_at: String,
    #[schema(value_type = Object)]
    pub raw: serde_json::Value,
    pub agent: Option<String>,
}

impl From<&Signal> for SignalDto {
    fn from(signal: &Signal) -> Self {
        Self {
            id: signal.id.to_string(),
            kind: signal.kind.as_str().to_owned(),
            value: signal.value.clone(),
            source: signal.source.as_str().to_owned(),
            provenance: signal.provenance.as_str().to_owned(),
            strength: signal.strength.get(),
            observed_at: signal.observed_at.to_rfc3339(),
            raw: signal.raw.clone(),
            agent: signal.agent.as_ref().map(|agent| agent.as_str().to_owned()),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ContactDto {
    pub id: String,
    pub address: String,
    pub method: String,
    pub kind: String,
    pub confidence: f64,
    pub provenance: String,
    #[schema(format = DateTime)]
    pub found_at: String,
}

impl From<&ContactEmail> for ContactDto {
    fn from(contact: &ContactEmail) -> Self {
        Self {
            id: contact.id.to_string(),
            address: contact.address.as_str().to_owned(),
            method: contact.method.as_str().to_owned(),
            kind: contact.kind.as_str().to_owned(),
            confidence: contact.confidence.get(),
            provenance: contact.provenance.as_str().to_owned(),
            found_at: contact.found_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct EvidenceDto {
    pub quote: String,
    pub provenance: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DraftStatusDto {
    Pending,
    Approved,
    Rejected,
}

impl From<DraftStatus> for DraftStatusDto {
    fn from(status: DraftStatus) -> Self {
        match status {
            DraftStatus::Pending => Self::Pending,
            DraftStatus::Approved => Self::Approved,
            DraftStatus::Rejected => Self::Rejected,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct DraftDto {
    pub subject: String,
    pub body: String,
    pub status: DraftStatusDto,
    /// Text produced by the model, present once the draft has been edited.
    pub original_subject: Option<String>,
    pub original_body: Option<String>,
    #[schema(format = DateTime)]
    pub edited_at: Option<String>,
    pub reviewed_by: Option<String>,
    #[schema(format = DateTime)]
    pub reviewed_at: Option<String>,
    pub rejection_reason: Option<String>,
}

impl DraftDto {
    pub fn new(draft: &DraftMessage, review: Option<&DraftReview>) -> Self {
        let pending = DraftReview::pending();
        let review = review.unwrap_or(&pending);
        Self {
            subject: draft.subject.clone(),
            body: draft.body.clone(),
            status: review.status.into(),
            original_subject: review
                .original
                .as_ref()
                .map(|original| original.subject.clone()),
            original_body: review
                .original
                .as_ref()
                .map(|original| original.body.clone()),
            edited_at: review.edited_at.map(|at| at.to_rfc3339()),
            reviewed_by: review.reviewed_by.map(|user| user.to_string()),
            reviewed_at: review.reviewed_at.map(|at| at.to_rfc3339()),
            rejection_reason: review.rejection_reason.clone(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct AssessmentDto {
    pub id: String,
    pub profile_revision: u32,
    pub summary: String,
    pub approach_angle: String,
    pub evidence: Vec<EvidenceDto>,
    pub model: String,
    pub prompt_version: String,
    pub draft: Option<DraftDto>,
    #[schema(format = DateTime)]
    pub created_at: String,
}

impl From<&Assessment> for AssessmentDto {
    fn from(assessment: &Assessment) -> Self {
        Self {
            id: assessment.id.to_string(),
            profile_revision: assessment.profile_revision.get(),
            summary: assessment.summary.clone(),
            approach_angle: assessment.approach_angle.clone(),
            evidence: assessment
                .evidence
                .iter()
                .map(|evidence| EvidenceDto {
                    quote: evidence.quote.clone(),
                    provenance: evidence.provenance.as_str().to_owned(),
                })
                .collect(),
            model: assessment.model.clone(),
            prompt_version: assessment.prompt_version.clone(),
            draft: assessment
                .draft
                .as_ref()
                .map(|draft| DraftDto::new(draft, assessment.draft_review.as_ref())),
            created_at: assessment.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ScoreContributionDto {
    pub signal: String,
    pub value: Option<String>,
    pub weight: f64,
    pub measure: f64,
    pub contribution: f64,
}

impl From<&ScoreContribution> for ScoreContributionDto {
    fn from(contribution: &ScoreContribution) -> Self {
        Self {
            signal: contribution.signal.clone(),
            value: contribution.value.clone(),
            weight: contribution.weight,
            measure: contribution.measure,
            contribution: contribution.contribution,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct LeadDetailDto {
    pub lead: LeadDto,
    pub score_breakdown: Vec<ScoreContributionDto>,
    pub signals: Vec<SignalDto>,
    pub contacts: Vec<ContactDto>,
    pub assessments: Vec<AssessmentDto>,
}

impl From<&LeadDetail> for LeadDetailDto {
    fn from(detail: &LeadDetail) -> Self {
        Self {
            lead: LeadDto::new(&detail.lead, &detail.profile),
            score_breakdown: detail
                .lead
                .score_breakdown
                .iter()
                .map(ScoreContributionDto::from)
                .collect(),
            signals: detail.signals.iter().map(SignalDto::from).collect(),
            contacts: detail.contacts.iter().map(ContactDto::from).collect(),
            assessments: detail.assessments.iter().map(AssessmentDto::from).collect(),
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListLeadsParams {
    /// One of new, qualified, validated, rejected, contacted, uncertain.
    #[param(value_type = Option<LeadStatusDto>)]
    pub(crate) status: Option<String>,
    /// Minimum score, inclusive. Must be a non-negative number.
    #[param(value_type = Option<f64>, minimum = 0)]
    pub(crate) min_score: Option<String>,
    /// Case-insensitive substring of the lead domain.
    pub(crate) domain: Option<String>,
    /// Only leads having at least one signal of this kind.
    pub(crate) signal: Option<String>,
    /// One of score_desc (default), score_asc, last_seen_desc, first_seen_desc, domain_asc.
    pub(crate) sort: Option<String>,
    /// Page number, starting at 1.
    #[param(value_type = Option<u64>, minimum = 1)]
    pub(crate) page: Option<String>,
    /// Page size, between 1 and 100. Defaults to 20.
    #[param(value_type = Option<u64>, minimum = 1, maximum = 100)]
    pub(crate) per_page: Option<String>,
}

pub(crate) fn invalid(field: &str, reason: &str) -> ApiError {
    ApiError::Validation(format!("{field}: {reason}"))
}

pub(crate) fn parse_number<T: FromStr>(field: &str, raw: &str) -> Result<T, ApiError> {
    raw.trim()
        .parse()
        .map_err(|_| invalid(field, "must be a number"))
}

pub(crate) fn parse_pagination(
    page: Option<&str>,
    per_page: Option<&str>,
) -> Result<PaginationParams, ApiError> {
    let page = page
        .map(|raw| parse_number::<u64>("page", raw))
        .transpose()?;
    if page.is_some_and(|page| page == 0 || page > MAX_PAGE) {
        return Err(invalid("page", "must be between 1 and 1000000"));
    }
    let per_page = per_page
        .map(|raw| parse_number::<u64>("per_page", raw))
        .transpose()?;
    if per_page.is_some_and(|per_page| per_page == 0 || per_page > MAX_PER_PAGE) {
        return Err(invalid("per_page", "must be between 1 and 100"));
    }
    Ok(PaginationParams { page, per_page })
}

impl ListLeadsParams {
    pub(crate) fn into_query(self) -> Result<LeadQuery, ApiError> {
        let status = self
            .status
            .as_deref()
            .map(|raw| {
                raw.parse::<LeadStatus>().map_err(|_| {
                    invalid(
                        "status",
                        "must be one of new, qualified, validated, rejected, contacted, uncertain",
                    )
                })
            })
            .transpose()?;

        let min_score = self
            .min_score
            .as_deref()
            .map(|raw| {
                let value = parse_number::<f64>("min_score", raw)?;
                if value.is_finite() && value >= 0.0 {
                    Ok(value)
                } else {
                    Err(invalid("min_score", "must be a non-negative number"))
                }
            })
            .transpose()?;

        let domain_contains = match self.domain.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(value) if value.len() > MAX_DOMAIN_FILTER_LENGTH => {
                return Err(invalid("domain", "is too long"));
            }
            Some(value) => Some(value.to_owned()),
        };

        let signal_kind = self
            .signal
            .as_deref()
            .map(|raw| {
                SignalKind::parse(raw)
                    .map_err(|_| invalid("signal", "must be a lowercase snake_case signal kind"))
            })
            .transpose()?;

        let sort = match self.sort.as_deref() {
            None => LeadSort::default(),
            Some(raw) => LeadSort::parse(raw).ok_or_else(|| {
                invalid(
                    "sort",
                    "must be one of score_desc, score_asc, last_seen_desc, first_seen_desc, domain_asc",
                )
            })?,
        };

        let pagination = parse_pagination(self.page.as_deref(), self.per_page.as_deref())?;

        Ok(LeadQuery {
            status,
            min_score,
            domain_contains,
            signal_kind,
            sort,
            limit: u32::try_from(pagination.per_page()).unwrap_or(u32::MAX),
            offset: pagination.offset(),
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangeLeadStatusRequest {
    pub status: LeadStatusDto,
}

fn lead_id(raw: &str) -> Result<LeadId, ApiError> {
    raw.parse()
        .map_err(|_| invalid("lead_id", "must be a UUID"))
}

#[utoipa::path(
    get,
    path = "/api/v1/profiles/{profile_id}/leads",
    tag = "leads",
    security(("bearer_auth" = [])),
    params(
        ("profile_id" = String, Path, description = "Profile identifier (UUID)"),
        ListLeadsParams,
    ),
    responses(
        (status = 200, description = "A page of leads of the profile, suppressed domains excluded", body = crate::response::DataEnvelope<Vec<LeadDto>>),
        (status = 400, description = "A filter, sort or pagination parameter is invalid"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 404, description = "Unknown profile"),
    )
)]
pub async fn list_leads(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    Query(params): Query<ListLeadsParams>,
) -> Result<Response<LeadDto>, ApiError> {
    let profile_id: ProfileId = profile_id
        .parse()
        .map_err(|_| invalid("profile_id", "must be a UUID"))?;
    let query = params.into_query()?;
    let per_page = u64::from(query.limit);
    let current_page = query.offset / per_page + 1;

    let listing = state.usecase.list_leads(profile_id, query).await?;
    let items: Vec<LeadDto> = listing
        .page
        .items
        .iter()
        .map(|lead| LeadDto::new(lead, &listing.profile))
        .collect();
    let meta = PaginationMetadata::new(
        per_page,
        current_page,
        Some(listing.page.total),
        items.is_empty(),
    );
    Ok(Response::Paginated(Page::new(items, meta)))
}

#[utoipa::path(
    get,
    path = "/api/v1/leads/{lead_id}",
    tag = "leads",
    security(("bearer_auth" = [])),
    params(("lead_id" = String, Path, description = "Lead identifier (UUID)")),
    responses(
        (status = 200, description = "The lead with its profile, signals, contacts, assessments and score breakdown", body = crate::response::DataEnvelope<LeadDetailDto>),
        (status = 400, description = "Malformed lead identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 404, description = "Unknown lead, or its domain is suppressed"),
    )
)]
pub async fn lead_detail(
    State(state): State<AppState>,
    Path(lead_id_raw): Path<String>,
) -> Result<Response<LeadDetailDto>, ApiError> {
    let detail = state.usecase.lead_detail(lead_id(&lead_id_raw)?).await?;
    Ok(Response::OK(LeadDetailDto::from(&detail)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/leads/{lead_id}/status",
    tag = "leads",
    security(("bearer_auth" = [])),
    params(("lead_id" = String, Path, description = "Lead identifier (UUID)")),
    request_body = ChangeLeadStatusRequest,
    responses(
        (status = 200, description = "The lead after the transition", body = crate::response::DataEnvelope<LeadDto>),
        (status = 400, description = "Malformed lead identifier or unknown status"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown lead, or its domain is suppressed"),
        (status = 409, description = "The transition is not allowed from the current status, or the status changed concurrently"),
    )
)]
pub async fn change_lead_status(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(lead_id_raw): Path<String>,
    body: Result<Json<ChangeLeadStatusRequest>, JsonRejection>,
) -> Result<Response<LeadDto>, ApiError> {
    let Some(Extension(AuthenticatedUser(actor))) = user else {
        return Err(ApiError::Forbidden);
    };
    let id = lead_id(&lead_id_raw)?;
    let Json(request) = body.map_err(|rejection| invalid("status", &rejection.body_text()))?;

    let lead = state
        .usecase
        .change_lead_status(id, request.status.into(), actor)
        .await?;
    let detail = state.usecase.lead_detail(lead.id).await?;
    Ok(Response::OK(LeadDto::new(&lead, &detail.profile)))
}
