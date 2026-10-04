use chrono::{DateTime, Utc};
use common::CoreError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Assessment, AssessmentId, DraftMessage, Evidence, LeadId, Provenance, RevisionNumber};

#[derive(Debug, Clone)]
pub struct AssessmentRow {
    pub id: Uuid,
    pub lead_id: Uuid,
    pub profile_revision: i32,
    pub summary: String,
    pub approach_angle: String,
    pub evidence: serde_json::Value,
    pub model: String,
    pub prompt_version: String,
    pub draft_subject: Option<String>,
    pub draft_body: Option<String>,
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
        Ok(Self {
            id: AssessmentId(row.id),
            lead_id: LeadId(row.lead_id),
            profile_revision,
            summary: row.summary,
            approach_angle: row.approach_angle,
            evidence: evidence_from_json(row.evidence)?,
            model: row.model,
            prompt_version: row.prompt_version,
            draft: match (row.draft_subject, row.draft_body) {
                (Some(subject), Some(body)) => Some(DraftMessage { subject, body }),
                _ => None,
            },
            created_at: row.created_at,
        })
    }
}
