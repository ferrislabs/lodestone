use std::{cmp::Reverse, sync::Arc, time::Duration};

use chrono::{DateTime, NaiveDate, Utc};
use lodestone_core::{
    EcosystemFinding, EcosystemModule, FetchError, Fetcher, Inspection, InspectionTarget,
    SIREN_UNKNOWN, Siren, SourceError,
};
use serde::Deserialize;
use serde_json::{from_slice, json};
use url::Url;

use super::{loose_number, loose_text, non_empty};
use crate::{
    company::CompanyResearch,
    ecosystem::registry::{day_start, parse_day, read_api, resolve_siren, shortened},
    endpoint::Endpoint,
};

pub use lodestone_core::DECP_HOLDERS_DEFAULT_URL as DECP_HOLDERS_URL;

pub const MAX_PUBLIC_CONTRACTS: usize = 20;
const PAGE_SIZE: &str = "50";
const SERVICE: &str = "DECP";

#[derive(Debug, Clone)]
pub struct HolderSettings {
    pub holders_url: Endpoint,
    pub max_rate_limit_wait: Duration,
}

#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    data: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    #[serde(default, deserialize_with = "loose_text")]
    uid: Option<String>,
    #[serde(default, deserialize_with = "loose_text")]
    id: Option<String>,
    #[serde(default, deserialize_with = "loose_text")]
    acheteur_id: Option<String>,
    #[serde(default, deserialize_with = "loose_text")]
    acheteur_nom: Option<String>,
    #[serde(default, deserialize_with = "loose_text")]
    titulaire_id: Option<String>,
    #[serde(
        default,
        rename = "titulaire_typeIdentifiant",
        deserialize_with = "loose_text"
    )]
    titulaire_kind: Option<String>,
    #[serde(default, deserialize_with = "loose_text")]
    objet: Option<String>,
    #[serde(default, deserialize_with = "loose_text")]
    procedure: Option<String>,
    #[serde(default, rename = "dateNotification", deserialize_with = "loose_text")]
    notified_on: Option<String>,
    #[serde(default, deserialize_with = "loose_number")]
    montant: Option<f64>,
    #[serde(default, rename = "donneesActuelles")]
    current: Option<bool>,
}

fn holds(row: &Row, siren: &Siren) -> bool {
    let identified = row.titulaire_kind.as_deref().is_none_or(|kind| {
        let kind = kind.trim();
        kind.eq_ignore_ascii_case("SIRET") || kind.eq_ignore_ascii_case("SIREN")
    });
    identified
        && row
            .titulaire_id
            .as_deref()
            .and_then(|id| Siren::parse(id).ok())
            .is_some_and(|holder| &holder == siren)
}

fn award(
    row: Row,
    source: &str,
    now: DateTime<Utc>,
) -> Option<(NaiveDate, String, EcosystemFinding)> {
    let notified_on = row.notified_on.as_deref().and_then(parse_day)?;
    let object = non_empty(row.objet)?;
    let contract_id = non_empty(row.id);
    let buyer_siret = non_empty(row.acheteur_id);
    let identity = non_empty(row.uid).unwrap_or_else(|| {
        format!(
            "{}{}",
            buyer_siret.as_deref().unwrap_or_default(),
            contract_id.as_deref().unwrap_or_default()
        )
    });
    let finding = EcosystemFinding::new(
        EcosystemModule::PublicContracts,
        "contract",
        "Marché remporté",
        shortened(&object),
        source,
        day_start(notified_on).unwrap_or(now),
    )
    .keyed(format!("contract:{identity}"))
    .with_details(json!({
        "date": notified_on.format("%Y-%m-%d").to_string(),
        "buyer": non_empty(row.acheteur_nom),
        "buyer_siret": buyer_siret,
        "amount_eur": row.montant,
        "procedure": non_empty(row.procedure),
        "contract_id": contract_id,
        "holder_siret": non_empty(row.titulaire_id),
    }));
    Some((notified_on, identity, finding))
}

pub fn parse_awards(
    body: &[u8],
    siren: &Siren,
    source: &str,
    now: DateTime<Utc>,
) -> Result<Vec<EcosystemFinding>, String> {
    let page: Page = from_slice(body)
        .map_err(|error| format!("DECP response is not the expected json: {error}"))?;
    let mut awards: Vec<(NaiveDate, String, EcosystemFinding)> = page
        .data
        .into_iter()
        .filter(|row| row.current != Some(false) && holds(row, siren))
        .filter_map(|row| award(row, source, now))
        .collect();
    awards.sort_by_key(|(date, _, _)| Reverse(*date));
    let mut findings: Vec<EcosystemFinding> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (_, identity, finding) in awards {
        if !seen.contains(&identity) {
            seen.push(identity);
            findings.push(finding);
        }
    }
    findings.truncate(MAX_PUBLIC_CONTRACTS);
    Ok(findings)
}

pub fn holder_query(base: &Url, siren: &Siren) -> Url {
    let mut url = base.clone();
    url.set_query(None);
    url.query_pairs_mut()
        .append_pair("titulaire_id__contains", siren.as_str())
        .append_pair("dateNotification__sort", "desc")
        .append_pair("page_size", PAGE_SIZE);
    url
}

pub struct PublicContractsModule<F: Fetcher> {
    fetcher: Arc<F>,
    settings: HolderSettings,
    research: CompanyResearch<F>,
}

impl<F: Fetcher> PublicContractsModule<F> {
    pub fn new(fetcher: Arc<F>, settings: HolderSettings, research: CompanyResearch<F>) -> Self {
        Self {
            fetcher,
            settings,
            research,
        }
    }

    pub async fn inspect(&self, target: &InspectionTarget<'_>) -> Result<Inspection, SourceError> {
        let Some(siren) = resolve_siren(&self.research, target).await? else {
            return Ok(Inspection::NotApplicable(SIREN_UNKNOWN));
        };
        let base = self.settings.holders_url.resolve().await?;
        if base.cannot_be_a_base() {
            return Err(SourceError::Fetch(FetchError::InvalidUrl));
        }
        let url = holder_query(&base, &siren);
        let source = url.to_string();
        let body = read_api(
            self.fetcher.as_ref(),
            url,
            self.settings.max_rate_limit_wait,
            SERVICE,
        )
        .await?;
        parse_awards(&body, &siren, &source, target.now)
            .map(Inspection::Found)
            .map_err(SourceError::Upstream)
    }
}

#[cfg(test)]
mod tests;
