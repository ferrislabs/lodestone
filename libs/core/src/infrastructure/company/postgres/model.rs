use chrono::{DateTime, Utc};
use common::CoreError;
use serde_json::{Value, from_value};
use uuid::Uuid;

use crate::{
    LeadId, NormalizedDomain, ProfileId, ProfileName,
    domain::{
        company::{
            CompanyDossier, CompanyEntity, CompanyPage, DossierState, OfficialData, SiblingLead,
            Siren,
        },
        qualification::Verdict,
        targeting::TargetedLead,
    },
};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DossierRow {
    pub domain: String,
    pub state: String,
    pub siren: Option<String>,
    pub official: Option<serde_json::Value>,
    pub pages: serde_json::Value,
    pub content_hash: Option<String>,
    pub content_at: Option<DateTime<Utc>>,
    pub built_at: Option<DateTime<Utc>>,
    pub attempts: i32,
    pub next_attempt_at: DateTime<Utc>,
    pub error: Option<String>,
    pub ignored_links: i32,
    pub pages_checked_at: Option<DateTime<Utc>>,
}

fn invalid(what: &str) -> CoreError {
    CoreError::Internal(format!("stored company dossier {what} is invalid"))
}

impl TryFrom<DossierRow> for CompanyDossier {
    type Error = CoreError;

    fn try_from(row: DossierRow) -> Result<Self, Self::Error> {
        let official: Option<OfficialData> = row
            .official
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| invalid("official data"))?;
        let pages: Vec<CompanyPage> =
            serde_json::from_value(row.pages).map_err(|_| invalid("pages"))?;
        Ok(Self {
            domain: NormalizedDomain::from_host(&row.domain).map_err(|_| invalid("domain"))?,
            state: DossierState::parse(&row.state).ok_or_else(|| invalid("state"))?,
            siren: row
                .siren
                .as_deref()
                .map(Siren::parse)
                .transpose()
                .map_err(|_| invalid("siren"))?,
            official,
            pages,
            content_hash: row.content_hash,
            content_at: row.content_at,
            built_at: row.built_at,
            attempts: u32::try_from(row.attempts).unwrap_or_default(),
            next_attempt_at: row.next_attempt_at,
            error: row.error,
            ignored_links: u32::try_from(row.ignored_links).unwrap_or_default(),
            pages_checked_at: row.pages_checked_at,
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EntityRow {
    pub siren: String,
    pub legal_name: String,
}

impl TryFrom<EntityRow> for CompanyEntity {
    type Error = CoreError;

    fn try_from(row: EntityRow) -> Result<Self, Self::Error> {
        Ok(Self {
            siren: Siren::parse(&row.siren).map_err(|_| invalid("entity siren"))?,
            legal_name: row.legal_name,
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SiblingRow {
    pub id: Uuid,
    pub domain: String,
    pub profile_id: Uuid,
    pub profile_name: String,
    pub verdict: Option<String>,
    pub confidence: Option<f64>,
}

impl TryFrom<SiblingRow> for SiblingLead {
    type Error = CoreError;

    fn try_from(row: SiblingRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: LeadId(row.id),
            domain: NormalizedDomain::from_host(&row.domain).map_err(|_| invalid("domain"))?,
            profile_id: ProfileId(row.profile_id),
            profile_name: ProfileName::parse(&row.profile_name)
                .map_err(|_| invalid("profile name"))?,
            verdict: row
                .verdict
                .as_deref()
                .map(|raw| Verdict::parse(raw).ok_or_else(|| invalid("verdict")))
                .transpose()?,
            confidence: row.confidence,
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TargetedRow {
    pub id: Uuid,
    pub excluded_reason: Option<String>,
    pub official: Value,
}

impl TryFrom<TargetedRow> for TargetedLead {
    type Error = CoreError;

    fn try_from(row: TargetedRow) -> Result<Self, Self::Error> {
        Ok(Self {
            lead_id: LeadId(row.id),
            exclusion: row.excluded_reason,
            official: from_value(row.official).map_err(|_| invalid("official data"))?,
        })
    }
}
