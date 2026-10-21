use chrono::{DateTime, NaiveDate, Utc};
use lodestone_core::{
    Candidate, NormalizedDomain, Provenance, SignalKind, SourceName, WELCOME_TO_THE_JUNGLE_HOST,
    alpha3_of,
};
use serde_json::{Value, json};
use url::Url;

use super::SITE_URL;
use crate::buyer_domain::domain_of_url;

pub const TEXT_MAX_CHARS: usize = 4000;
const DEFAULT_LANGUAGE: &str = "fr";
const WEBSITE_KEYS: [&str; 5] = ["website", "website_url", "websiteUrl", "site_url", "url"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Malformed;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Company<'a> {
    pub name: &'a str,
    pub slug: Option<&'a str>,
    pub website: Option<&'a str>,
}

impl Company<'_> {
    pub fn cache_key(&self) -> String {
        self.slug
            .map_or_else(|| self.name.to_lowercase(), str::to_owned)
    }

    pub fn own_domain(&self) -> Option<NormalizedDomain> {
        self.website.and_then(domain_of_url)
    }

    pub fn is(&self, organisation: &Value) -> bool {
        match self.slug {
            Some(slug) => text_of(organisation, "slug") == Some(slug),
            None => text_of(organisation, "name")
                .is_some_and(|name| name.eq_ignore_ascii_case(self.name)),
        }
    }
}

#[derive(Debug)]
pub struct Offer<'a> {
    hit: &'a Value,
    pub id: String,
    title: &'a str,
    pub company: Company<'a>,
    language: &'a str,
    provenance: Provenance,
    observed_at: DateTime<Utc>,
}

pub fn text_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

pub fn id_of(value: &Value, key: &str) -> Option<String> {
    match value.get(key)? {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

pub fn website_of(organisation: &Value) -> Option<&str> {
    WEBSITE_KEYS
        .iter()
        .find_map(|key| text_of(organisation, key))
}

pub fn is_offer_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str().is_some_and(|host| {
            host == WELCOME_TO_THE_JUNGLE_HOST
                || host
                    .strip_suffix(WELCOME_TO_THE_JUNGLE_HOST)
                    .is_some_and(|rest| rest.ends_with('.'))
        })
}

fn published(hit: &Value) -> Option<DateTime<Utc>> {
    text_of(hit, "published_at")
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|parsed| parsed.with_timezone(&Utc))
        .or_else(|| {
            hit.get("published_at_timestamp")
                .and_then(Value::as_i64)
                .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
        })
        .or_else(|| {
            text_of(hit, "published_at_date")
                .and_then(|raw| NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok())
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|moment| moment.and_utc())
        })
}

fn language_of(hit: &Value) -> &str {
    text_of(hit, "language")
        .filter(|language| language.len() == 2 && language.bytes().all(|b| b.is_ascii_lowercase()))
        .unwrap_or(DEFAULT_LANGUAGE)
}

fn offer_url(language: &str, organisation: &str, job: &str) -> Option<Provenance> {
    let mut url = Url::parse(SITE_URL).ok()?;
    url.path_segments_mut().ok()?.clear().extend([
        language,
        "companies",
        organisation,
        "jobs",
        job,
    ]);
    if !is_offer_url(&url) {
        return None;
    }
    Provenance::parse(url.as_str()).ok()
}

pub fn parse_hit(hit: &Value) -> Result<Offer<'_>, Malformed> {
    let id = id_of(hit, "objectID")
        .or_else(|| id_of(hit, "reference"))
        .ok_or(Malformed)?;
    let title = text_of(hit, "name")
        .or_else(|| text_of(hit, "title"))
        .ok_or(Malformed)?;
    let organisation = hit.get("organization").ok_or(Malformed)?;
    let company = Company {
        name: text_of(organisation, "name").ok_or(Malformed)?,
        slug: text_of(organisation, "slug"),
        website: website_of(organisation),
    };
    let observed_at = published(hit).ok_or(Malformed)?;
    let language = language_of(hit);
    let provenance = offer_url(
        language,
        company.slug.ok_or(Malformed)?,
        text_of(hit, "slug").ok_or(Malformed)?,
    )
    .ok_or(Malformed)?;
    Ok(Offer {
        hit,
        id,
        title,
        company,
        language,
        provenance,
        observed_at,
    })
}

pub fn contract_label(contract: &str) -> Option<&'static str> {
    match contract {
        "full_time" => Some("CDI"),
        "temporary" => Some("CDD"),
        "freelance" => Some("Freelance"),
        "internship" => Some("Stage"),
        "apprenticeship" => Some("Alternance"),
        "part_time" => Some("Temps partiel"),
        "vie" => Some("VIE"),
        _ => None,
    }
}

pub fn remote_label(remote: &str) -> Option<&'static str> {
    match remote {
        "fulltime" => Some("Télétravail total"),
        "partial" => Some("Télétravail partiel"),
        "punctual" => Some("Télétravail occasionnel"),
        "no" => Some("Pas de télétravail"),
        _ => None,
    }
}

fn joined(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        Value::Array(items) => {
            let lines: Vec<&str> = items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect();
            (!lines.is_empty()).then(|| lines.join("\n"))
        }
        _ => None,
    }
}

fn bounded_text(parts: &[Option<String>]) -> String {
    let mut text = String::new();
    for part in parts.iter().flatten() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(part);
        if text.chars().count() >= TEXT_MAX_CHARS {
            break;
        }
    }
    text.chars().take(TEXT_MAX_CHARS).collect()
}

impl Offer<'_> {
    fn office(&self) -> Option<&Value> {
        self.hit
            .get("offices")
            .and_then(Value::as_array)
            .and_then(|offices| offices.first())
            .or_else(|| self.hit.get("office"))
    }

    fn location(&self) -> Option<String> {
        let office = self.office()?;
        let parts: Vec<&str> = [text_of(office, "city"), text_of(office, "country")]
            .into_iter()
            .flatten()
            .collect();
        (!parts.is_empty()).then(|| parts.join(", "))
    }

    pub fn candidate(self, domain: NormalizedDomain) -> Result<Candidate, Malformed> {
        let kind = SignalKind::parse("job_offer").map_err(|_| Malformed)?;
        let contract = text_of(self.hit, "contract_type");
        let remote = text_of(self.hit, "remote");
        let location = self.location();
        let country = self
            .office()
            .and_then(|office| text_of(office, "country_code"))
            .and_then(alpha3_of);
        let text = bounded_text(&[
            Some(format!("Entreprise : {}", self.company.name)),
            contract
                .and_then(contract_label)
                .map(|label| format!("Contrat : {label}")),
            remote.and_then(remote_label).map(str::to_owned),
            location.as_ref().map(|place| format!("Lieu : {place}")),
            joined(self.hit.get("summary")),
            joined(self.hit.get("key_missions")),
            joined(self.hit.get("profile")),
            joined(self.hit.get("description")),
        ]);
        Ok(Candidate {
            domain,
            source: SourceName::WelcomeToTheJungle,
            kind,
            title: format!("{} — {}", self.title, self.company.name),
            text,
            provenance: self.provenance,
            observed_at: self.observed_at,
            details: json!({
                "offer_id": self.id,
                "reference": text_of(self.hit, "reference"),
                "company": self.company.name,
                "company_slug": self.company.slug,
                "contract_type": contract,
                "remote": remote,
                "location": location,
                "country": country,
                "language": self.language,
                "published": self.observed_at.to_rfc3339(),
            }),
        })
    }
}
