use chrono::{DateTime, Utc};
use common::CoreError;

use crate::{
    NormalizedDomain,
    domain::company::{CompanyDossier, CompanyPage, DossierState, OfficialData, Siren},
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
        })
    }
}
