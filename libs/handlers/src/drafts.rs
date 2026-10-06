use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use lodestone_core::{
    AssessmentId, ContactEmail, DraftEdit, DraftEntry, DraftProblem, ProfileId, Rejection,
};
use pagination::{Page, PaginationMetadata};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    auth::AuthenticatedUser,
    errors::{ApiError, ErrorDetail},
    leads::{
        DraftDto, EvidenceDto, RejectionReasonDto, VerdictDto, invalid, parse_pagination,
        parse_verdicts, rejection_problem,
    },
    profiles::ProfileRefDto,
    response::{DataEnvelope, Response},
    state::AppState,
};

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct DraftLeadDto {
    pub id: String,
    pub domain: String,
    pub score: f64,
    /// Weighted confidence in percent; absent until a qualification agent answered validly.
    pub confidence: Option<f64>,
    /// One of `fiable`, `a_verifier`, `ecarte`; absent while the verdict is pending.
    pub verdict: Option<VerdictDto>,
    pub profile: ProfileRefDto,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct DraftContactDto {
    pub address: String,
    pub method: String,
    pub confidence: f64,
}

impl From<&ContactEmail> for DraftContactDto {
    fn from(contact: &ContactEmail) -> Self {
        Self {
            address: contact.address.as_str().to_owned(),
            method: contact.method.as_str().to_owned(),
            confidence: contact.confidence.get(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct DraftEntryDto {
    pub assessment_id: String,
    pub lead: DraftLeadDto,
    pub draft: DraftDto,
    pub summary: String,
    pub verdict_narrative: String,
    pub approach_angle: String,
    pub evidence: Vec<EvidenceDto>,
    pub evidence_verified: bool,
    pub contact: Option<DraftContactDto>,
    #[schema(format = DateTime)]
    pub created_at: String,
}

impl TryFrom<&DraftEntry> for DraftEntryDto {
    type Error = ApiError;

    fn try_from(entry: &DraftEntry) -> Result<Self, Self::Error> {
        let record = &entry.record;
        let assessment = &record.assessment;
        let draft = assessment.draft.as_ref().ok_or(ApiError::Internal)?;
        Ok(Self {
            assessment_id: assessment.id.to_string(),
            lead: DraftLeadDto {
                id: record.lead.id.to_string(),
                domain: record.lead.domain.as_str().to_owned(),
                score: record.lead.score.get(),
                confidence: record.lead.confidence,
                verdict: record.lead.verdict.map(VerdictDto::from),
                profile: ProfileRefDto {
                    id: record.profile_id.to_string(),
                    name: record.profile_name.as_str().to_owned(),
                },
            },
            draft: DraftDto::new(draft, assessment.draft_review.as_ref()),
            summary: assessment.summary.clone(),
            verdict_narrative: assessment.verdict_narrative.clone(),
            approach_angle: assessment.approach_angle.clone(),
            evidence: assessment
                .evidence
                .iter()
                .map(|evidence| EvidenceDto {
                    quote: evidence.quote.clone(),
                    provenance: evidence.provenance.as_str().to_owned(),
                })
                .collect(),
            evidence_verified: assessment.evidence_verified,
            contact: entry.contact.as_ref().map(DraftContactDto::from),
            created_at: assessment.created_at.to_rfc3339(),
        })
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListDraftsParams {
    /// Only drafts of this profile.
    profile_id: Option<String>,
    /// Comma-separated verdicts to keep: fiable, a_verifier, ecarte, en_attente. All drafts when absent.
    verdict: Option<String>,
    /// Page number, starting at 1.
    #[param(value_type = Option<u64>, minimum = 1)]
    page: Option<String>,
    /// Page size, between 1 and 100. Defaults to 20.
    #[param(value_type = Option<u64>, minimum = 1, maximum = 100)]
    per_page: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EditDraftRequest {
    /// New subject, 1 to 200 characters.
    pub subject: Option<String>,
    /// New body, 1 to 10000 characters.
    pub body: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct RejectDraftRequest {
    /// Required: one of `hors_cible`, `deja_client`, `concurrent`, `editeur_integrateur`, `hebergeur_agence`, `mauvaise_donnee`, `autre`.
    #[schema(value_type = Option<RejectionReasonDto>)]
    pub reason: Option<String>,
    /// Free text, at most 500 characters. Required when the reason is `autre`.
    pub note: Option<String>,
}

fn assessment_id(raw: &str) -> Result<AssessmentId, ApiError> {
    raw.parse()
        .map_err(|_| invalid("assessment_id", "must be a UUID"))
}

fn rejected(problems: Vec<DraftProblem>) -> ApiError {
    ApiError::Rejected(
        problems
            .into_iter()
            .map(|problem| ErrorDetail {
                path: problem.field.to_owned(),
                message: problem.message,
            })
            .collect(),
    )
}

fn actor(user: Option<Extension<AuthenticatedUser>>) -> Result<lodestone_core::UserId, ApiError> {
    match user {
        Some(Extension(AuthenticatedUser(user))) => Ok(user),
        None => Err(ApiError::Forbidden),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/drafts",
    tag = "drafts",
    security(("bearer_auth" = [])),
    params(ListDraftsParams),
    responses(
        (status = 200, description = "Pending drafts of qualified, non-suppressed leads, highest lead confidence first", body = DataEnvelope<Vec<DraftEntryDto>>),
        (status = 400, description = "A filter or pagination parameter is invalid"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 404, description = "Unknown profile"),
    )
)]
pub async fn list_drafts(
    State(state): State<AppState>,
    Query(params): Query<ListDraftsParams>,
) -> Result<Response<DraftEntryDto>, ApiError> {
    let profile_id = params
        .profile_id
        .as_deref()
        .map(|raw| {
            raw.parse::<ProfileId>()
                .map_err(|_| invalid("profile_id", "must be a UUID"))
        })
        .transpose()?;
    let verdicts = parse_verdicts(params.verdict.as_deref())?;
    let pagination = parse_pagination(params.page.as_deref(), params.per_page.as_deref())?;
    let per_page = pagination.per_page();
    let current_page = pagination.offset() / per_page + 1;

    let queue = state
        .usecase
        .list_pending_drafts(
            profile_id,
            &verdicts,
            u32::try_from(per_page).unwrap_or(u32::MAX),
            pagination.offset(),
        )
        .await?;
    let items: Vec<DraftEntryDto> = queue
        .items
        .iter()
        .map(DraftEntryDto::try_from)
        .collect::<Result<_, _>>()?;
    let meta = PaginationMetadata::new(per_page, current_page, Some(queue.total), items.is_empty());
    Ok(Response::Paginated(Page::new(items, meta)))
}

#[utoipa::path(
    get,
    path = "/api/v1/drafts/{assessment_id}",
    tag = "drafts",
    security(("bearer_auth" = [])),
    params(("assessment_id" = String, Path, description = "Assessment identifier (UUID)")),
    responses(
        (status = 200, description = "The draft in any review status", body = DataEnvelope<DraftEntryDto>),
        (status = 400, description = "Malformed assessment identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 404, description = "Unknown draft, or the lead domain is suppressed"),
    )
)]
pub async fn get_draft(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Response<DraftEntryDto>, ApiError> {
    let entry = state.usecase.get_draft(assessment_id(&raw)?).await?;
    Ok(Response::OK(DraftEntryDto::try_from(&entry)?))
}

#[utoipa::path(
    patch,
    path = "/api/v1/drafts/{assessment_id}",
    tag = "drafts",
    security(("bearer_auth" = [])),
    params(("assessment_id" = String, Path, description = "Assessment identifier (UUID)")),
    request_body = EditDraftRequest,
    responses(
        (status = 200, description = "The edited draft; the model text is kept as original_subject and original_body", body = DataEnvelope<DraftEntryDto>),
        (status = 400, description = "Malformed assessment identifier or body"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown draft, or the lead domain is suppressed"),
        (status = 409, description = "The draft is no longer pending"),
        (status = 422, description = "Subject or body is out of bounds, or both are missing"),
    )
)]
pub async fn edit_draft(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
    body: Result<Json<EditDraftRequest>, JsonRejection>,
) -> Result<Response<DraftEntryDto>, ApiError> {
    actor(user)?;
    let id = assessment_id(&raw)?;
    let Json(request) = body.map_err(|rejection| invalid("body", &rejection.body_text()))?;
    let edit = DraftEdit::new(request.subject, request.body).map_err(rejected)?;
    let entry = state.usecase.edit_draft(id, edit).await?;
    Ok(Response::OK(DraftEntryDto::try_from(&entry)?))
}

#[utoipa::path(
    post,
    path = "/api/v1/drafts/{assessment_id}/approve",
    tag = "drafts",
    security(("bearer_auth" = [])),
    params(("assessment_id" = String, Path, description = "Assessment identifier (UUID)")),
    responses(
        (status = 200, description = "The draft is marked approved and its lead validated; nothing is sent", body = DataEnvelope<DraftEntryDto>),
        (status = 400, description = "Malformed assessment identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown or superseded draft, or the lead domain is suppressed"),
        (status = 409, description = "The draft is not pending, or its lead is not qualified"),
    )
)]
pub async fn approve_draft(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
) -> Result<Response<DraftEntryDto>, ApiError> {
    let reviewer = actor(user)?;
    let entry = state
        .usecase
        .approve_draft(assessment_id(&raw)?, reviewer)
        .await?;
    Ok(Response::OK(DraftEntryDto::try_from(&entry)?))
}

#[utoipa::path(
    post,
    path = "/api/v1/drafts/{assessment_id}/reject",
    tag = "drafts",
    security(("bearer_auth" = [])),
    params(("assessment_id" = String, Path, description = "Assessment identifier (UUID)")),
    request_body(content = RejectDraftRequest, description = "Rejection reason, required"),
    responses(
        (status = 200, description = "The draft is marked rejected and its lead rejected", body = DataEnvelope<DraftEntryDto>),
        (status = 400, description = "Malformed assessment identifier or body"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown or superseded draft, or the lead domain is suppressed"),
        (status = 409, description = "The draft is not pending, or its lead is not qualified"),
        (status = 422, description = "The reason is missing or unknown, or the note is too long or missing for `autre`"),
    )
)]
pub async fn reject_draft(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
    body: Option<Json<RejectDraftRequest>>,
) -> Result<Response<DraftEntryDto>, ApiError> {
    let reviewer = actor(user)?;
    let id = assessment_id(&raw)?;
    let request = body.map(|Json(request)| request).unwrap_or_default();
    let rejection = Rejection::parse(request.reason.as_deref(), request.note.as_deref())
        .map_err(rejection_problem)?;
    let entry = state.usecase.reject_draft(id, reviewer, rejection).await?;
    Ok(Response::OK(DraftEntryDto::try_from(&entry)?))
}
