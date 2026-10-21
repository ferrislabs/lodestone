use std::{
    cmp::Reverse,
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration as StdDuration,
};

use chrono::{DateTime, Utc};
use fetch::{ApiFetcher, ApiRequest};
use lodestone_core::{
    ActiveProfile, Candidate, FetchError, FetchResponse, HACKER_NEWS_THREAD_TITLE, Provenance,
    SignalKind, SourceError, SourceName, triage::CandidateSource,
};
use serde_json::{Value, from_slice, json};
use tokio::time::sleep;
use tracing::{debug, info};
use url::Url;

use crate::{endpoint::Endpoint, search};

mod comment;

use comment::parse as parse_comment;

pub const THREAD_LIST_SIZE: usize = 10;
pub const PAGE_SIZE: usize = 100;
pub const TEXT_MAX_CHARS: usize = 4000;
const ITEM_URL: &str = "https://news.ycombinator.com/item?id=";
const MAX_RATE_LIMIT_WAIT: StdDuration = StdDuration::from_secs(75);
const MAX_THROTTLE_RETRIES: u8 = 2;
const DEFAULT_RETRY_AFTER: StdDuration = StdDuration::from_secs(60);

#[derive(Debug, Clone)]
pub struct HackerNewsSettings {
    pub base_url: Endpoint,
    pub max_pages: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HackerNewsStats {
    pub threads: u64,
    pub queries: u64,
    pub requests: u64,
    pub records: u64,
    pub duplicates: u64,
    pub replies: u64,
    pub candidates: u64,
    pub skipped_no_domain: u64,
    pub skipped_malformed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub created: i64,
}

#[derive(Debug, PartialEq, Eq)]
enum Skip {
    Malformed,
    NoDomain,
    Reply,
}

pub struct HackerNewsSource<F: ApiFetcher> {
    fetcher: Arc<F>,
    settings: HackerNewsSettings,
    first_run: AtomicBool,
}

fn text_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn id_of(value: &Value, key: &str) -> Option<String> {
    match value.get(key)? {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

pub fn select_threads(list: &Value, count: usize) -> Vec<Thread> {
    let prefix = HACKER_NEWS_THREAD_TITLE.to_lowercase();
    let mut threads: Vec<Thread> = list
        .get("hits")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|hit| {
            let title = text_of(hit, "title")?;
            if !title.to_lowercase().starts_with(&prefix) {
                return None;
            }
            Some(Thread {
                id: id_of(hit, "objectID")?,
                title: title.to_owned(),
                created: hit.get("created_at_i").and_then(Value::as_i64).unwrap_or(0),
            })
        })
        .collect();
    threads.sort_by_key(|thread| Reverse(thread.created));
    threads.truncate(count);
    threads
}

fn observed_at(hit: &Value) -> Option<DateTime<Utc>> {
    text_of(hit, "created_at")
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|parsed| parsed.with_timezone(&Utc))
        .or_else(|| {
            hit.get("created_at_i")
                .and_then(Value::as_i64)
                .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
        })
}

fn parse_hit(hit: &Value, thread: &Thread) -> Result<Candidate, Skip> {
    if id_of(hit, "parent_id").is_some_and(|parent| parent != thread.id) {
        return Err(Skip::Reply);
    }
    let object_id = id_of(hit, "objectID").ok_or(Skip::Malformed)?;
    let html = text_of(hit, "comment_text").ok_or(Skip::Malformed)?;
    let observed_at = observed_at(hit).ok_or(Skip::Malformed)?;
    let parsed = parse_comment(html).ok_or(Skip::Malformed)?;
    let domain = parsed.domain.clone().ok_or(Skip::NoDomain)?;
    let provenance =
        Provenance::parse(&format!("{ITEM_URL}{object_id}")).map_err(|_| Skip::Malformed)?;
    let kind = SignalKind::parse("job_offer").map_err(|_| Skip::Malformed)?;
    let contracts: Vec<&str> = parsed.contracts.iter().map(|hint| hint.as_str()).collect();
    Ok(Candidate {
        domain,
        source: SourceName::HackerNewsHiring,
        kind,
        title: parsed.title(),
        text: parsed.text(TEXT_MAX_CHARS),
        provenance,
        observed_at,
        details: json!({
            "object_id": object_id,
            "thread_id": thread.id,
            "thread": thread.title,
            "company": parsed.company,
            "role": parsed.role,
            "location": parsed.location,
            "work_mode": parsed.work_mode.as_str(),
            "contract_hints": contracts,
            "country": parsed.country,
            "author": text_of(hit, "author"),
        }),
    })
}

fn upstream(message: impl Into<String>) -> SourceError {
    SourceError::Upstream(message.into())
}

fn retry_after(response: &FetchResponse) -> StdDuration {
    response
        .header("retry-after")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map_or(DEFAULT_RETRY_AFTER, StdDuration::from_secs)
}

fn endpoint_url(base: &Url, segment: &str, pairs: &[(&str, &str)]) -> Result<Url, SourceError> {
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|_| upstream("hacker news base url cannot carry a path"))?
        .pop_if_empty()
        .push(segment);
    url.query_pairs_mut().extend_pairs(pairs);
    Ok(url)
}

impl<F: ApiFetcher> HackerNewsSource<F> {
    pub fn new(fetcher: Arc<F>, settings: HackerNewsSettings) -> Self {
        Self {
            fetcher,
            settings,
            first_run: AtomicBool::new(true),
        }
    }

    async fn get(&self, url: Url) -> Result<Value, SourceError> {
        let request = ApiRequest {
            url,
            source: SourceName::HackerNewsHiring,
            bearer: None,
            headers: Vec::new(),
            form: None,
        };
        let mut throttled: u8 = 0;
        loop {
            let response = match self.fetcher.call(&request).await {
                Ok(response) => response,
                Err(FetchError::RateLimited { retry_after })
                    if throttled < MAX_THROTTLE_RETRIES && retry_after <= MAX_RATE_LIMIT_WAIT =>
                {
                    throttled += 1;
                    sleep(retry_after).await;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            if response.status == 429 {
                let wait = retry_after(&response);
                if throttled >= MAX_THROTTLE_RETRIES || wait > MAX_RATE_LIMIT_WAIT {
                    return Err(SourceError::RateLimited { retry_after: wait });
                }
                throttled += 1;
                sleep(wait).await;
                continue;
            }
            if response.status != 200 {
                return Err(upstream(format!(
                    "hacker news answered {}",
                    response.status
                )));
            }
            if response.truncated {
                return Err(upstream("hacker news response exceeded the body cap"));
            }
            return from_slice(&response.body)
                .map_err(|_| upstream("hacker news response is not json"));
        }
    }

    async fn threads(
        &self,
        base: &Url,
        stats: &mut HackerNewsStats,
    ) -> Result<Vec<Thread>, SourceError> {
        let wanted = if self.first_run.load(Ordering::Relaxed) {
            2
        } else {
            1
        };
        let url = endpoint_url(
            base,
            "search_by_date",
            &[
                ("tags", "story,author_whoishiring"),
                ("hitsPerPage", &THREAD_LIST_SIZE.to_string()),
            ],
        )?;
        stats.requests += 1;
        let list = self.get(url).await?;
        let threads = select_threads(&list, wanted);
        if threads.is_empty() {
            return Err(upstream("no \"Who is hiring?\" thread found"));
        }
        stats.threads = threads.len() as u64;
        Ok(threads)
    }

    pub async fn collect_with_stats(
        &self,
        profiles: &[ActiveProfile],
    ) -> Result<(Vec<Candidate>, HackerNewsStats), SourceError> {
        let mut stats = HackerNewsStats::default();
        let keywords = search::keyword_union(profiles, SourceName::HackerNewsHiring);
        if keywords.is_empty() {
            return Ok((Vec::new(), stats));
        }
        let base = self.settings.base_url.resolve().await?;
        let threads = self.threads(&base, &mut stats).await?;
        let mut seen: HashSet<String> = HashSet::new();
        let mut candidates = Vec::new();
        for thread in &threads {
            let tag = format!("comment,story_{}", thread.id);
            for keyword in &keywords {
                stats.queries += 1;
                for page in 0..self.settings.max_pages {
                    let url = endpoint_url(
                        &base,
                        "search",
                        &[
                            ("query", keyword),
                            ("tags", &tag),
                            ("hitsPerPage", &PAGE_SIZE.to_string()),
                            ("page", &page.to_string()),
                        ],
                    )?;
                    stats.requests += 1;
                    let body = self.get(url).await?;
                    let hits = body
                        .get("hits")
                        .and_then(Value::as_array)
                        .ok_or_else(|| upstream("hacker news response has no hits"))?;
                    for hit in hits {
                        stats.records += 1;
                        let object_id = id_of(hit, "objectID").unwrap_or_default();
                        if !object_id.is_empty() && !seen.insert(object_id.clone()) {
                            stats.duplicates += 1;
                            continue;
                        }
                        match parse_hit(hit, thread) {
                            Ok(candidate) => {
                                stats.candidates += 1;
                                candidates.push(candidate);
                            }
                            Err(Skip::Reply) => stats.replies += 1,
                            Err(Skip::NoDomain) => {
                                stats.skipped_no_domain += 1;
                                debug!(object_id, "hacker news post skipped, no company domain");
                            }
                            Err(Skip::Malformed) => {
                                stats.skipped_malformed += 1;
                                debug!(object_id, "hacker news post skipped, malformed record");
                            }
                        }
                    }
                    let total_pages = body.get("nbPages").and_then(Value::as_u64);
                    let more = match total_pages {
                        Some(total) => u64::from(page) + 1 < total,
                        None => hits.len() >= PAGE_SIZE,
                    };
                    if hits.is_empty() || !more {
                        break;
                    }
                }
            }
        }
        self.first_run.store(false, Ordering::Relaxed);
        info!(
            threads = stats.threads,
            queries = stats.queries,
            requests = stats.requests,
            records = stats.records,
            duplicates = stats.duplicates,
            replies = stats.replies,
            candidates = stats.candidates,
            skipped_no_domain = stats.skipped_no_domain,
            skipped_malformed = stats.skipped_malformed,
            "hacker news fetch finished"
        );
        Ok((candidates, stats))
    }
}

impl<F: ApiFetcher> CandidateSource for HackerNewsSource<F> {
    fn name(&self) -> SourceName {
        SourceName::HackerNewsHiring
    }

    async fn collect(
        &self,
        profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        self.collect_with_stats(profiles)
            .await
            .map(|(candidates, _)| candidates)
    }
}
