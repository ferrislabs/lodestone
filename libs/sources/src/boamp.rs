use std::{collections::HashSet, sync::Arc};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use lodestone_core::{
    ActiveProfile, Candidate, FetchPurpose, FetchRequest, Fetcher, NormalizedDomain, Provenance,
    SignalKind, SourceError, SourceName, triage::CandidateSource,
};
use serde_json::{Value, json};
use tracing::{debug, info};
use url::Url;

use crate::buyer_domain::{domain_of_email, domain_of_url};

pub const PAGE_SIZE: usize = 50;
pub const OFFSET_CAP: usize = 10_000;
pub const TEXT_MAX_CHARS: usize = 4000;
const FALLBACK_NOTICE_URL: &str = "https://www.boamp.fr/pages/avis/?q=idweb:";
const DESCRIPTION_KEYS: [&str; 4] = ["objet", "principales", "description", "intitule"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoampSettings {
    pub base_url: Url,
    pub lookback_days: u32,
    pub max_pages: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoampStats {
    pub pages: u64,
    pub records: u64,
    pub candidates: u64,
    pub skipped_no_domain: u64,
    pub skipped_malformed: u64,
}

pub struct BoampSource<F: Fetcher> {
    fetcher: Arc<F>,
    settings: BoampSettings,
}

pub fn keyword_union(profiles: &[ActiveProfile]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut union = Vec::new();
    for rules in profiles.iter().flat_map(|active| {
        active
            .revision
            .definition
            .definition()
            .enabled_keyword_rules(SourceName::Boamp)
    }) {
        for keyword in &rules.keywords {
            let keyword = keyword.trim();
            if !keyword.is_empty() && seen.insert(keyword.to_lowercase()) {
                union.push(keyword.to_owned());
            }
        }
    }
    union
}

fn quoted(keyword: &str) -> String {
    let escaped = keyword.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

pub fn where_clause(keywords: &[String], now: DateTime<Utc>, lookback_days: u32) -> String {
    let since = (now - Duration::days(i64::from(lookback_days))).date_naive();
    let terms: Vec<String> = keywords.iter().map(|keyword| quoted(keyword)).collect();
    format!(
        "({}) and nature in (\"APPEL_OFFRE\") and datelimitereponse>=now() and dateparution>=date'{}'",
        terms.join(" or "),
        since.format("%Y-%m-%d")
    )
}

fn page_url(base: &Url, clause: &str, offset: usize) -> Url {
    let mut url = base.clone();
    url.query_pairs_mut()
        .append_pair("where", clause)
        .append_pair("order_by", "dateparution desc")
        .append_pair("limit", &PAGE_SIZE.to_string())
        .append_pair("offset", &offset.to_string());
    url
}

fn text_of(value: &Value) -> Option<&str> {
    match value {
        Value::String(text) => Some(text),
        Value::Object(map) => map.get("#text").and_then(Value::as_str),
        _ => None,
    }
}

fn items(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(items) => items.iter().collect(),
        Value::Null => Vec::new(),
        other => vec![other],
    }
}

fn at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |current, key| current.get(key))
}

fn find_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map
            .get(key)
            .or_else(|| map.values().find_map(|inner| find_key(inner, key))),
        Value::Array(list) => list.iter().find_map(|inner| find_key(inner, key)),
        _ => None,
    }
}

#[derive(Default)]
struct BuyerContacts<'a> {
    urls: Vec<&'a str>,
    emails: Vec<&'a str>,
}

fn eforms_buyer_ids(notice: &Value) -> Vec<&str> {
    notice
        .get("cac:ContractingParty")
        .map(items)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|party| at(party, &["cac:Party", "cac:PartyIdentification", "cbc:ID"]))
        .filter_map(text_of)
        .collect()
}

fn eforms_contacts(notice: &Value) -> BuyerContacts<'_> {
    let mut contacts = BuyerContacts::default();
    let ids = eforms_buyer_ids(notice);
    let organizations = find_key(notice, "efac:Organization")
        .map(items)
        .unwrap_or_default();
    for company in organizations
        .into_iter()
        .filter_map(|organization| organization.get("efac:Company"))
    {
        let id = at(company, &["cac:PartyIdentification", "cbc:ID"]).and_then(text_of);
        if !id.is_some_and(|id| ids.contains(&id)) {
            continue;
        }
        contacts
            .urls
            .extend(company.get("cbc:WebsiteURI").and_then(text_of));
        contacts
            .emails
            .extend(at(company, &["cac:Contact", "cbc:ElectronicMail"]).and_then(text_of));
    }
    contacts
}

fn notice_contacts(root: &Value) -> BuyerContacts<'_> {
    let mut contacts = BuyerContacts::default();
    let organism = root.get("organisme");
    contacts.urls.extend(
        organism
            .and_then(|organism| at(organism, &["coord", "url"]))
            .and_then(text_of),
    );
    contacts.urls.extend(
        organism
            .and_then(|organism| organism.get("urlProfilAcheteur"))
            .and_then(text_of),
    );
    contacts.emails.extend(
        organism
            .and_then(|organism| at(organism, &["coord", "mel"]))
            .and_then(text_of),
    );
    contacts
        .emails
        .extend(at(root, &["initial", "communication", "adresseMailContact"]).and_then(text_of));
    contacts
}

fn notice_root(donnees: &Value) -> Option<(&str, &Value)> {
    let (format, root) = donnees.as_object()?.iter().next()?;
    Some((format.as_str(), root))
}

pub fn resolve_buyer_domain(donnees: &Value) -> Option<NormalizedDomain> {
    let (format, root) = notice_root(donnees)?;
    let contacts = if format == "EFORMS" {
        eforms_contacts(root.as_object()?.values().next()?)
    } else {
        notice_contacts(root)
    };
    contacts
        .urls
        .iter()
        .find_map(|url| domain_of_url(url))
        .or_else(|| {
            contacts
                .emails
                .iter()
                .find_map(|email| domain_of_email(email))
        })
}

fn all_strings<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => out.push(text),
        Value::Array(list) => list.iter().for_each(|inner| all_strings(inner, out)),
        Value::Object(map) => map
            .iter()
            .filter(|(key, _)| !key.starts_with('@'))
            .for_each(|(_, inner)| all_strings(inner, out)),
        _ => {}
    }
}

fn collect_texts<'a>(value: &'a Value, keys: &[&str], out: &mut Vec<&'a str>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                if keys.contains(&key.as_str()) {
                    all_strings(inner, out);
                } else {
                    collect_texts(inner, keys, out);
                }
            }
        }
        Value::Array(list) => list
            .iter()
            .for_each(|inner| collect_texts(inner, keys, out)),
        _ => {}
    }
}

fn description_texts(donnees: &Value) -> Vec<&str> {
    let mut out = Vec::new();
    let Some((format, root)) = notice_root(donnees) else {
        return out;
    };
    if format == "EFORMS" {
        if let Some(notice) = root.as_object().and_then(|map| map.values().next()) {
            for section in ["cac:ProcurementProject", "cac:ProcurementProjectLot"] {
                if let Some(project) = notice.get(section) {
                    collect_texts(project, &["cbc:Name", "cbc:Description"], &mut out);
                }
            }
        }
    } else if let Some(initial) = root.get("initial") {
        collect_texts(initial, &DESCRIPTION_KEYS, &mut out);
    }
    out
}

fn labels(record: &Value) -> Vec<&str> {
    match record.get("descripteur_libelle") {
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).collect(),
        Some(Value::String(text)) => vec![text.as_str()],
        _ => Vec::new(),
    }
}

fn bounded_text(record: &Value, donnees: &Value) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let labels = labels(record);
    let mut text = labels.join(", ");
    for description in description_texts(donnees) {
        let description = description.trim();
        if description.is_empty() || parts.contains(&description) {
            continue;
        }
        parts.push(description);
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(description);
        if text.chars().count() >= TEXT_MAX_CHARS {
            break;
        }
    }
    text.chars().take(TEXT_MAX_CHARS).collect()
}

#[derive(Debug, PartialEq, Eq)]
enum Skip {
    Malformed,
    NoDomain,
}

fn str_field<'a>(record: &'a Value, key: &str) -> Option<&'a str> {
    record
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

fn parse_record(record: &Value) -> Result<Candidate, Skip> {
    let idweb = str_field(record, "idweb").ok_or(Skip::Malformed)?;
    let published = str_field(record, "dateparution").ok_or(Skip::Malformed)?;
    let observed_at = NaiveDate::parse_from_str(published, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|naive| naive.and_utc())
        .ok_or(Skip::Malformed)?;
    let donnees = match record.get("donnees") {
        Some(Value::String(raw)) => serde_json::from_str(raw).unwrap_or(Value::Null),
        Some(other) => other.clone(),
        None => Value::Null,
    };
    let domain = resolve_buyer_domain(&donnees).ok_or(Skip::NoDomain)?;
    let notice_url = str_field(record, "url_avis")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{FALLBACK_NOTICE_URL}{idweb}"));
    let provenance = Provenance::parse(&notice_url)
        .or_else(|_| Provenance::parse(&format!("{FALLBACK_NOTICE_URL}{idweb}")))
        .map_err(|_| Skip::Malformed)?;
    let kind = SignalKind::parse("public_tender").map_err(|_| Skip::Malformed)?;
    Ok(Candidate {
        domain,
        source: SourceName::Boamp,
        kind,
        title: str_field(record, "objet").unwrap_or_default().to_owned(),
        text: bounded_text(record, &donnees),
        provenance,
        observed_at,
        details: json!({
            "idweb": idweb,
            "buyer": record.get("nomacheteur"),
            "deadline": record.get("datelimitereponse"),
            "published": published,
            "notice_url": notice_url,
        }),
    })
}

fn upstream(message: impl Into<String>) -> SourceError {
    SourceError::Upstream(message.into())
}

impl<F: Fetcher> BoampSource<F> {
    pub fn new(fetcher: Arc<F>, settings: BoampSettings) -> Self {
        Self { fetcher, settings }
    }

    async fn fetch_page(&self, url: Url) -> Result<Vec<Value>, SourceError> {
        let request = FetchRequest {
            url,
            purpose: FetchPurpose::Api,
            source: SourceName::Boamp,
        };
        let response = self.fetcher.fetch(&request).await?;
        if response.status != 200 {
            return Err(upstream(format!("boamp answered {}", response.status)));
        }
        if response.truncated {
            return Err(upstream("boamp response exceeded the body cap"));
        }
        let mut page: Value = serde_json::from_slice(&response.body)
            .map_err(|error| upstream(format!("boamp response is not json: {error}")))?;
        match page.get_mut("results").map(Value::take) {
            Some(Value::Array(records)) => Ok(records),
            _ => Err(upstream("boamp response has no results array")),
        }
    }

    pub async fn collect_with_stats(
        &self,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<(Vec<Candidate>, BoampStats), SourceError> {
        let mut stats = BoampStats::default();
        let keywords = keyword_union(profiles);
        if keywords.is_empty() {
            return Ok((Vec::new(), stats));
        }
        let clause = where_clause(&keywords, now, self.settings.lookback_days);
        let mut seen: HashSet<String> = HashSet::new();
        let mut candidates = Vec::new();
        for page in 0..self.settings.max_pages {
            let offset = usize::try_from(page)
                .unwrap_or(usize::MAX)
                .saturating_mul(PAGE_SIZE);
            if offset.saturating_add(PAGE_SIZE) > OFFSET_CAP {
                break;
            }
            let records = self
                .fetch_page(page_url(&self.settings.base_url, &clause, offset))
                .await?;
            stats.pages += 1;
            let fetched = records.len();
            for record in &records {
                stats.records += 1;
                let idweb = str_field(record, "idweb").unwrap_or_default();
                if !idweb.is_empty() && !seen.insert(idweb.to_owned()) {
                    continue;
                }
                match parse_record(record) {
                    Ok(candidate) => {
                        stats.candidates += 1;
                        candidates.push(candidate);
                    }
                    Err(Skip::NoDomain) => {
                        stats.skipped_no_domain += 1;
                        debug!(idweb, "boamp notice skipped, no buyer domain");
                    }
                    Err(Skip::Malformed) => {
                        stats.skipped_malformed += 1;
                        debug!(idweb, "boamp notice skipped, malformed record");
                    }
                }
            }
            if fetched < PAGE_SIZE {
                break;
            }
        }
        info!(
            pages = stats.pages,
            records = stats.records,
            candidates = stats.candidates,
            skipped_no_domain = stats.skipped_no_domain,
            skipped_malformed = stats.skipped_malformed,
            "boamp fetch finished"
        );
        Ok((candidates, stats))
    }
}

impl<F: Fetcher> CandidateSource for BoampSource<F> {
    fn name(&self) -> SourceName {
        SourceName::Boamp
    }

    async fn collect(
        &self,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        self.collect_with_stats(profiles, now)
            .await
            .map(|(candidates, _)| candidates)
    }
}

#[cfg(test)]
mod tests;
