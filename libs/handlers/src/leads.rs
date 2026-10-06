use std::str::FromStr;

use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use lodestone_core::{
    Assessment, ContactEmail, DraftMessage, DraftReview, DraftStatus, Lead, LeadDetail, LeadFacts,
    LeadId, LeadQuery, LeadScoring, LeadSort, LeadStatus, LlmProviderRecord, Profile, ProfileId,
    QualificationResult, Rejection, RejectionProblem, ResultStatus, RunProvenance,
    SUMMARY_FAILED_AFTER, ScoreContribution, Signal, SignalKind, SummaryState, Verdict, VerdictCap,
    VerdictFilter, definition::QualificationAgent, signal_sentence,
};
use pagination::{MAX_PAGE, MAX_PER_PAGE, Page, PaginationMetadata, PaginationParams};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    auth::AuthenticatedUser,
    company::CompanyDto,
    errors::{ApiError, ErrorDetail},
    profiles::ProfileRefDto,
    response::Response,
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

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ToSchema)]
pub enum VerdictDto {
    #[serde(rename = "fiable")]
    Reliable,
    #[serde(rename = "a_verifier")]
    ToCheck,
    #[serde(rename = "ecarte")]
    Discarded,
}

impl From<Verdict> for VerdictDto {
    fn from(verdict: Verdict) -> Self {
        match verdict {
            Verdict::Reliable => Self::Reliable,
            Verdict::ToCheck => Self::ToCheck,
            Verdict::Discarded => Self::Discarded,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerdictReasonDto {
    SingleAngle,
    WeakEvidence,
    NonProduction,
}

impl From<VerdictCap> for VerdictReasonDto {
    fn from(cap: VerdictCap) -> Self {
        match cap {
            VerdictCap::SingleAngle => Self::SingleAngle,
            VerdictCap::WeakEvidence => Self::WeakEvidence,
            VerdictCap::NonProduction => Self::NonProduction,
        }
    }
}

fn percent(fraction: f64) -> f64 {
    (fraction * 1000.0).round() / 10.0
}

/// Components of the evidence strength, each in percent from 0 to 100.
#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct EvidenceComponentsDto {
    /// Number of signals: 1 - exp(-signals / 3).
    pub volume: f64,
    /// Distinct sources: min(1, sources / 2).
    pub diversity: f64,
    /// Share of production hosts among the hosts carrying signals; 50 when no signal names a host.
    pub environment: f64,
    /// Signal score against the profile reference score, capped at 100.
    pub strength: f64,
}

/// Strength of the evidence behind a lead, computed in code from its facts and signal score.
#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct EvidenceStrengthDto {
    /// Weighted mean of the components, in percent from 0 to 100.
    pub strength: f64,
    pub components: EvidenceComponentsDto,
    pub signal_count: u32,
    pub signal_score: f64,
    pub only_non_production: bool,
}

impl From<&LeadScoring> for EvidenceStrengthDto {
    fn from(scoring: &LeadScoring) -> Self {
        let evidence = &scoring.evidence;
        Self {
            strength: percent(evidence.score),
            components: EvidenceComponentsDto {
                volume: percent(evidence.components.volume),
                diversity: percent(evidence.components.diversity),
                environment: percent(evidence.components.environment),
                strength: percent(evidence.components.strength),
            },
            signal_count: count(evidence.signal_count),
            signal_score: evidence.signal_score,
            only_non_production: evidence.only_non_production,
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
    /// Weighted confidence in percent from 0 to 100; absent until a qualification agent answered validly.
    pub confidence: Option<f64>,
    /// One of `fiable`, `a_verifier`, `ecarte`; absent while the verdict is pending.
    pub verdict: Option<VerdictDto>,
    /// Opinion of the qualification agents in percent from 0 to 100, after calibration; absent until an agent answered validly.
    pub opinion: Option<f64>,
    /// Strength of the evidence behind the lead; absent until the lead has been scored.
    pub evidence: Option<EvidenceStrengthDto>,
    /// Rule that capped a confident lead at `a_verifier`: `single_angle`, `weak_evidence` or `non_production`; absent when no rule applied.
    pub verdict_reason: Option<VerdictReasonDto>,
    /// Summary written by the aggregator; absent until the lead has been summarised.
    pub summary: Option<String>,
    #[schema(format = DateTime)]
    pub qualified_at: Option<String>,
    #[schema(format = DateTime)]
    pub first_seen_at: String,
    #[schema(format = DateTime)]
    pub last_seen_at: String,
    #[schema(format = DateTime)]
    pub updated_at: String,
}

impl LeadDto {
    fn new(
        lead: &Lead,
        profile: &Profile,
        summary: Option<&str>,
        scoring: Option<&LeadScoring>,
    ) -> Self {
        Self {
            id: lead.id.to_string(),
            profile: ProfileRefDto::from(profile),
            domain: lead.domain.as_str().to_owned(),
            status: lead.status.into(),
            score: lead.score.get(),
            confidence: lead.confidence,
            verdict: lead.verdict.map(VerdictDto::from),
            opinion: scoring.and_then(|scoring| scoring.opinion).map(percent),
            evidence: scoring.map(EvidenceStrengthDto::from),
            verdict_reason: scoring
                .and_then(|scoring| scoring.cap)
                .map(VerdictReasonDto::from),
            summary: summary.map(str::to_owned),
            qualified_at: lead.qualified_at.map(|at| at.to_rfc3339()),
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
    /// The signal as a readable French sentence.
    pub sentence: String,
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
            sentence: signal_sentence(signal),
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
    /// Subject the aggregator proposed after the draft was edited; the edited draft is kept.
    pub suggested_subject: Option<String>,
    pub suggested_body: Option<String>,
    /// Set when the summary was rewritten after a human edit, so the suggestion is newer than the draft.
    #[schema(format = DateTime)]
    pub outdated_at: Option<String>,
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
            suggested_subject: review
                .suggestion
                .as_ref()
                .map(|suggestion| suggestion.subject.clone()),
            suggested_body: review
                .suggestion
                .as_ref()
                .map(|suggestion| suggestion.body.clone()),
            outdated_at: review.outdated_at.map(|at| at.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct GenerationDto {
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub frequency_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    pub repeat_penalty: Option<f64>,
    /// One of `none`, `low`, `medium`, `high`; absent when the provider sets none.
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ProvenanceDto {
    pub provider_id: Option<String>,
    /// Name of the provider when the result was produced; empty when none could be resolved.
    pub provider_name: String,
    pub model: String,
    /// Prompt version, `qualification-agent@<hash>` for an agent and `aggregator@<hash>` for the aggregator.
    pub prompt_version: String,
    /// Generation settings in effect when the result was produced.
    pub generation: GenerationDto,
    /// Number of samples the result was drawn from.
    pub samples: u32,
}

impl From<&RunProvenance> for ProvenanceDto {
    fn from(provenance: &RunProvenance) -> Self {
        let generation = &provenance.generation;
        Self {
            provider_id: provenance.provider_id.map(|id| id.to_string()),
            provider_name: provenance.provider_name.clone(),
            model: provenance.model.clone(),
            prompt_version: provenance.prompt_version.clone(),
            generation: GenerationDto {
                temperature: generation.temperature,
                max_tokens: generation.max_tokens,
                frequency_penalty: generation.frequency_penalty,
                presence_penalty: generation.presence_penalty,
                repeat_penalty: generation.repeat_penalty,
                reasoning_effort: generation
                    .reasoning_effort
                    .map(|effort| effort.as_str().to_owned()),
            },
            samples: provenance.samples,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct AssessmentDto {
    pub id: String,
    pub profile_revision: u32,
    pub summary: String,
    /// Why the verdict holds, written by the aggregator; empty on assessments of the former qualification job.
    pub verdict_narrative: String,
    pub approach_angle: String,
    pub evidence: Vec<EvidenceDto>,
    /// False when no quote could be checked verbatim against the signals. Quotes that failed the check are dropped, never listed.
    pub evidence_verified: bool,
    /// Why the synthesis is of uncertain quality, when it is: `repetition`, `too_long`, `restated_figures` (a percentage or the verdict restated with a number) or `dropped_quotes` (some quotes could not be verified and were left out).
    pub quality_warnings: Vec<String>,
    pub model: String,
    pub prompt_version: String,
    pub provenance: ProvenanceDto,
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
            quality_warnings: assessment
                .quality_warnings
                .iter()
                .map(|warning| warning.as_str().to_owned())
                .collect(),
            model: assessment.provenance.model.clone(),
            prompt_version: assessment.provenance.prompt_version.clone(),
            provenance: ProvenanceDto::from(&assessment.provenance),
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
pub struct ProbabilityDto {
    pub value: String,
    pub label: String,
    pub probability: f64,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct QualificationAnswerDto {
    pub question_id: String,
    pub wording: String,
    pub choice: String,
    pub choice_label: String,
    /// Sentence copied verbatim from the documents that justifies the choice. Empty for an unknown choice and for answers stored before the quote existed.
    pub evidence: String,
    /// Source of the first document containing the quote, when known.
    pub evidence_source: Option<String>,
    /// False when the answer was downgraded to the unknown option for lack of a verified quote.
    pub grounded: bool,
    /// Value the model chose before the downgrade.
    pub original_choice: Option<String>,
    pub original_choice_label: Option<String>,
    pub confidence: f64,
    pub probabilities: Vec<ProbabilityDto>,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct QualificationResultDto {
    pub agent_id: String,
    pub agent_name: String,
    /// What the agent evaluates, as written in the profile revision that produced the result.
    pub agent_description: Option<String>,
    /// Short summary written by the agent, in the profile language. Absent when the result is invalid or predates summaries.
    pub summary: Option<String>,
    pub provider_id: Option<String>,
    pub provider_name: Option<String>,
    pub model: String,
    pub provenance: ProvenanceDto,
    /// One of `valid`, `invalid`.
    pub status: String,
    /// Agent score from 0 to 1, absent when the result is invalid.
    pub agent_score: Option<f64>,
    pub answers: Vec<QualificationAnswerDto>,
    pub error: Option<String>,
    #[schema(format = DateTime)]
    pub created_at: String,
}

fn answers_of(
    result: &QualificationResult,
    agent: Option<&QualificationAgent>,
) -> Vec<QualificationAnswerDto> {
    let Some(agent) = agent else {
        return Vec::new();
    };
    agent
        .questions
        .iter()
        .filter_map(|question| {
            let answer = result.answers.get(&question.id)?;
            let choice = answer.get("choice")?.as_str()?;
            let probabilities = answer.get("probabilities")?.as_object()?;
            let label_of = |value: &str| {
                question
                    .options
                    .iter()
                    .find(|option| option.value == value)
                    .map_or_else(|| value.to_owned(), |option| option.label.clone())
            };
            let original_choice = answer
                .get("original_choice")
                .and_then(serde_json::Value::as_str);
            Some(QualificationAnswerDto {
                question_id: question.id.clone(),
                wording: question.wording.clone(),
                choice: choice.to_owned(),
                choice_label: label_of(choice),
                evidence: answer
                    .get("evidence")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                evidence_source: answer
                    .get("evidence_source")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                grounded: answer
                    .get("grounded")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                original_choice: original_choice.map(str::to_owned),
                original_choice_label: original_choice.map(label_of),
                confidence: answer
                    .get("confidence")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or_default(),
                probabilities: question
                    .options
                    .iter()
                    .map(|option| ProbabilityDto {
                        value: option.value.clone(),
                        label: option.label.clone(),
                        probability: probabilities
                            .get(&option.value)
                            .and_then(serde_json::Value::as_f64)
                            .unwrap_or_default(),
                    })
                    .collect(),
            })
        })
        .collect()
}

impl QualificationResultDto {
    fn new(
        result: &QualificationResult,
        agents: &[QualificationAgent],
        providers: &[LlmProviderRecord],
    ) -> Self {
        let agent = agents.iter().find(|agent| agent.id == result.agent_id);
        let provider = result
            .provenance
            .provider_id
            .and_then(|id| providers.iter().find(|record| record.id == id));
        Self {
            agent_id: result.agent_id.as_str().to_owned(),
            agent_name: agent.map_or_else(
                || result.agent_id.as_str().to_owned(),
                |agent| agent.name.clone(),
            ),
            agent_description: agent.map(|agent| agent.description.clone()),
            summary: result.summary.clone(),
            provider_id: result.provenance.provider_id.map(|id| id.to_string()),
            provider_name: provider.map(|record| record.name.clone()).or_else(|| {
                Some(result.provenance.provider_name.clone()).filter(|name| !name.is_empty())
            }),
            model: result.provenance.model.clone(),
            provenance: ProvenanceDto::from(&result.provenance),
            status: result.status.as_str().to_owned(),
            agent_score: match result.status {
                ResultStatus::Valid => result.agent_score,
                ResultStatus::Invalid => None,
            },
            answers: answers_of(result, agent),
            error: result.error.clone(),
            created_at: result.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct SummaryStateDto {
    /// Consecutive failed attempts of the aggregator on this lead.
    pub attempts: u32,
    /// True from 5 consecutive failures; the aggregator then retries every 6 hours.
    pub failed: bool,
    /// Short reason of the last failure, in French.
    pub error: Option<String>,
    #[schema(format = DateTime)]
    pub next_attempt_at: Option<String>,
}

impl SummaryStateDto {
    fn new(state: &SummaryState) -> Option<Self> {
        (state.attempts > 0).then(|| Self {
            attempts: state.attempts,
            failed: state.attempts >= SUMMARY_FAILED_AFTER,
            error: state.error.clone(),
            next_attempt_at: state.next_attempt_at.map(|at| at.to_rfc3339()),
        })
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct HostFactDto {
    pub host: String,
    /// One of production, development, staging, test, preproduction, demo.
    pub environment: String,
    pub signals: u32,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct CountryDto {
    /// ISO 3166-1 alpha-2 code.
    pub code: String,
    /// English name; the interface localises it from the code.
    pub name: String,
}

/// Facts computed on read from the lead, its signals and the hosts already known, with no external call.
#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct LeadFactsDto {
    /// Hosts carrying at least one signal, with their environment.
    pub hosts: Vec<HostFactDto>,
    pub distinct_hosts: u32,
    pub production_hosts: u32,
    /// Share of production hosts among the hosts carrying signals, from 0 to 1; null when no signal names a host.
    pub production_share: Option<f64>,
    /// True when hosts carry signals and none is a production host.
    pub only_non_production: bool,
    /// Distinct subdomains seen for the domain, from signals and candidate hosts, apex excluded.
    pub distinct_subdomains: u32,
    pub signal_count: u32,
    pub source_count: u32,
    pub sources: Vec<String>,
    #[schema(format = DateTime)]
    pub first_seen_at: String,
    #[schema(format = DateTime)]
    pub last_seen_at: String,
    /// Head office country when official data exists, else deduced from the domain extension; null for a generic extension without official data.
    pub probable_country: Option<CountryDto>,
    /// Basis of the probable country: `head_office` (official register) or `tld` (domain extension).
    pub country_basis: Option<String>,
    /// Official register data of the company, null until a dossier with official data exists.
    pub official: Option<OfficialFactsDto>,
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct OfficialFactsDto {
    pub siren: String,
    pub legal_name: String,
    pub naf_code: Option<String>,
    pub naf_label: Option<String>,
    pub headcount_range: Option<String>,
    pub head_office_country: Option<String>,
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

impl From<&LeadFacts> for LeadFactsDto {
    fn from(facts: &LeadFacts) -> Self {
        Self {
            hosts: facts
                .hosts
                .iter()
                .map(|fact| HostFactDto {
                    host: fact.host.to_string(),
                    environment: fact.environment.as_str().to_owned(),
                    signals: count(fact.signals),
                })
                .collect(),
            distinct_hosts: count(facts.distinct_hosts()),
            production_hosts: count(facts.production_hosts()),
            production_share: facts.production_share(),
            only_non_production: facts.only_non_production(),
            distinct_subdomains: count(facts.distinct_subdomains),
            signal_count: count(facts.signal_count),
            source_count: count(facts.sources.len()),
            sources: facts
                .sources
                .iter()
                .map(|source| source.as_str().to_owned())
                .collect(),
            first_seen_at: facts.first_seen_at.to_rfc3339(),
            last_seen_at: facts.last_seen_at.to_rfc3339(),
            probable_country: facts.probable_country.map(|country| CountryDto {
                code: country.code.to_owned(),
                name: country.name.to_owned(),
            }),
            country_basis: facts.country_basis.map(|basis| basis.as_str().to_owned()),
            official: facts
                .company
                .as_ref()
                .and_then(|company| company.official.as_ref())
                .map(|official| OfficialFactsDto {
                    siren: official.siren.clone(),
                    legal_name: official.legal_name.clone(),
                    naf_code: official.naf_code.clone(),
                    naf_label: official.naf_label.clone(),
                    headcount_range: official.headcount_range.clone(),
                    head_office_country: official.head_office_country.clone(),
                }),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct LeadDetailDto {
    pub lead: LeadDto,
    pub facts: LeadFactsDto,
    pub score_breakdown: Vec<ScoreContributionDto>,
    pub signals: Vec<SignalDto>,
    pub contacts: Vec<ContactDto>,
    pub assessments: Vec<AssessmentDto>,
    /// Latest result of every qualification agent of the revision last used, in profile order.
    pub qualification_results: Vec<QualificationResultDto>,
    /// Present while the summary of the lead failed at least once since the agents last answered.
    pub summary_state: Option<SummaryStateDto>,
    /// Company dossier of the lead domain; null until the company research listener has seen the domain.
    pub company: Option<CompanyDto>,
}

fn ordered_results(
    detail: &LeadDetail,
    providers: &[LlmProviderRecord],
) -> Vec<QualificationResultDto> {
    let position = |result: &QualificationResult| {
        detail
            .qualification_agents
            .iter()
            .position(|agent| agent.id == result.agent_id)
            .unwrap_or(usize::MAX)
    };
    let mut results: Vec<&QualificationResult> = detail.qualification_results.iter().collect();
    results.sort_by_key(|result| position(result));
    results
        .into_iter()
        .map(|result| QualificationResultDto::new(result, &detail.qualification_agents, providers))
        .collect()
}

impl LeadDetailDto {
    pub fn new(detail: &LeadDetail, providers: &[LlmProviderRecord]) -> Self {
        Self {
            summary_state: detail.summary_state.as_ref().and_then(SummaryStateDto::new),
            company: detail.company.as_ref().map(CompanyDto::from),
            qualification_results: ordered_results(detail, providers),
            lead: LeadDto::new(
                &detail.lead,
                &detail.profile,
                detail
                    .assessments
                    .first()
                    .map(|assessment| assessment.summary.as_str()),
                detail.scoring.as_ref(),
            ),
            score_breakdown: detail
                .lead
                .score_breakdown
                .iter()
                .map(ScoreContributionDto::from)
                .collect(),
            facts: LeadFactsDto::from(&detail.facts),
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
    /// Comma-separated verdicts to keep: fiable, a_verifier, ecarte, en_attente. All leads when absent.
    pub(crate) verdict: Option<String>,
    /// Minimum confidence in percent, inclusive. Leads without a confidence are left out.
    #[param(value_type = Option<f64>, minimum = 0, maximum = 100)]
    pub(crate) min_confidence: Option<String>,
    /// Minimum score, inclusive. Must be a non-negative number.
    #[param(value_type = Option<f64>, minimum = 0)]
    pub(crate) min_score: Option<String>,
    /// Case-insensitive substring of the lead domain.
    pub(crate) domain: Option<String>,
    /// Only leads having at least one signal of this kind.
    pub(crate) signal: Option<String>,
    /// One of confidence_desc (default), confidence_asc, score_desc, score_asc, last_seen_desc, first_seen_desc, domain_asc.
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

pub(crate) fn parse_verdicts(raw: Option<&str>) -> Result<Vec<VerdictFilter>, ApiError> {
    let mut verdicts = Vec::new();
    for part in raw.unwrap_or_default().split(',').map(str::trim) {
        if part.is_empty() {
            continue;
        }
        let filter = VerdictFilter::parse(part).ok_or_else(|| {
            invalid(
                "verdict",
                "must be a comma-separated list of fiable, a_verifier, ecarte, en_attente",
            )
        })?;
        if !verdicts.contains(&filter) {
            verdicts.push(filter);
        }
    }
    Ok(verdicts)
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

        let verdicts = parse_verdicts(self.verdict.as_deref())?;

        let min_confidence = self
            .min_confidence
            .as_deref()
            .map(|raw| {
                let value = parse_number::<f64>("min_confidence", raw)?;
                if value.is_finite() && (0.0..=100.0).contains(&value) {
                    Ok(value)
                } else {
                    Err(invalid("min_confidence", "must be between 0 and 100"))
                }
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
                    "must be one of confidence_desc, confidence_asc, score_desc, score_asc, last_seen_desc, first_seen_desc, domain_asc",
                )
            })?,
        };

        let pagination = parse_pagination(self.page.as_deref(), self.per_page.as_deref())?;

        Ok(LeadQuery {
            status,
            verdicts,
            min_confidence,
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
    /// Required when the status is `rejected`: one of `hors_cible`, `deja_client`, `concurrent`, `editeur_integrateur`, `hebergeur_agence`, `mauvaise_donnee`, `autre`.
    #[schema(value_type = Option<RejectionReasonDto>)]
    pub reason: Option<String>,
    /// Free text, at most 500 characters. Required when the reason is `autre`.
    pub note: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RejectionReasonDto {
    HorsCible,
    DejaClient,
    Concurrent,
    EditeurIntegrateur,
    HebergeurAgence,
    MauvaiseDonnee,
    Autre,
}

pub(crate) fn rejection_problem(problem: RejectionProblem) -> ApiError {
    ApiError::Rejected(vec![ErrorDetail {
        path: problem.field.to_owned(),
        message: problem.message,
    }])
}

pub(crate) fn lead_id(raw: &str) -> Result<LeadId, ApiError> {
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
        .map(|lead| {
            LeadDto::new(
                lead,
                &listing.profile,
                listing.summaries.get(&lead.id).map(String::as_str),
                listing.scorings.get(&lead.id),
            )
        })
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
    let providers = if detail.qualification_results.is_empty() {
        Vec::new()
    } else {
        state.usecase.list_llm_providers().await?
    };
    Ok(Response::OK(LeadDetailDto::new(&detail, &providers)))
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
        (status = 422, description = "Rejecting without a valid reason, or a note that is too long or missing for `autre`"),
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

    let target = LeadStatus::from(request.status);
    let rejection = if target == LeadStatus::Rejected {
        Some(
            Rejection::parse(request.reason.as_deref(), request.note.as_deref())
                .map_err(rejection_problem)?,
        )
    } else {
        None
    };
    let lead = state
        .usecase
        .change_lead_status(id, target, actor, rejection)
        .await?;
    let detail = state.usecase.lead_detail(lead.id).await?;
    let summary = detail
        .assessments
        .first()
        .map(|assessment| assessment.summary.as_str());
    Ok(Response::OK(LeadDto::new(
        &lead,
        &detail.profile,
        summary,
        detail.scoring.as_ref(),
    )))
}

#[utoipa::path(
    post,
    path = "/api/v1/leads/{lead_id}/summary/retry",
    tag = "leads",
    security(("bearer_auth" = [])),
    params(("lead_id" = String, Path, description = "Lead identifier (UUID)")),
    responses(
        (status = 200, description = "The failure counter is reset; the aggregator tries again on its next loop", body = crate::response::DataEnvelope<LeadDto>),
        (status = 400, description = "Malformed lead identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown lead, or its domain is suppressed"),
        (status = 409, description = "The lead has not been qualified yet"),
    )
)]
pub async fn retry_lead_summary(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(lead_id_raw): Path<String>,
) -> Result<Response<LeadDto>, ApiError> {
    if user.is_none() {
        return Err(ApiError::Forbidden);
    }
    let lead = state
        .usecase
        .retry_lead_summary(lead_id(&lead_id_raw)?)
        .await?;
    let detail = state.usecase.lead_detail(lead.id).await?;
    let summary = detail
        .assessments
        .first()
        .map(|assessment| assessment.summary.as_str());
    Ok(Response::OK(LeadDto::new(
        &lead,
        &detail.profile,
        summary,
        detail.scoring.as_ref(),
    )))
}
