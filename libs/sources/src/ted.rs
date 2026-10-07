use std::{collections::HashSet, sync::Arc};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use fetch::{PostFetcher, PostRequest};
use lodestone_core::{
    ActiveProfile, Candidate, NormalizedDomain, Provenance, SignalKind, SourceError, SourceName,
    alpha3_of, definition::SearchAgent, triage::CandidateSource,
};
use serde_json::{Value, json};
use tracing::{debug, info};
use url::Url;

use crate::{
    buyer_domain::domain_of_url,
    endpoint::Endpoint,
    search::{self, query_groups},
};

pub const PAGE_SIZE: usize = 100;
pub const WINDOW_CAP: usize = 15_000;
pub const TEXT_MAX_CHARS: usize = 4000;
const FALLBACK_NOTICE_URL: &str = "https://ted.europa.eu/en/notice/-/detail/";
const TITLE_SEPARATOR: &str = " \u{2013} ";
const PREFERRED_LANGUAGES: [&str; 2] = ["fra", "eng"];
const FIELDS: [&str; 10] = [
    "publication-number",
    "notice-title",
    "title-proc",
    "description-lot",
    "deadline-receipt-tender-date-lot",
    "notice-type",
    "buyer-name",
    "organisation-internet-address-buyer",
    "publication-date",
    "buyer-country",
];

#[derive(Debug, Clone)]
pub struct TedSettings {
    pub base_url: Endpoint,
    pub lookback_days: u32,
    pub max_pages: u32,
    pub buyer_countries: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TedStats {
    pub pages: u64,
    pub records: u64,
    pub total_matching: u64,
    pub candidates: u64,
    pub skipped_no_domain: u64,
    pub skipped_malformed: u64,
}

pub struct TedSource<F: PostFetcher> {
    fetcher: Arc<F>,
    settings: TedSettings,
}

pub fn keyword_union(profiles: &[ActiveProfile]) -> Vec<String> {
    search::keyword_union(profiles, SourceName::Ted)
}

pub fn native_countries(agent: &SearchAgent, defaults: &[String]) -> Vec<String> {
    if agent.filters.countries.is_empty() {
        return valid_countries(defaults);
    }
    let mut codes: Vec<String> = Vec::new();
    for country in &agent.filters.countries {
        let Some(code) = alpha3_of(country.as_str()) else {
            return Vec::new();
        };
        if !codes.iter().any(|known| known == code) {
            codes.push(code.to_owned());
        }
    }
    codes.sort();
    codes
}

fn quoted(keyword: &str) -> String {
    let escaped = keyword.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn valid_countries(countries: &[String]) -> Vec<String> {
    let mut valid: Vec<String> = Vec::new();
    for country in countries {
        let country = country.trim().to_ascii_uppercase();
        if country.len() == 3
            && country.chars().all(|letter| letter.is_ascii_uppercase())
            && !valid.contains(&country)
        {
            valid.push(country);
        }
    }
    valid
}

pub fn expert_query(
    keywords: &[String],
    countries: &[String],
    now: DateTime<Utc>,
    lookback_days: u32,
) -> String {
    let since = (now - Duration::days(i64::from(lookback_days))).date_naive();
    let terms: Vec<String> = keywords
        .iter()
        .map(|keyword| format!("FT~{}", quoted(keyword)))
        .collect();
    let mut query = format!(
        "({}) AND notice-type=cn* AND deadline-receipt-tender-date-lot>={} AND publication-date>={}",
        terms.join(" OR "),
        now.date_naive().format("%Y%m%d"),
        since.format("%Y%m%d")
    );
    match valid_countries(countries).as_slice() {
        [] => {}
        [single] => query.push_str(&format!(" AND buyer-country={single}")),
        several => query.push_str(&format!(" AND buyer-country IN ({})", several.join(" "))),
    }
    query
}

fn page_body(query: &str, page: usize) -> Value {
    json!({
        "query": query,
        "fields": FIELDS,
        "limit": PAGE_SIZE,
        "page": page,
        "scope": "ACTIVE",
    })
}

fn strings_of(value: &Value) -> Vec<&str> {
    match value {
        Value::String(text) => vec![text.as_str()],
        Value::Array(list) => list.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

fn localized(value: &Value) -> Vec<&str> {
    let Some(map) = value.as_object() else {
        return strings_of(value);
    };
    PREFERRED_LANGUAGES
        .iter()
        .filter_map(|language| map.get(*language))
        .chain(map.values())
        .map(strings_of)
        .find(|texts| !texts.is_empty())
        .unwrap_or_default()
}

fn first_localized(value: Option<&Value>) -> Option<&str> {
    localized(value?)
        .into_iter()
        .map(str::trim)
        .find(|text| !text.is_empty())
}

fn split_title(raw: &str) -> (&str, Option<&str>) {
    let mut parts = raw.splitn(3, TITLE_SEPARATOR);
    match (parts.next(), parts.next(), parts.next()) {
        (Some(_), Some(cpv), Some(title)) if !title.trim().is_empty() => {
            (title.trim(), Some(cpv.trim()))
        }
        _ => (raw.trim(), None),
    }
}

fn bounded_text(cpv: Option<&str>, descriptions: &[&str]) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let mut text = String::new();
    for part in cpv.into_iter().chain(descriptions.iter().copied()) {
        let part = part.trim();
        if part.is_empty() || parts.contains(&part) {
            continue;
        }
        parts.push(part);
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

fn date_part(raw: &str) -> Option<&str> {
    let date = raw.get(..10)?;
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    Some(date)
}

fn earliest_deadline(notice: &Value) -> Option<&str> {
    notice
        .get("deadline-receipt-tender-date-lot")
        .map(strings_of)
        .unwrap_or_default()
        .into_iter()
        .filter_map(date_part)
        .min()
}

fn buyer_domain_of(notice: &Value) -> Option<NormalizedDomain> {
    notice
        .get("organisation-internet-address-buyer")
        .map(strings_of)
        .unwrap_or_default()
        .into_iter()
        .find_map(domain_of_url)
}

fn notice_url(notice: &Value, publication_number: &str) -> String {
    let html = notice.get("links").and_then(|links| links.get("html"));
    ["FRA", "ENG"]
        .iter()
        .filter_map(|language| html.and_then(|html| html.get(*language)))
        .filter_map(Value::as_str)
        .next()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{FALLBACK_NOTICE_URL}{publication_number}"))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Skip {
    Malformed,
    NoDomain,
}

pub fn parse_notice(notice: &Value) -> Result<Candidate, Skip> {
    let publication_number = notice
        .get("publication-number")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .ok_or(Skip::Malformed)?;
    let published = notice
        .get("publication-date")
        .and_then(Value::as_str)
        .and_then(date_part)
        .ok_or(Skip::Malformed)?;
    let observed_at = NaiveDate::parse_from_str(published, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|naive| naive.and_utc())
        .ok_or(Skip::Malformed)?;
    let domain = buyer_domain_of(notice).ok_or(Skip::NoDomain)?;
    let (title, cpv) = match first_localized(notice.get("notice-title")) {
        Some(raw) => split_title(raw),
        None => (
            first_localized(notice.get("title-proc")).unwrap_or_default(),
            None,
        ),
    };
    let descriptions = notice
        .get("description-lot")
        .map(localized)
        .unwrap_or_default();
    let url = notice_url(notice, publication_number);
    let provenance = Provenance::parse(&url)
        .or_else(|_| Provenance::parse(&format!("{FALLBACK_NOTICE_URL}{publication_number}")))
        .map_err(|_| Skip::Malformed)?;
    let kind = SignalKind::parse("public_tender").map_err(|_| Skip::Malformed)?;
    Ok(Candidate {
        domain,
        source: SourceName::Ted,
        kind,
        title: title.to_owned(),
        text: bounded_text(cpv, &descriptions),
        provenance,
        observed_at,
        details: json!({
            "publication_number": publication_number,
            "buyer": first_localized(notice.get("buyer-name")),
            "deadline": earliest_deadline(notice),
            "published": published,
            "notice_url": url,
            "country": notice.get("buyer-country").map(strings_of).and_then(|list| list.first().copied()),
        }),
    })
}

fn upstream(message: impl Into<String>) -> SourceError {
    SourceError::Upstream(message.into())
}

impl<F: PostFetcher> TedSource<F> {
    pub fn new(fetcher: Arc<F>, settings: TedSettings) -> Self {
        Self { fetcher, settings }
    }

    async fn fetch_page(
        &self,
        url: &Url,
        query: &str,
        page: usize,
    ) -> Result<(Vec<Value>, u64), SourceError> {
        let body = serde_json::to_vec(&page_body(query, page))
            .map_err(|error| upstream(format!("ted request is not serializable: {error}")))?;
        let request = PostRequest {
            url: url.clone(),
            body,
            source: SourceName::Ted,
        };
        let response = self.fetcher.post(&request).await?;
        if response.status != 200 {
            return Err(upstream(format!("ted answered {}", response.status)));
        }
        if response.truncated {
            return Err(upstream("ted response exceeded the body cap"));
        }
        let mut page: Value = serde_json::from_slice(&response.body)
            .map_err(|error| upstream(format!("ted response is not json: {error}")))?;
        let total = page
            .get("totalNoticeCount")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        match page.get_mut("notices").map(Value::take) {
            Some(Value::Array(notices)) => Ok((notices, total)),
            _ => Err(upstream("ted response has no notices array")),
        }
    }

    pub async fn collect_with_stats(
        &self,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<(Vec<Candidate>, TedStats), SourceError> {
        let mut stats = TedStats::default();
        let groups = query_groups(profiles, SourceName::Ted, |agent| {
            native_countries(agent, &self.settings.buyer_countries)
        });
        if groups.is_empty() {
            return Ok((Vec::new(), stats));
        }
        let url = self.settings.base_url.resolve().await?;
        let mut seen: HashSet<String> = HashSet::new();
        let mut candidates = Vec::new();
        for group in &groups {
            let query = expert_query(
                &group.keywords,
                &group.native,
                now,
                self.settings.lookback_days,
            );
            for page in 1..=self.settings.max_pages {
                let page = usize::try_from(page).unwrap_or(usize::MAX);
                if page.saturating_mul(PAGE_SIZE) > WINDOW_CAP {
                    break;
                }
                let (notices, total) = self.fetch_page(&url, &query, page).await?;
                stats.pages += 1;
                if page == 1 {
                    stats.total_matching += total;
                }
                let fetched = notices.len();
                for notice in &notices {
                    stats.records += 1;
                    let number = notice
                        .get("publication-number")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if !number.is_empty() && !seen.insert(number.to_owned()) {
                        continue;
                    }
                    match parse_notice(notice) {
                        Ok(candidate) => {
                            stats.candidates += 1;
                            candidates.push(candidate);
                        }
                        Err(Skip::NoDomain) => {
                            stats.skipped_no_domain += 1;
                            debug!(number, "ted notice skipped, no buyer domain");
                        }
                        Err(Skip::Malformed) => {
                            stats.skipped_malformed += 1;
                            debug!(number, "ted notice skipped, malformed record");
                        }
                    }
                }
                let reached = page.saturating_mul(PAGE_SIZE);
                if fetched < PAGE_SIZE || u64::try_from(reached).unwrap_or(u64::MAX) >= total {
                    break;
                }
            }
        }
        info!(
            pages = stats.pages,
            records = stats.records,
            total_matching = stats.total_matching,
            candidates = stats.candidates,
            skipped_no_domain = stats.skipped_no_domain,
            skipped_malformed = stats.skipped_malformed,
            "ted fetch finished"
        );
        Ok((candidates, stats))
    }
}

impl<F: PostFetcher> CandidateSource for TedSource<F> {
    fn name(&self) -> SourceName {
        SourceName::Ted
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
