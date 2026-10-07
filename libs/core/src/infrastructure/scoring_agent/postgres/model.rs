use chrono::{DateTime, Utc};
use common::CoreError;
use uuid::Uuid;

use crate::{
    GenerationSnapshot, LeadId, ProfileId, ProviderId, RevisionNumber, RunProvenance,
    definition::AgentId,
    domain::{
        qualification::ResultStatus,
        scoring_agent::{ScoringResult, ScoringResultId, VerdictAnswer},
    },
};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ScoringResultRow {
    pub id: Uuid,
    pub lead_id: Uuid,
    pub profile_id: Uuid,
    pub profile_revision: i32,
    pub agent_id: String,
    pub provider_id: Option<Uuid>,
    pub provider_name: String,
    pub model: String,
    pub prompt_version: String,
    pub generation: serde_json::Value,
    pub samples: i32,
    pub status: String,
    pub criteria: serde_json::Value,
    pub score: Option<i16>,
    pub verdict: Option<serde_json::Value>,
    pub reasons: serde_json::Value,
    pub error: Option<String>,
    pub raw_samples: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl TryFrom<ScoringResultRow> for ScoringResult {
    type Error = CoreError;

    fn try_from(row: ScoringResultRow) -> Result<Self, Self::Error> {
        let profile_revision = u32::try_from(row.profile_revision)
            .ok()
            .and_then(RevisionNumber::new)
            .ok_or_else(|| CoreError::Internal("stored scoring revision is invalid".to_owned()))?;
        let status = ResultStatus::parse(&row.status)
            .ok_or_else(|| CoreError::Internal("stored scoring status is invalid".to_owned()))?;
        let generation: GenerationSnapshot = serde_json::from_value(row.generation)
            .map_err(|_| CoreError::Internal("stored scoring generation is invalid".to_owned()))?;
        let samples = u32::try_from(row.samples)
            .ok()
            .filter(|samples| *samples >= 1)
            .ok_or_else(|| CoreError::Internal("stored scoring samples is invalid".to_owned()))?;
        let score = row
            .score
            .map(|score| {
                u8::try_from(score)
                    .ok()
                    .filter(|score| *score <= 100)
                    .ok_or_else(|| {
                        CoreError::Internal("stored scoring score is invalid".to_owned())
                    })
            })
            .transpose()?;
        let verdict = row
            .verdict
            .as_ref()
            .map(|value| {
                VerdictAnswer::from_json(value).ok_or_else(|| {
                    CoreError::Internal("stored scoring verdict is invalid".to_owned())
                })
            })
            .transpose()?;
        let reasons: Vec<String> = serde_json::from_value(row.reasons)
            .map_err(|_| CoreError::Internal("stored scoring reasons are invalid".to_owned()))?;
        Ok(Self {
            id: ScoringResultId(row.id),
            lead_id: LeadId(row.lead_id),
            profile_id: ProfileId(row.profile_id),
            profile_revision,
            agent_id: AgentId::new(row.agent_id),
            provenance: RunProvenance {
                provider_id: row.provider_id.map(ProviderId),
                provider_name: row.provider_name,
                model: row.model,
                prompt_version: row.prompt_version,
                generation,
                samples,
            },
            status,
            criteria: row.criteria,
            score,
            verdict,
            reasons,
            error: row.error,
            raw_samples: row.raw_samples,
            created_at: row.created_at,
        })
    }
}
