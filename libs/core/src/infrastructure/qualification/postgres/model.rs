use chrono::{DateTime, Utc};
use common::CoreError;
use uuid::Uuid;

use crate::{
    LeadId, ProfileId, ProviderId, RevisionNumber,
    definition::AgentId,
    domain::qualification::{
        EvidenceStrength, LeadScoring, QualificationResult, ResultId, ResultStatus, VerdictCap,
    },
};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ResultRow {
    pub id: Uuid,
    pub lead_id: Uuid,
    pub profile_id: Uuid,
    pub profile_revision: i32,
    pub agent_id: String,
    pub provider_id: Option<Uuid>,
    pub model: String,
    pub prompt_hash: String,
    pub status: String,
    pub summary: Option<String>,
    pub answers: serde_json::Value,
    pub agent_score: Option<f64>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl TryFrom<ResultRow> for QualificationResult {
    type Error = CoreError;

    fn try_from(row: ResultRow) -> Result<Self, Self::Error> {
        let profile_revision = u32::try_from(row.profile_revision)
            .ok()
            .and_then(RevisionNumber::new)
            .ok_or_else(|| CoreError::Internal("stored result revision is invalid".to_owned()))?;
        let status = ResultStatus::parse(&row.status)
            .ok_or_else(|| CoreError::Internal("stored result status is invalid".to_owned()))?;
        Ok(Self {
            id: ResultId(row.id),
            lead_id: LeadId(row.lead_id),
            profile_id: ProfileId(row.profile_id),
            profile_revision,
            agent_id: AgentId::new(row.agent_id),
            provider_id: row.provider_id.map(ProviderId),
            model: row.model,
            prompt_hash: row.prompt_hash,
            status,
            summary: row.summary,
            answers: row.answers,
            agent_score: row.agent_score,
            error: row.error,
            created_at: row.created_at,
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ScoringRow {
    pub lead_id: Uuid,
    pub opinion: Option<f64>,
    pub evidence: Option<serde_json::Value>,
    pub verdict_reason: Option<String>,
}

impl TryFrom<ScoringRow> for LeadScoring {
    type Error = CoreError;

    fn try_from(row: ScoringRow) -> Result<Self, Self::Error> {
        let evidence: EvidenceStrength = row
            .evidence
            .ok_or_else(|| CoreError::Internal("stored evidence is missing".to_owned()))
            .and_then(|value| {
                serde_json::from_value(value)
                    .map_err(|_| CoreError::Internal("stored evidence is invalid".to_owned()))
            })?;
        let cap = row
            .verdict_reason
            .as_deref()
            .map(|raw| {
                VerdictCap::parse(raw).ok_or_else(|| {
                    CoreError::Internal("stored verdict reason is invalid".to_owned())
                })
            })
            .transpose()?;
        Ok(Self {
            opinion: row.opinion,
            evidence,
            cap,
        })
    }
}
