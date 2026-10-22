use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use chrono::Utc;
use lodestone_core::{
    DropReason, DropReview, DroppedCandidate, DroppedFilter, DroppedId, ProfileId,
    definition::AgentId, dropped::ReviewedDrop,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    auth::AuthenticatedUser,
    errors::ApiError,
    leads::{invalid, parse_number},
    response::{DataEnvelope, Response},
    state::AppState,
};

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DropReasonDto {
    NoKeyword,
    Exclusion,
    FilterRemote,
    FilterCountry,
    FilterContract,
}

impl From<DropReason> for DropReasonDto {
    fn from(reason: DropReason) -> Self {
        match reason {
            DropReason::NoKeyword => Self::NoKeyword,
            DropReason::Exclusion => Self::Exclusion,
            DropReason::FilterRemote => Self::FilterRemote,
            DropReason::FilterCountry => Self::FilterCountry,
            DropReason::FilterContract => Self::FilterContract,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DropReviewDto {
    RightlyDropped,
    ShouldHaveKept,
}

impl From<DropReview> for DropReviewDto {
    fn from(review: DropReview) -> Self {
        match review {
            DropReview::RightlyDropped => Self::RightlyDropped,
            DropReview::ShouldHaveKept => Self::ShouldHaveKept,
        }
    }
}

impl From<DropReviewDto> for DropReview {
    fn from(review: DropReviewDto) -> Self {
        match review {
            DropReviewDto::RightlyDropped => Self::RightlyDropped,
            DropReviewDto::ShouldHaveKept => Self::ShouldHaveKept,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct DroppedCandidateDto {
    pub id: String,
    pub profile_id: String,
    /// Agent whose keyword triage dropped the candidate; absent for the company job offers read without an agent.
    pub agent: Option<String>,
    pub source: String,
    /// Signal kind the candidate would have produced, for example `job_offer` or `public_tender`.
    pub kind: String,
    pub domain: String,
    pub title: String,
    /// Passages of the text that show why the candidate was dropped: around the excluded words for an exclusion, around the matched keywords for a filter, the opening of the text otherwise. At most 3.
    pub excerpts: Vec<String>,
    /// Address of the offer or notice.
    pub provenance: String,
    #[schema(format = DateTime)]
    pub observed_at: String,
    /// Why the candidate was dropped: no keyword matched, an exclusion matched, or a remote, country or contract filter failed.
    pub reason: DropReasonDto,
    /// Keywords of the agent found in the candidate; empty when none matched.
    pub matched: Vec<String>,
    /// Exclusions of the agent found in the candidate; empty unless the reason is `exclusion`.
    pub excluded: Vec<String>,
    /// Last triage run that dropped the candidate.
    #[schema(format = DateTime)]
    pub dropped_at: String,
    /// The operator's verdict; absent until the candidate is reviewed.
    pub review: Option<DropReviewDto>,
    #[schema(format = DateTime)]
    pub reviewed_at: Option<String>,
}

impl From<&DroppedCandidate> for DroppedCandidateDto {
    fn from(candidate: &DroppedCandidate) -> Self {
        Self {
            id: candidate.id.to_string(),
            profile_id: candidate.profile_id.to_string(),
            agent: candidate
                .agent
                .as_ref()
                .map(|agent| agent.as_str().to_owned()),
            source: candidate.source.as_str().to_owned(),
            kind: candidate.kind.as_str().to_owned(),
            domain: candidate.domain.as_str().to_owned(),
            title: candidate.title.clone(),
            excerpts: candidate.excerpts.clone(),
            provenance: candidate.provenance.as_str().to_owned(),
            observed_at: candidate.observed_at.to_rfc3339(),
            reason: DropReasonDto::from(candidate.reason),
            matched: candidate.matched.clone(),
            excluded: candidate.excluded.clone(),
            dropped_at: candidate.dropped_at.to_rfc3339(),
            review: candidate.review.map(DropReviewDto::from),
            reviewed_at: candidate.reviewed_at.map(|at| at.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ReviewedDropDto {
    pub candidate: DroppedCandidateDto,
    /// Lead of the candidate's domain after a `should_have_kept` review; absent when none exists, for example for a company job offer whose domain has no lead.
    pub lead_id: Option<String>,
}

impl From<&ReviewedDrop> for ReviewedDropDto {
    fn from(reviewed: &ReviewedDrop) -> Self {
        Self {
            candidate: DroppedCandidateDto::from(&reviewed.candidate),
            lead_id: reviewed.lead_id.map(|id| id.to_string()),
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListDroppedParams {
    /// One of no_keyword, exclusion, filter_remote, filter_country, filter_contract.
    #[param(value_type = Option<DropReasonDto>)]
    reason: Option<String>,
    /// Only candidates dropped by this agent.
    agent: Option<String>,
    /// `true` for the reviewed candidates only, `false` for the pending ones only. Both when absent.
    #[param(value_type = Option<bool>)]
    reviewed: Option<String>,
    /// `recent` (default) lists the latest drops first; `random` draws a sample to review.
    order: Option<String>,
    /// Number of candidates, between 1 and 100. Defaults to 20.
    #[param(value_type = Option<u32>, minimum = 1, maximum = 100)]
    limit: Option<String>,
}

struct DroppedQuery {
    filter: DroppedFilter,
    limit: u32,
    random: bool,
}

impl ListDroppedParams {
    fn into_query(self) -> Result<DroppedQuery, ApiError> {
        let reason = self
            .reason
            .as_deref()
            .map(|raw| {
                DropReason::parse(raw).ok_or_else(|| {
                    invalid(
                        "reason",
                        "must be one of no_keyword, exclusion, filter_remote, filter_country, filter_contract",
                    )
                })
            })
            .transpose()?;
        let agent = self
            .agent
            .as_deref()
            .map(|raw| {
                let agent = AgentId::new(raw);
                if agent.is_slug() {
                    Ok(agent)
                } else {
                    Err(invalid("agent", "must be an agent id"))
                }
            })
            .transpose()?;
        let reviewed = self
            .reviewed
            .as_deref()
            .map(|raw| match raw {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(invalid("reviewed", "must be true or false")),
            })
            .transpose()?;
        let random = match self.order.as_deref() {
            None | Some("recent") => false,
            Some("random") => true,
            Some(_) => return Err(invalid("order", "must be recent or random")),
        };
        let limit = match self.limit.as_deref() {
            None => DEFAULT_LIMIT,
            Some(raw) => parse_number::<u32>("limit", raw)?,
        };
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(invalid("limit", "must be between 1 and 100"));
        }
        Ok(DroppedQuery {
            filter: DroppedFilter {
                reason,
                agent,
                reviewed,
            },
            limit,
            random,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReviewDroppedRequest {
    /// `rightly_dropped` confirms the drop; `should_have_kept` recovers the candidate as a signal of the agent and creates the lead when it does not exist.
    pub review: DropReviewDto,
}

#[utoipa::path(
    get,
    path = "/api/v1/profiles/{profile_id}/dropped",
    tag = "dropped",
    security(("bearer_auth" = [])),
    params(
        ("profile_id" = String, Path, description = "Profile identifier (UUID)"),
        ListDroppedParams,
    ),
    responses(
        (status = 200, description = "Candidates the keyword triage dropped, with the reason and the passages that explain it", body = DataEnvelope<Vec<DroppedCandidateDto>>),
        (status = 400, description = "A parameter is invalid"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown profile"),
    )
)]
pub async fn list_dropped(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(profile_id): Path<String>,
    Query(params): Query<ListDroppedParams>,
) -> Result<Response<Vec<DroppedCandidateDto>>, ApiError> {
    if user.is_none() {
        return Err(ApiError::Forbidden);
    }
    let profile_id: ProfileId = profile_id
        .parse()
        .map_err(|_| invalid("profile_id", "must be a UUID"))?;
    let query = params.into_query()?;
    let candidates = state
        .usecase
        .list_dropped(profile_id, &query.filter, query.limit, query.random)
        .await?;
    Ok(Response::OK(
        candidates.iter().map(DroppedCandidateDto::from).collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/dropped/{dropped_id}/review",
    tag = "dropped",
    security(("bearer_auth" = [])),
    params(("dropped_id" = String, Path, description = "Dropped candidate identifier (UUID)")),
    request_body = ReviewDroppedRequest,
    responses(
        (status = 200, description = "The reviewed candidate; a `should_have_kept` review also recorded it as a `recovered` signal of its agent at strength 0.5", body = DataEnvelope<ReviewedDropDto>),
        (status = 400, description = "Malformed identifier or unknown review"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown dropped candidate"),
        (status = 409, description = "The candidate is already reviewed, or its profile is inactive and the candidate cannot be recovered"),
    )
)]
pub async fn review_dropped(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(dropped_id): Path<String>,
    body: Result<Json<ReviewDroppedRequest>, JsonRejection>,
) -> Result<Response<ReviewedDropDto>, ApiError> {
    let Some(Extension(AuthenticatedUser(actor))) = user else {
        return Err(ApiError::Forbidden);
    };
    let id: DroppedId = dropped_id
        .parse()
        .map_err(|_| invalid("dropped_id", "must be a UUID"))?;
    let Json(request) = body.map_err(|rejection| invalid("review", &rejection.body_text()))?;
    let reviewed = state
        .usecase
        .review_dropped(id, request.review.into(), actor, Utc::now())
        .await?;
    Ok(Response::OK(ReviewedDropDto::from(&reviewed)))
}
