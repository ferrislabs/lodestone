use chrono::{DateTime, Utc};
use common::CoreError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Assessment, AssessmentId, DraftMessage, DraftReview, DraftStatus, Evidence, LeadId, Provenance,
    QualityWarning, RevisionNumber, UserId,
};

pub const ASSESSMENT_COLUMNS: &str = "assessments.id, assessments.lead_id, assessments.profile_revision, assessments.summary, assessments.verdict_narrative, assessments.approach_angle, assessments.evidence, assessments.evidence_verified, assessments.quality_warnings, assessments.model, assessments.prompt_version, assessments.draft_subject, assessments.draft_body, assessments.draft_status, assessments.draft_reviewed_by, assessments.draft_reviewed_at, assessments.draft_edited_at, assessments.draft_rejection_reason, assessments.draft_original_subject, assessments.draft_original_body, assessments.draft_suggested_subject, assessments.draft_suggested_body, assessments.draft_outdated_at, assessments.created_at";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AssessmentRow {
    pub id: Uuid,
    pub lead_id: Uuid,
    pub profile_revision: i32,
    pub summary: String,
    pub verdict_narrative: String,
    pub approach_angle: String,
    pub evidence: serde_json::Value,
    pub evidence_verified: bool,
    pub quality_warnings: Vec<String>,
    pub model: String,
    pub prompt_version: String,
    pub draft_subject: Option<String>,
    pub draft_body: Option<String>,
    pub draft_status: Option<String>,
    pub draft_reviewed_by: Option<Uuid>,
    pub draft_reviewed_at: Option<DateTime<Utc>>,
    pub draft_edited_at: Option<DateTime<Utc>>,
    pub draft_rejection_reason: Option<String>,
    pub draft_original_subject: Option<String>,
    pub draft_original_body: Option<String>,
    pub draft_suggested_subject: Option<String>,
    pub draft_suggested_body: Option<String>,
    pub draft_outdated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
struct EvidenceDoc {
    quote: String,
    provenance: String,
}

pub fn evidence_to_json(evidence: &[Evidence]) -> Result<serde_json::Value, CoreError> {
    let docs: Vec<EvidenceDoc> = evidence
        .iter()
        .map(|item| EvidenceDoc {
            quote: item.quote.clone(),
            provenance: item.provenance.as_str().to_owned(),
        })
        .collect();
    serde_json::to_value(docs)
        .map_err(|_| CoreError::Internal("assessment evidence cannot be serialized".to_owned()))
}

fn evidence_from_json(value: serde_json::Value) -> Result<Vec<Evidence>, CoreError> {
    let docs: Vec<EvidenceDoc> = serde_json::from_value(value)
        .map_err(|_| CoreError::Internal("stored assessment evidence is invalid".to_owned()))?;
    docs.into_iter()
        .map(|doc| {
            let provenance = Provenance::parse(&doc.provenance).map_err(|_| {
                CoreError::Internal("stored assessment evidence is invalid".to_owned())
            })?;
            Ok(Evidence {
                quote: doc.quote,
                provenance,
            })
        })
        .collect()
}

impl TryFrom<AssessmentRow> for Assessment {
    type Error = CoreError;

    fn try_from(row: AssessmentRow) -> Result<Self, Self::Error> {
        let profile_revision = u32::try_from(row.profile_revision)
            .ok()
            .and_then(RevisionNumber::new)
            .ok_or_else(|| {
                CoreError::Internal("stored assessment revision is invalid".to_owned())
            })?;
        let draft_review = match (&row.draft_subject, row.draft_status.as_deref()) {
            (Some(_), Some(raw)) => Some(DraftReview {
                status: DraftStatus::parse(raw).ok_or_else(|| {
                    CoreError::Internal("stored draft status is invalid".to_owned())
                })?,
                original: match (row.draft_original_subject, row.draft_original_body) {
                    (Some(subject), Some(body)) => Some(DraftMessage { subject, body }),
                    _ => None,
                },
                edited_at: row.draft_edited_at,
                reviewed_by: row.draft_reviewed_by.map(UserId),
                reviewed_at: row.draft_reviewed_at,
                rejection_reason: row.draft_rejection_reason,
                suggestion: match (row.draft_suggested_subject, row.draft_suggested_body) {
                    (Some(subject), Some(body)) => Some(DraftMessage { subject, body }),
                    _ => None,
                },
                outdated_at: row.draft_outdated_at,
            }),
            (Some(_), None) => {
                return Err(CoreError::Internal("stored draft has no status".to_owned()));
            }
            (None, _) => None,
        };
        let quality_warnings = row
            .quality_warnings
            .iter()
            .map(|raw| {
                QualityWarning::parse(raw).ok_or_else(|| {
                    CoreError::Internal("stored assessment quality warning is invalid".to_owned())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            id: AssessmentId(row.id),
            lead_id: LeadId(row.lead_id),
            profile_revision,
            summary: row.summary,
            verdict_narrative: row.verdict_narrative,
            approach_angle: row.approach_angle,
            evidence: evidence_from_json(row.evidence)?,
            evidence_verified: row.evidence_verified,
            quality_warnings,
            model: row.model,
            prompt_version: row.prompt_version,
            draft: match (row.draft_subject, row.draft_body) {
                (Some(subject), Some(body)) => Some(DraftMessage { subject, body }),
                _ => None,
            },
            draft_review,
            created_at: row.created_at,
        })
    }
}
