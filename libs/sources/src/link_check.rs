use std::{
    collections::{HashMap, HashSet, hash_map::RandomState},
    hash::BuildHasher,
    sync::LazyLock,
    time::Duration,
};

use lodestone_core::{
    FetchError, FetchPurpose, FetchRequest, FetchResponse, Fetcher, LinkRejection, SourceName,
    UnconfirmedLinks,
};
use regex::Regex;
use tokio::time::sleep;
use url::Url;

use crate::ats::jobs::{html_to_text, unescape_html};

pub const BASELINE_PATH_PREFIX: &str = "/lodestone-missing-page-";
pub const MIN_CONTENT_CHARS: usize = 20;
pub const SHORT_PAGE_CHARS: usize = 2000;
pub const SIMILARITY_THRESHOLD: f64 = 0.9;
pub const TITLE_TEXT_FLOOR: f64 = 0.5;
const FETCH_ATTEMPTS: usize = 3;
const MARKERS: &[&str] = &[
    "introuvable",
    "page non trouvée",
    "page non trouvee",
    "n'existe pas",
    "not found",
    "page doesn't exist",
    "page does not exist",
];

fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => unreachable!("static pattern {pattern} is invalid: {error}"),
    }
}

static TITLE: LazyLock<Regex> = LazyLock::new(|| compile(r"(?is)<title\b[^>]*>(.*?)</title\s*>"));
static HIDDEN: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"(?is)<head\b.*?</head\s*>|<noscript\b.*?</noscript\s*>|<template\b.*?</template\s*>|<script\b.*?</script\s*>|<style\b.*?</style\s*>",
    )
});
static CODE_404: LazyLock<Regex> = LazyLock::new(|| compile(r"\b404\b"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkOrigin {
    Home,
    Linked,
    Guessed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSnapshot {
    pub title: String,
    pub text: String,
}

impl PageSnapshot {
    pub fn of(html: &str) -> Self {
        let title = TITLE
            .captures(html)
            .and_then(|capture| capture.get(1))
            .map(|title| collapse(&unescape_html(title.as_str())))
            .unwrap_or_default();
        let visible = HIDDEN.replace_all(html, " ");
        Self {
            title,
            text: html_to_text(&visible),
        }
    }

    fn content_chars(&self) -> usize {
        self.text.chars().filter(|c| c.is_alphanumeric()).count()
    }

    fn has_marker(&self) -> bool {
        let haystack = normalise(&format!("{}\n{}", self.title, self.text));
        MARKERS.iter().any(|marker| haystack.contains(marker)) || CODE_404.is_match(&haystack)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkVerdict {
    Confirmed(PageSnapshot),
    NotFound(LinkRejection),
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalise(text: &str) -> String {
    text.to_lowercase()
        .replace(['\u{2019}', '\u{2018}', '`'], "'")
}

fn tokens(text: &str, ignored: &HashSet<String>) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty() && !ignored.contains(*token))
        .map(str::to_owned)
        .collect()
}

fn shingles(words: &[String]) -> HashSet<String> {
    if words.len() < 2 {
        return words.iter().cloned().collect();
    }
    words
        .windows(2)
        .map(|pair| format!("{} {}", pair[0], pair[1]))
        .collect()
}

pub fn similarity(left: &str, right: &str, left_path: &Url, right_path: &Url) -> f64 {
    let mut ignored = path_tokens(left_path);
    ignored.extend(path_tokens(right_path));
    let left = shingles(&tokens(left, &ignored));
    let right = shingles(&tokens(right, &ignored));
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let common = left.intersection(&right).count();
    let union = left.union(&right).count();
    common as f64 / union as f64
}

fn path_tokens(url: &Url) -> HashSet<String> {
    tokens(url.path(), &HashSet::new()).into_iter().collect()
}

fn bare_host(url: &Url) -> String {
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    match host.strip_prefix("www.") {
        Some(rest) => rest.to_owned(),
        None => host,
    }
}

fn is_root(url: &Url) -> bool {
    url.path() == "/" || url.path().is_empty()
}

pub fn is_parsable(response: &FetchResponse) -> bool {
    match response.header("content-type") {
        None => true,
        Some(value) => {
            let media = value
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            matches!(
                media.as_str(),
                "text/html" | "text/plain" | "application/xhtml+xml"
            )
        }
    }
}

pub struct LinkCheck<'a> {
    pub requested: &'a Url,
    pub origin: LinkOrigin,
}

impl LinkCheck<'_> {
    pub fn precheck(&self, response: &FetchResponse) -> Result<PageSnapshot, LinkRejection> {
        if response.status != 200 {
            return Err(LinkRejection::Status);
        }
        if self.origin != LinkOrigin::Home && is_root(&response.final_url) {
            return Err(LinkRejection::RedirectedHome);
        }
        if self.origin == LinkOrigin::Guessed
            && bare_host(&response.final_url) != bare_host(self.requested)
        {
            return Err(LinkRejection::RedirectedHome);
        }
        if !is_parsable(response) {
            return Err(LinkRejection::EmptyContent);
        }
        let page = PageSnapshot::of(&response.text());
        if page.content_chars() < MIN_CONTENT_CHARS {
            return Err(LinkRejection::EmptyContent);
        }
        if page.text.chars().count() < SHORT_PAGE_CHARS && page.has_marker() {
            return Err(LinkRejection::Soft404Marker);
        }
        Ok(page)
    }

    pub fn against(
        &self,
        response: &FetchResponse,
        page: &PageSnapshot,
        baseline: &Baseline,
    ) -> Result<(), LinkRejection> {
        if self.origin == LinkOrigin::Home {
            return Ok(());
        }
        let text = similarity(
            &page.text,
            &baseline.page.text,
            &response.final_url,
            &baseline.url,
        );
        let title = similarity(
            &page.title,
            &baseline.page.title,
            &response.final_url,
            &baseline.url,
        );
        if text >= SIMILARITY_THRESHOLD
            || (title >= SIMILARITY_THRESHOLD && text >= TITLE_TEXT_FLOOR)
        {
            Err(LinkRejection::Soft404Baseline)
        } else {
            Ok(())
        }
    }

    pub fn verdict(&self, response: &FetchResponse, baseline: Option<&Baseline>) -> LinkVerdict {
        let page = match self.precheck(response) {
            Ok(page) => page,
            Err(reason) => return LinkVerdict::NotFound(reason),
        };
        match baseline.map(|baseline| self.against(response, &page, baseline)) {
            Some(Err(reason)) => LinkVerdict::NotFound(reason),
            _ => LinkVerdict::Confirmed(page),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    pub url: Url,
    pub page: PageSnapshot,
}

impl Baseline {
    pub fn from_response(response: &FetchResponse) -> Option<Self> {
        if response.status != 200 || is_root(&response.final_url) || !is_parsable(response) {
            return None;
        }
        Some(Self {
            url: response.final_url.clone(),
            page: PageSnapshot::of(&response.text()),
        })
    }
}

pub fn baseline_url(of: &Url) -> Option<Url> {
    let token = RandomState::new().hash_one(of.as_str());
    of.join(&format!("{BASELINE_PATH_PREFIX}{token:016x}")).ok()
}

pub async fn fetch_waiting<F: Fetcher>(
    fetcher: &F,
    request: &FetchRequest,
    max_wait: Duration,
) -> Result<FetchResponse, FetchError> {
    let mut attempt = 1;
    loop {
        match fetcher.fetch(request).await {
            Err(FetchError::RateLimited { retry_after })
                if attempt < FETCH_ATTEMPTS && retry_after <= max_wait =>
            {
                attempt += 1;
                sleep(retry_after).await;
            }
            other => return other,
        }
    }
}

pub fn rejection_of(error: &FetchError) -> Option<LinkRejection> {
    match error {
        FetchError::Redirect => Some(LinkRejection::RedirectedHome),
        FetchError::Timeout | FetchError::Network(_) => Some(LinkRejection::Status),
        _ => None,
    }
}

pub struct LinkChecker<'a, F: Fetcher> {
    fetcher: &'a F,
    source: SourceName,
    max_wait: Duration,
    baselines: HashMap<String, Option<Baseline>>,
    unconfirmed: UnconfirmedLinks,
}

impl<'a, F: Fetcher> LinkChecker<'a, F> {
    pub fn new(fetcher: &'a F, source: SourceName, max_wait: Duration) -> Self {
        Self {
            fetcher,
            source,
            max_wait,
            baselines: HashMap::new(),
            unconfirmed: UnconfirmedLinks::default(),
        }
    }

    pub fn unconfirmed(&self) -> UnconfirmedLinks {
        self.unconfirmed
    }

    pub fn reject(&mut self, reason: LinkRejection) {
        self.unconfirmed.record(reason);
    }

    async fn baseline(&mut self, page: &Url) -> Result<Option<&Baseline>, FetchError> {
        let key = page.origin().ascii_serialization();
        if !self.baselines.contains_key(&key) {
            let baseline = match baseline_url(page) {
                Some(url) => {
                    let request = FetchRequest {
                        url,
                        purpose: FetchPurpose::Crawl,
                        source: self.source,
                    };
                    match fetch_waiting(self.fetcher, &request, self.max_wait).await {
                        Ok(response) => Baseline::from_response(&response),
                        Err(FetchError::RateLimited { retry_after }) => {
                            return Err(FetchError::RateLimited { retry_after });
                        }
                        Err(_) => None,
                    }
                }
                None => None,
            };
            self.baselines.insert(key.clone(), baseline);
        }
        Ok(self.baselines.get(&key).and_then(Option::as_ref))
    }

    pub async fn confirm(
        &mut self,
        requested: &Url,
        origin: LinkOrigin,
        response: &FetchResponse,
    ) -> Result<LinkVerdict, FetchError> {
        let check = LinkCheck { requested, origin };
        let page = match check.precheck(response) {
            Ok(page) => page,
            Err(reason) => {
                self.reject(reason);
                return Ok(LinkVerdict::NotFound(reason));
            }
        };
        if origin == LinkOrigin::Home {
            return Ok(LinkVerdict::Confirmed(page));
        }
        let verdict = match self.baseline(&response.final_url).await? {
            Some(baseline) => match check.against(response, &page, baseline) {
                Ok(()) => LinkVerdict::Confirmed(page),
                Err(reason) => LinkVerdict::NotFound(reason),
            },
            None => LinkVerdict::Confirmed(page),
        };
        if let LinkVerdict::NotFound(reason) = verdict {
            self.reject(reason);
        }
        Ok(verdict)
    }
}

#[cfg(test)]
mod tests;
