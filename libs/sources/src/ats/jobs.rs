use std::{collections::HashSet, sync::LazyLock};

use chrono::{DateTime, NaiveDate, Utc};
use lodestone_core::{Candidate, NormalizedDomain, Provenance, SignalKind, SourceName};
use regex::Regex;
use serde_json::{Value, from_str, json};
use url::Url;

use super::discover::{Board, Vendor};
use crate::contact_pages::extract::decode_entities;

pub const TEXT_MAX_CHARS: usize = 4000;

fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => unreachable!("static pattern {pattern} is invalid: {error}"),
    }
}

static INERT: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?is)<!--.*?-->|<script\b.*?</script\s*>|<style\b.*?</style\s*>"));
static BREAK: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"(?i)<br\s*/?>|</(?:p|div|li|ul|ol|h[1-6]|tr|table|section|article|blockquote)\s*>")
});
static TAG: LazyLock<Regex> = LazyLock::new(|| compile(r"<[^>]*>"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| compile(r"[ \t\u{a0}]+"));
static JSON_LD: LazyLock<Regex> = LazyLock::new(|| {
    compile(r#"(?is)<script[^>]*type\s*=\s*["']application/ld\+json["'][^>]*>(.*?)</script\s*>"#)
});
const MAX_JSON_LD_DEPTH: usize = 4;

fn typographic(text: &str) -> String {
    text.replace("&rsquo;", "\u{2019}")
        .replace("&lsquo;", "\u{2018}")
        .replace("&rdquo;", "\u{201d}")
        .replace("&ldquo;", "\u{201c}")
        .replace("&ndash;", "\u{2013}")
        .replace("&mdash;", "\u{2014}")
        .replace("&hellip;", "\u{2026}")
}

pub fn html_to_text(html: &str) -> String {
    let without_inert = INERT.replace_all(html, " ");
    let broken = BREAK.replace_all(&without_inert, "\n");
    let stripped = TAG.replace_all(&broken, " ");
    let decoded = decode_entities(&typographic(&stripped));
    let mut lines: Vec<String> = Vec::new();
    for line in decoded.lines() {
        let collapsed = SPACES.replace_all(line, " ");
        let trimmed = collapsed.trim();
        if trimmed.is_empty() && lines.last().is_none_or(String::is_empty) {
            continue;
        }
        lines.push(trimmed.to_owned());
    }
    lines.join("\n").trim().to_owned()
}

pub fn unescape_html(escaped: &str) -> String {
    decode_entities(escaped)
}

fn bounded(text: &str) -> String {
    text.chars().take(TEXT_MAX_CHARS).collect()
}

fn text_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn id_of(value: &Value) -> Option<String> {
    match value.get("id")? {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn parse_time(raw: Option<&str>) -> Option<DateTime<Utc>> {
    raw.and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|time| time.with_timezone(&Utc))
}

struct Posting {
    id: String,
    title: String,
    url: String,
    text: String,
    location: Option<String>,
    team: Option<String>,
    contract: Option<String>,
    remote: Option<bool>,
    observed_at: Option<DateTime<Utc>>,
}

fn greenhouse(job: &Value) -> Option<Posting> {
    let content = text_of(job, "content")
        .map(|escaped| html_to_text(&unescape_html(escaped)))
        .unwrap_or_default();
    Some(Posting {
        id: id_of(job)?,
        title: text_of(job, "title")?.to_owned(),
        url: text_of(job, "absolute_url")?.to_owned(),
        text: content,
        location: job
            .get("location")
            .and_then(|place| text_of(place, "name"))
            .map(str::to_owned),
        team: job
            .get("departments")
            .and_then(Value::as_array)
            .and_then(|departments| departments.iter().find_map(|d| text_of(d, "name")))
            .map(str::to_owned),
        contract: None,
        remote: None,
        observed_at: parse_time(text_of(job, "first_published"))
            .or_else(|| parse_time(text_of(job, "updated_at"))),
    })
}

fn lever(job: &Value) -> Option<Posting> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(plain) = text_of(job, "descriptionPlain") {
        parts.push(plain.to_owned());
    }
    if let Some(lists) = job.get("lists").and_then(Value::as_array) {
        for list in lists {
            if let Some(heading) = text_of(list, "text") {
                parts.push(heading.to_owned());
            }
            if let Some(content) = text_of(list, "content") {
                parts.push(html_to_text(content));
            }
        }
    }
    if let Some(additional) = text_of(job, "additionalPlain") {
        parts.push(additional.to_owned());
    }
    let categories = job.get("categories");
    Some(Posting {
        id: id_of(job)?,
        title: text_of(job, "text")?.to_owned(),
        url: text_of(job, "hostedUrl")?.to_owned(),
        text: parts.join("\n"),
        location: categories
            .and_then(|c| text_of(c, "location"))
            .map(str::to_owned),
        team: categories
            .and_then(|c| text_of(c, "team"))
            .map(str::to_owned),
        contract: categories
            .and_then(|c| text_of(c, "commitment"))
            .map(str::to_owned),
        remote: text_of(job, "workplaceType").map(|kind| kind.eq_ignore_ascii_case("remote")),
        observed_at: job
            .get("createdAt")
            .and_then(Value::as_i64)
            .and_then(DateTime::from_timestamp_millis),
    })
}

fn ashby(job: &Value) -> Option<Posting> {
    if job.get("isListed").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let description = match text_of(job, "descriptionPlain") {
        Some(plain) => plain.to_owned(),
        None => text_of(job, "descriptionHtml")
            .map(html_to_text)
            .unwrap_or_default(),
    };
    Some(Posting {
        id: id_of(job)?,
        title: text_of(job, "title")?.to_owned(),
        url: text_of(job, "jobUrl")
            .or_else(|| text_of(job, "applyUrl"))?
            .to_owned(),
        text: description,
        location: text_of(job, "location").map(str::to_owned),
        team: text_of(job, "team")
            .or_else(|| text_of(job, "department"))
            .map(str::to_owned),
        contract: text_of(job, "employmentType").map(str::to_owned),
        remote: job.get("isRemote").and_then(Value::as_bool).or_else(|| {
            text_of(job, "workplaceType").map(|kind| kind.eq_ignore_ascii_case("remote"))
        }),
        observed_at: parse_time(text_of(job, "publishedAt")),
    })
}

fn postings(vendor: Vendor, body: &Value) -> Vec<&Value> {
    let list = match vendor {
        Vendor::Greenhouse | Vendor::Ashby => body.get("jobs"),
        Vendor::Lever | Vendor::LeverEu => Some(body),
    };
    list.and_then(Value::as_array)
        .map(|jobs| jobs.iter().collect())
        .unwrap_or_default()
}

pub fn parse_board(
    board: &Board,
    body: &Value,
    domain: &NormalizedDomain,
    now: DateTime<Utc>,
) -> Vec<Candidate> {
    let Ok(kind) = SignalKind::parse("job_offer") else {
        return Vec::new();
    };
    let mut seen: HashSet<String> = HashSet::new();
    let mut candidates = Vec::new();
    for job in postings(board.vendor, body) {
        let posting = match board.vendor {
            Vendor::Greenhouse => greenhouse(job),
            Vendor::Lever | Vendor::LeverEu => lever(job),
            Vendor::Ashby => ashby(job),
        };
        let Some(posting) = posting else {
            continue;
        };
        if !seen.insert(posting.id.clone()) {
            continue;
        }
        let Ok(provenance) = Provenance::parse(&posting.url) else {
            continue;
        };
        candidates.push(Candidate {
            domain: domain.clone(),
            source: SourceName::Ats,
            kind: kind.clone(),
            title: posting.title,
            text: bounded(&posting.text),
            provenance,
            observed_at: posting.observed_at.unwrap_or(now),
            details: json!({
                "ats": board.vendor.label(),
                "board": board.slug,
                "job_id": posting.id,
                "location": posting.location,
                "team": posting.team,
                "contract": posting.contract,
                "remote": posting.remote,
                "posted_at": posting.observed_at.map(|time| time.to_rfc3339()),
            }),
        });
    }
    candidates
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagePosting {
    pub title: String,
    pub url: Url,
    pub location: Option<String>,
    pub contract: Option<String>,
    pub remote: bool,
    pub posted_at: Option<DateTime<Utc>>,
}

fn has_type(value: &Value, wanted: &str) -> bool {
    match value.get("@type") {
        Some(Value::String(kind)) => kind == wanted,
        Some(Value::Array(kinds)) => kinds.iter().any(|kind| kind.as_str() == Some(wanted)),
        _ => false,
    }
}

fn place_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.trim().to_owned()).filter(|text| !text.is_empty()),
        Value::Object(_) => text_of(value, "name").map(str::to_owned),
        _ => None,
    }
}

fn location_of(value: &Value) -> Option<String> {
    let place = match value {
        Value::Array(places) => places.first()?,
        other => other,
    };
    let address = place.get("address").unwrap_or(place);
    if let Some(text) = place_text(address).filter(|_| address.is_string()) {
        return Some(text);
    }
    let parts: Vec<String> = ["addressLocality", "addressRegion", "addressCountry"]
        .iter()
        .filter_map(|key| address.get(*key).and_then(place_text))
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

fn contract_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.trim().to_owned()).filter(|text| !text.is_empty()),
        Value::Array(items) => {
            let parts: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            (!parts.is_empty()).then(|| parts.join(", "))
        }
        _ => None,
    }
}

fn posted_on(raw: Option<&str>) -> Option<DateTime<Utc>> {
    let raw = raw?.trim();
    parse_time(Some(raw)).or_else(|| {
        NaiveDate::parse_from_str(raw.get(..10).unwrap_or(raw), "%Y-%m-%d")
            .ok()
            .and_then(|day| day.and_hms_opt(0, 0, 0))
            .map(|naive| naive.and_utc())
    })
}

fn job_posting(value: &Value, page: &Url) -> Option<PagePosting> {
    let title = text_of(value, "title")?;
    let url = text_of(value, "url")
        .and_then(|raw| page.join(raw).ok())
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .unwrap_or_else(|| page.clone());
    let remote = text_of(value, "jobLocationType")
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TELECOMMUTE"))
        || value.get("applicantLocationRequirements").is_some();
    Some(PagePosting {
        title: decode_entities(title),
        url,
        location: value.get("jobLocation").and_then(location_of),
        contract: contract_text(value.get("employmentType")),
        remote,
        posted_at: posted_on(text_of(value, "datePosted")),
    })
}

fn collect_postings(value: &Value, page: &Url, depth: usize, found: &mut Vec<PagePosting>) {
    if depth > MAX_JSON_LD_DEPTH {
        return;
    }
    match value {
        Value::Array(items) => {
            for item in items {
                collect_postings(item, page, depth + 1, found);
            }
        }
        Value::Object(map) => {
            if has_type(value, "JobPosting") {
                if let Some(posting) = job_posting(value, page) {
                    found.push(posting);
                }
                return;
            }
            for key in ["@graph", "itemListElement", "item"] {
                if let Some(nested) = map.get(key) {
                    collect_postings(nested, page, depth + 1, found);
                }
            }
        }
        _ => {}
    }
}

pub fn page_postings(html: &str, page: &Url) -> Vec<PagePosting> {
    let mut found: Vec<PagePosting> = Vec::new();
    for captures in JSON_LD.captures_iter(html) {
        let Some(raw) = captures.get(1) else {
            continue;
        };
        let Ok(value) = from_str::<Value>(raw.as_str().trim()) else {
            continue;
        };
        collect_postings(&value, page, 0, &mut found);
    }
    let mut seen: HashSet<(String, String)> = HashSet::new();
    found.retain(|posting| seen.insert((posting.title.clone(), posting.url.to_string())));
    found
}
