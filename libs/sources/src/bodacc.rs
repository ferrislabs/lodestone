use std::{cmp::Reverse, sync::Arc, time::Duration};

use chrono::{DateTime, NaiveDate, Utc};
use lodestone_core::{
    EcosystemFinding, EcosystemModule, FetchError, Fetcher, Inspection, InspectionTarget,
    SIREN_UNKNOWN, Siren, SourceError,
};
use serde::Deserialize;
use serde_json::{Value, from_slice, from_str, json};
use url::Url;

use crate::{
    company::CompanyResearch,
    ecosystem::registry::{day_start, parse_day, read_api, resolve_siren, shortened},
    endpoint::Endpoint,
};

pub use lodestone_core::BODACC_DEFAULT_URL as BODACC_URL;

pub const MAX_LEGAL_EVENTS: usize = 20;
const SERVICE: &str = "BODACC";
const DEREGISTERED: &str = "Radiation du registre du commerce et des sociétés";

#[derive(Debug, Clone)]
pub struct BodaccSettings {
    pub dataset_url: Endpoint,
    pub max_rate_limit_wait: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Creation,
    Registration,
    Modification,
    Sale,
    Deregistration,
    Collective,
    Conciliation,
    Recovery,
    Accounts,
    Other,
}

impl Family {
    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "creation" => Self::Creation,
            "immatriculation" => Self::Registration,
            "modification" => Self::Modification,
            "vente" => Self::Sale,
            "radiation" => Self::Deregistration,
            "collective" => Self::Collective,
            "conciliation" => Self::Conciliation,
            "retablissement_professionnel" => Self::Recovery,
            "dpc" => Self::Accounts,
            _ => Self::Other,
        }
    }

    pub fn kind(self) -> &'static str {
        match self {
            Self::Creation => "creation",
            Self::Registration => "registration",
            Self::Modification => "modification",
            Self::Sale => "sale",
            Self::Deregistration => "deregistration",
            Self::Collective => "collective_proceeding",
            Self::Conciliation => "conciliation",
            Self::Recovery => "professional_recovery",
            Self::Accounts => "accounts_filing",
            Self::Other => "announcement",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Creation => "Création",
            Self::Registration => "Immatriculation",
            Self::Modification => "Modification",
            Self::Sale => "Vente ou cession",
            Self::Deregistration => "Radiation",
            Self::Collective => "Procédure collective",
            Self::Conciliation => "Procédure de conciliation",
            Self::Recovery => "Rétablissement professionnel",
            Self::Accounts => "Dépôt des comptes",
            Self::Other => "Annonce",
        }
    }
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    results: Vec<Record>,
}

#[derive(Deserialize)]
struct Record {
    id: Option<String>,
    dateparution: Option<String>,
    familleavis: Option<String>,
    familleavis_lib: Option<String>,
    typeavis: Option<String>,
    typeavis_lib: Option<String>,
    tribunal: Option<String>,
    jugement: Option<Value>,
    acte: Option<Value>,
    modificationsgenerales: Option<Value>,
    depot: Option<Value>,
    radiationaurcs: Option<Value>,
    url_complete: Option<String>,
}

fn nested(value: Option<Value>) -> Value {
    match value {
        Some(Value::String(raw)) => from_str(&raw).unwrap_or(Value::Null),
        Some(other) => other,
        None => Value::Null,
    }
}

fn text_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter()
        .try_fold(value, |current, key| current.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

struct Parts {
    judgment: Value,
    act: Value,
    changes: Value,
    filing: Value,
    deregistration: Value,
}

fn description(family: Family, parts: &Parts) -> Option<String> {
    let act_text = || {
        text_at(&parts.act, &["creation", "categorieCreation"])
            .or_else(|| text_at(&parts.act, &["immatriculation", "categorieImmatriculation"]))
            .or_else(|| text_at(&parts.act, &["vente", "categorieVente"]))
            .or_else(|| text_at(&parts.act, &["descriptif"]))
    };
    match family {
        Family::Collective | Family::Conciliation | Family::Recovery => {
            text_at(&parts.judgment, &["nature"])
                .or_else(|| text_at(&parts.judgment, &["famille"]))
                .map(str::to_owned)
        }
        Family::Creation | Family::Registration | Family::Sale => act_text().map(str::to_owned),
        Family::Modification => text_at(&parts.changes, &["descriptif"])
            .or_else(act_text)
            .map(str::to_owned),
        Family::Deregistration => Some(
            text_at(&parts.deregistration, &["commentaire"])
                .unwrap_or(DEREGISTERED)
                .to_owned(),
        ),
        Family::Accounts => {
            let kind = text_at(&parts.filing, &["typeDepot"]);
            let closing = text_at(&parts.filing, &["dateCloture"]);
            match (kind, closing) {
                (Some(kind), Some(closing)) => Some(format!("{kind}, clôture le {closing}")),
                (Some(kind), None) => Some(kind.to_owned()),
                (None, Some(closing)) => Some(format!("Comptes clos le {closing}")),
                (None, None) => None,
            }
        }
        Family::Other => act_text()
            .or_else(|| text_at(&parts.changes, &["descriptif"]))
            .map(str::to_owned),
    }
}

fn finding(
    record: Record,
    index: usize,
    source: &str,
    now: DateTime<Utc>,
) -> Option<(NaiveDate, EcosystemFinding)> {
    let date = record.dateparution.as_deref().and_then(parse_day)?;
    let family = Family::parse(record.familleavis.as_deref().unwrap_or_default());
    let parts = Parts {
        judgment: nested(record.jugement),
        act: nested(record.acte),
        changes: nested(record.modificationsgenerales),
        filing: nested(record.depot),
        deregistration: nested(record.radiationaurcs),
    };
    let mut value = description(family, &parts)
        .or_else(|| non_empty(record.familleavis_lib.as_deref()).map(str::to_owned))
        .unwrap_or_else(|| family.label().to_owned());
    let initial = record
        .typeavis
        .as_deref()
        .is_none_or(|kind| kind.trim() == "annonce");
    let notice = non_empty(record.typeavis_lib.as_deref());
    if !initial && let Some(notice) = notice {
        value = format!("{value} ({})", notice.to_lowercase());
    }
    let id = non_empty(record.id.as_deref());
    let key = match id {
        Some(id) => format!("bodacc:{id}"),
        None => format!("bodacc:{date}:{index}"),
    };
    let link = non_empty(record.url_complete.as_deref());
    let details = json!({
        "date": date.format("%Y-%m-%d").to_string(),
        "court": non_empty(record.tribunal.as_deref()),
        "family": family.kind(),
        "notice": notice,
        "announcement_id": id,
        "link": link,
        "judgment_date": text_at(&parts.judgment, &["date"]),
        "collective": family == Family::Collective,
    });
    let finding = EcosystemFinding::new(
        EcosystemModule::LegalLife,
        family.kind(),
        family.label(),
        shortened(&value),
        source,
        day_start(date).unwrap_or(now),
    )
    .keyed(key)
    .with_details(details);
    Some((date, finding))
}

pub fn parse_announcements(
    body: &[u8],
    source: &str,
    now: DateTime<Utc>,
) -> Result<Vec<EcosystemFinding>, String> {
    let answer: Answer = from_slice(body)
        .map_err(|error| format!("BODACC response is not the expected json: {error}"))?;
    let mut dated: Vec<(NaiveDate, EcosystemFinding)> = answer
        .results
        .into_iter()
        .enumerate()
        .filter_map(|(index, record)| finding(record, index, source, now))
        .collect();
    dated.sort_by_key(|(date, _)| Reverse(*date));
    let mut findings: Vec<EcosystemFinding> = Vec::new();
    for (_, found) in dated {
        if !findings.iter().any(|known| known.key == found.key) {
            findings.push(found);
        }
    }
    findings.truncate(MAX_LEGAL_EVENTS);
    Ok(findings)
}

pub fn records_url(dataset: &Url, siren: &Siren) -> Option<Url> {
    let mut url = dataset.clone();
    url.set_query(None);
    url.path_segments_mut().ok()?.pop_if_empty().push("records");
    url.query_pairs_mut()
        .append_pair("where", &format!("registre=\"{siren}\""))
        .append_pair("order_by", "dateparution desc")
        .append_pair("limit", &MAX_LEGAL_EVENTS.to_string());
    Some(url)
}

pub struct LegalLifeModule<F: Fetcher> {
    fetcher: Arc<F>,
    settings: BodaccSettings,
    research: CompanyResearch<F>,
}

impl<F: Fetcher> LegalLifeModule<F> {
    pub fn new(fetcher: Arc<F>, settings: BodaccSettings, research: CompanyResearch<F>) -> Self {
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
        let dataset = self.settings.dataset_url.resolve().await?;
        let url =
            records_url(&dataset, &siren).ok_or(SourceError::Fetch(FetchError::InvalidUrl))?;
        let source = url.to_string();
        let body = read_api(
            self.fetcher.as_ref(),
            url,
            self.settings.max_rate_limit_wait,
            SERVICE,
        )
        .await?;
        parse_announcements(&body, &source, target.now)
            .map(Inspection::Found)
            .map_err(SourceError::Upstream)
    }
}

#[cfg(test)]
mod tests;
