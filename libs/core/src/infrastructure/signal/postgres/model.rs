use chrono::{DateTime, Utc};
use common::CoreError;
use uuid::Uuid;

use crate::{
    LeadId, Provenance, Signal, SignalId, SignalKind, SourceName, UnitInterval, definition::AgentId,
};

#[derive(Debug, Clone)]
pub struct SignalRow {
    pub id: Uuid,
    pub lead_id: Uuid,
    pub kind: String,
    pub value: Option<String>,
    pub source: String,
    pub raw: serde_json::Value,
    pub provenance: String,
    pub strength: f64,
    pub observed_at: DateTime<Utc>,
    pub agent_id: Option<String>,
}

impl TryFrom<SignalRow> for Signal {
    type Error = CoreError;

    fn try_from(row: SignalRow) -> Result<Self, Self::Error> {
        let kind = SignalKind::parse(&row.kind)
            .map_err(|_| CoreError::Internal("stored signal kind is invalid".to_owned()))?;
        let source = row
            .source
            .parse::<SourceName>()
            .map_err(|_| CoreError::Internal("stored signal source is invalid".to_owned()))?;
        let provenance = Provenance::parse(&row.provenance)
            .map_err(|_| CoreError::Internal("stored signal provenance is invalid".to_owned()))?;
        let strength = UnitInterval::new(row.strength)
            .map_err(|_| CoreError::Internal("stored signal strength is invalid".to_owned()))?;
        let agent = row.agent_id.map(AgentId::new).filter(AgentId::is_slug);
        Ok(Self {
            id: SignalId(row.id),
            lead_id: LeadId(row.lead_id),
            kind,
            value: row.value,
            source,
            raw: row.raw,
            provenance,
            strength,
            observed_at: row.observed_at,
            agent,
        })
    }
}
