use std::{collections::HashSet, sync::LazyLock};

use chrono::{DateTime, Utc};
use lodestone_core::{Candidate, NormalizedDomain, Provenance, SignalKind, SourceName};
use regex::Regex;
use serde_json::{Value, json};

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
            }),
        });
    }
    candidates
}
