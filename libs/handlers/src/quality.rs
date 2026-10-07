use axum::{
    Extension,
    extract::{Query, State},
};
use chrono::{DateTime, Utc};
use lodestone_core::{
    MIN_LABELLED_LEADS, ProfileId, ProfileQuality, QualityReport, ReasonCount, Suggestion,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    auth::AuthenticatedUser,
    errors::ApiError,
    leads::{RejectionReasonDto, VerdictDto, invalid},
    response::{DataEnvelope, Response},
    state::AppState,
};

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct DecisionCountsDto {
    pub validated: u64,
    pub rejected: u64,
    pub drafts_approved: u64,
    pub drafts_rejected: u64,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct VerdictPrecisionDto {
    /// One of `fiable`, `a_verifier`, `ecarte`; absent for leads decided before any verdict.
    pub verdict: Option<VerdictDto>,
    pub validated: u64,
    pub rejected: u64,
    /// Number of decided leads, validated plus rejected.
    pub n: u64,
    /// Validated over decided, between 0 and 1; absent without decisions.
    pub precision: Option<f64>,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct DiscardedShareDto {
    pub discarded: u64,
    /// Leads that received a verdict in the period.
    pub total: u64,
    /// Between 0 and 1; absent without verdicts.
    pub share: Option<f64>,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ConfidenceBucketDto {
    /// Inclusive lower bound, in percent.
    pub from: u8,
    /// Upper bound in percent, exclusive except for the last bucket.
    pub to: u8,
    pub validated: u64,
    pub rejected: u64,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct AgentQualityDto {
    pub agent_id: String,
    /// Name of the agent in the profile revisions; the id when no revision declares it.
    pub name: String,
    /// Mean score, between 0 and 1, of the validated leads the agent answered validly.
    pub mean_validated: Option<f64>,
    pub mean_rejected: Option<f64>,
    /// Mean for validated minus mean for rejected leads; absent when one side is empty.
    pub separation: Option<f64>,
    pub validated: u64,
    pub rejected: u64,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct VersionPrecisionDto {
    /// One of `qualification`, `scoring`, `aggregator`.
    pub kind: String,
    /// Absent for the aggregator.
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub model: String,
    /// `qualification-agent@<hash>`, `scoring-agent@<hash>` or `aggregator@<hash>`.
    pub prompt_version: String,
    /// True for the runs of the escalation provider, counted apart from the original runs.
    pub escalated: bool,
    pub validated: u64,
    pub rejected: u64,
    /// Number of decided leads, validated plus rejected.
    pub n: u64,
    /// Validated over decided, between 0 and 1.
    pub precision: Option<f64>,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ReasonCountDto {
    #[schema(value_type = RejectionReasonDto)]
    pub reason: String,
    pub count: u64,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ExclusionCountDto {
    pub agent_id: String,
    pub agent_name: String,
    pub question_id: String,
    /// Value of the excluding option.
    pub option: String,
    pub label: String,
    /// Currently discarded leads in the period excluded by this option.
    pub count: u64,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ExclusionCandidateDto {
    pub lead_id: String,
    pub domain: String,
    /// Legal name from the company dossier; absent when unknown.
    pub legal_name: Option<String>,
    /// One of `deja_client`, `concurrent`, `editeur_integrateur`, `hebergeur_agence`.
    #[schema(value_type = RejectionReasonDto)]
    pub reason: String,
    #[schema(format = DateTime)]
    pub decided_at: String,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ThresholdsDto {
    pub reliable_from: u8,
    pub discard_below: u8,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct SuggestionDto {
    /// False while fewer than `required` decided leads carry a confidence.
    pub ready: bool,
    pub labelled: u64,
    pub required: u64,
    /// Step-5 value maximising the F1 of validated leads; absent when no lead was validated.
    pub reliable_from: Option<u8>,
    /// Largest step-5 value under which at most 5 % of the decided leads were validated.
    pub discard_below: Option<u8>,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ProfileQualityDto {
    pub profile_id: String,
    pub name: String,
    pub validated: u64,
    pub rejected: u64,
    pub precision: Option<f64>,
    /// Thresholds of the latest revision. Suggestions are never applied automatically.
    pub current: ThresholdsDto,
    pub suggestion: SuggestionDto,
    /// Reasons given for the rejected leads of the profile, most frequent first.
    pub rejection_reasons: Vec<ReasonCountDto>,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct QualityDto {
    pub decisions: DecisionCountsDto,
    pub verdicts: Vec<VerdictPrecisionDto>,
    pub discarded: DiscardedShareDto,
    /// Ten buckets of 10 %, from 0 to 100.
    pub confidence: Vec<ConfidenceBucketDto>,
    pub agents: Vec<AgentQualityDto>,
    /// Precision per agent, model and prompt version, then per aggregator model and prompt version.
    pub versions: Vec<VersionPrecisionDto>,
    pub profiles: Vec<ProfileQualityDto>,
    /// Discarded leads of the period per excluding option, most frequent first.
    pub exclusions: Vec<ExclusionCountDto>,
    /// Most recent leads rejected for a reason that can feed the Exclusions agent lists; the lists are never changed automatically.
    pub exclusion_candidates: Vec<ExclusionCandidateDto>,
}

impl From<&Suggestion> for SuggestionDto {
    fn from(suggestion: &Suggestion) -> Self {
        match *suggestion {
            Suggestion::Insufficient { labelled, required } => Self {
                ready: false,
                labelled,
                required,
                reliable_from: None,
                discard_below: None,
            },
            Suggestion::Ready {
                labelled,
                reliable_from,
                discard_below,
            } => Self {
                ready: true,
                labelled,
                required: MIN_LABELLED_LEADS,
                reliable_from,
                discard_below,
            },
        }
    }
}

impl From<&ProfileQuality> for ProfileQualityDto {
    fn from(profile: &ProfileQuality) -> Self {
        let mut counts: Vec<&ReasonCount> = profile.reasons.iter().collect();
        counts.sort_by(|a, b| b.count.cmp(&a.count).then(a.reason.cmp(&b.reason)));
        let reasons = counts
            .into_iter()
            .map(|row| ReasonCountDto {
                reason: row.reason.as_str().to_owned(),
                count: row.count,
            })
            .collect();
        Self {
            profile_id: profile.profile_id.to_string(),
            name: profile.name.clone(),
            validated: profile.validated,
            rejected: profile.rejected,
            precision: profile.precision,
            current: ThresholdsDto {
                reliable_from: profile.current.reliable_from,
                discard_below: profile.current.discard_below,
            },
            suggestion: SuggestionDto::from(&profile.suggestion),
            rejection_reasons: reasons,
        }
    }
}

impl From<&QualityReport> for QualityDto {
    fn from(report: &QualityReport) -> Self {
        Self {
            decisions: DecisionCountsDto {
                validated: report.decisions.validated,
                rejected: report.decisions.rejected,
                drafts_approved: report.decisions.drafts_approved,
                drafts_rejected: report.decisions.drafts_rejected,
            },
            verdicts: report
                .verdicts
                .iter()
                .map(|row| VerdictPrecisionDto {
                    verdict: row.verdict.map(VerdictDto::from),
                    validated: row.validated,
                    rejected: row.rejected,
                    n: row.decided(),
                    precision: row.precision,
                })
                .collect(),
            discarded: DiscardedShareDto {
                discarded: report.discarded.discarded,
                total: report.discarded.total,
                share: report.discarded.share,
            },
            confidence: report
                .confidence
                .iter()
                .map(|bucket| ConfidenceBucketDto {
                    from: bucket.from,
                    to: bucket.to,
                    validated: bucket.validated,
                    rejected: bucket.rejected,
                })
                .collect(),
            agents: report
                .agents
                .iter()
                .map(|agent| AgentQualityDto {
                    agent_id: agent.agent_id.clone(),
                    name: agent.name.clone(),
                    mean_validated: agent.mean_validated,
                    mean_rejected: agent.mean_rejected,
                    separation: agent.separation,
                    validated: agent.validated,
                    rejected: agent.rejected,
                })
                .collect(),
            versions: report
                .versions
                .iter()
                .map(|row| VersionPrecisionDto {
                    kind: row.kind.as_str().to_owned(),
                    agent_id: row.agent_id.clone(),
                    agent_name: row.agent_name.clone(),
                    model: row.model.clone(),
                    prompt_version: row.prompt_version.clone(),
                    escalated: row.escalated,
                    validated: row.validated,
                    rejected: row.rejected,
                    n: row.decided(),
                    precision: row.precision,
                })
                .collect(),
            profiles: report
                .profiles
                .iter()
                .map(ProfileQualityDto::from)
                .collect(),
            exclusions: report
                .exclusions
                .iter()
                .map(|row| ExclusionCountDto {
                    agent_id: row.agent_id.clone(),
                    agent_name: row.agent_name.clone(),
                    question_id: row.question_id.clone(),
                    option: row.option.clone(),
                    label: row.label.clone(),
                    count: row.count,
                })
                .collect(),
            exclusion_candidates: report
                .exclusion_candidates
                .iter()
                .map(|row| ExclusionCandidateDto {
                    lead_id: row.lead_id.to_string(),
                    domain: row.domain.clone(),
                    legal_name: row.legal_name.clone(),
                    reason: row.reason.as_str().to_owned(),
                    decided_at: row.decided_at.to_rfc3339(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct QualityParams {
    /// Start of the period, RFC 3339. No lower bound when absent.
    from: Option<String>,
    /// End of the period, RFC 3339. No upper bound when absent.
    to: Option<String>,
    /// Only decisions on leads of this profile.
    profile_id: Option<String>,
}

fn instant(field: &str, raw: Option<&str>) -> Result<Option<DateTime<Utc>>, ApiError> {
    raw.map(|raw| {
        DateTime::parse_from_rfc3339(raw)
            .map(|parsed| parsed.with_timezone(&Utc))
            .map_err(|_| invalid(field, "must be an RFC 3339 timestamp"))
    })
    .transpose()
}

#[utoipa::path(
    get,
    path = "/api/v1/quality",
    tag = "quality",
    security(("bearer_auth" = [])),
    params(QualityParams),
    responses(
        (status = 200, description = "Quality of the verdicts measured against human decisions. Threshold suggestions are advice only", body = DataEnvelope<QualityDto>),
        (status = 400, description = "A parameter is invalid"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown profile"),
    )
)]
pub async fn quality(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Query(params): Query<QualityParams>,
) -> Result<Response<QualityDto>, ApiError> {
    if user.is_none() {
        return Err(ApiError::Forbidden);
    }
    let from = instant("from", params.from.as_deref())?;
    let to = instant("to", params.to.as_deref())?;
    if from.zip(to).is_some_and(|(from, to)| from > to) {
        return Err(invalid("from", "must not be after `to`"));
    }
    let profile_id = params
        .profile_id
        .as_deref()
        .map(|raw| {
            raw.parse::<ProfileId>()
                .map_err(|_| invalid("profile_id", "must be a UUID"))
        })
        .transpose()?;
    let report = state.usecase.quality_report(from, to, profile_id).await?;
    Ok(Response::OK(QualityDto::from(&report)))
}
