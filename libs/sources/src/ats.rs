use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, AtsTargets, Candidate, FetchError, FetchPurpose, FetchRequest, FetchResponse,
    Fetcher, NormalizedDomain, SourceError, SourceName,
    definition::{KeywordSource, ProfileDefinition},
    triage::CandidateSource,
};
use serde_json::Value;
use tracing::{debug, info};
use url::Url;

use self::discover::{Board, careers_links, extract_boards};

pub mod discover;
pub mod jobs;

pub const CAREER_PATHS: [&str; 6] = [
    "/careers",
    "/jobs",
    "/carrieres",
    "/recrutement",
    "/en/careers",
    "/join-us",
];
pub const MAX_BOARDS_PER_DOMAIN: usize = 3;
const FETCH_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtsSettings {
    pub batch_size: u32,
    pub revisit_interval: Duration,
    pub max_career_paths: usize,
    pub max_rate_limit_wait: Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoardJobs {
    pub board: Board,
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DomainScan {
    pub pages: Vec<Url>,
    pub boards: Vec<BoardJobs>,
}

pub struct AtsScanner<F: Fetcher> {
    fetcher: Arc<F>,
    max_career_paths: usize,
    max_rate_limit_wait: Duration,
}

fn page_url(domain: &NormalizedDomain, path: &str) -> Option<Url> {
    Url::parse(&format!("https://{}{path}", domain.as_str())).ok()
}

fn is_parsable(response: &FetchResponse) -> bool {
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

impl<F: Fetcher> AtsScanner<F> {
    pub fn new(fetcher: Arc<F>, max_career_paths: usize, max_rate_limit_wait: Duration) -> Self {
        Self {
            fetcher,
            max_career_paths,
            max_rate_limit_wait,
        }
    }

    async fn fetch_waiting(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let mut attempt = 1;
        loop {
            match self.fetcher.fetch(request).await {
                Err(FetchError::RateLimited { retry_after })
                    if attempt < FETCH_ATTEMPTS && retry_after <= self.max_rate_limit_wait =>
                {
                    attempt += 1;
                    tokio::time::sleep(retry_after).await;
                }
                other => return other,
            }
        }
    }

    async fn fetch_ok(
        &self,
        url: Url,
        purpose: FetchPurpose,
    ) -> Result<Option<FetchResponse>, SourceError> {
        let request = FetchRequest {
            url,
            purpose,
            source: SourceName::Ats,
        };
        match self.fetch_waiting(&request).await {
            Ok(response) if response.status == 200 && !response.truncated => Ok(Some(response)),
            Ok(response) => {
                debug!(url = %request.url, status = response.status, "ats fetch ignored");
                Ok(None)
            }
            Err(FetchError::RateLimited { retry_after }) => {
                Err(SourceError::RateLimited { retry_after })
            }
            Err(error) => {
                debug!(url = %request.url, %error, "ats fetch skipped");
                Ok(None)
            }
        }
    }

    async fn page(&self, url: Url) -> Result<Option<FetchResponse>, SourceError> {
        Ok(self
            .fetch_ok(url, FetchPurpose::Crawl)
            .await?
            .filter(is_parsable))
    }

    pub async fn scan(
        &self,
        domain: &NormalizedDomain,
        now: DateTime<Utc>,
    ) -> Result<DomainScan, SourceError> {
        let mut scan = DomainScan::default();
        let mut boards: Vec<Board> = Vec::new();
        let mut linked: Vec<Url> = Vec::new();
        if let Some(home) = page_url(domain, "/") {
            scan.pages.push(home.clone());
            if let Some(response) = self.page(home).await? {
                let text = response.text();
                boards = extract_boards(&text);
                linked = careers_links(&text, &response.final_url, domain);
            }
        }
        if boards.is_empty() {
            let mut pages = linked;
            for path in CAREER_PATHS {
                if let Some(url) = page_url(domain, path)
                    && !pages.contains(&url)
                {
                    pages.push(url);
                }
            }
            pages.truncate(self.max_career_paths);
            for url in pages {
                scan.pages.push(url.clone());
                if let Some(response) = self.page(url).await? {
                    boards = extract_boards(&response.text());
                    if !boards.is_empty() {
                        break;
                    }
                }
            }
        }
        for board in boards.into_iter().take(MAX_BOARDS_PER_DOMAIN) {
            let Some(url) = board.api_url() else {
                continue;
            };
            let Some(response) = self.fetch_ok(url, FetchPurpose::Api).await? else {
                continue;
            };
            let Ok(body) = serde_json::from_slice::<Value>(&response.body) else {
                debug!(board = %board.slug, "ats board answer is not json");
                continue;
            };
            let candidates = jobs::parse_board(&board, &body, domain, now);
            scan.boards.push(BoardJobs { board, candidates });
        }
        Ok(scan)
    }
}

pub struct AtsSource<F: Fetcher, T: AtsTargets> {
    scanner: AtsScanner<F>,
    targets: T,
    settings: AtsSettings,
}

impl<F: Fetcher, T: AtsTargets> AtsSource<F, T> {
    pub fn new(fetcher: Arc<F>, targets: T, settings: AtsSettings) -> Self {
        Self {
            scanner: AtsScanner::new(
                fetcher,
                settings.max_career_paths,
                settings.max_rate_limit_wait,
            ),
            targets,
            settings,
        }
    }
}

fn store_error(error: impl std::fmt::Display) -> SourceError {
    SourceError::Upstream(format!("ats targets: {error}"))
}

impl<F: Fetcher, T: AtsTargets> CandidateSource for AtsSource<F, T> {
    fn name(&self) -> SourceName {
        SourceName::Ats
    }

    fn rules<'a>(&self, profile: &'a ProfileDefinition) -> Option<&'a KeywordSource> {
        profile.ats()
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        let revisit = chrono::Duration::from_std(self.settings.revisit_interval)
            .map_err(|_| store_error("revisit interval out of range"))?;
        let domains = self
            .targets
            .list_due(now - revisit, self.settings.batch_size)
            .await
            .map_err(store_error)?;
        let mut candidates = Vec::new();
        let (mut scanned, mut with_board, mut jobs) = (0u64, 0u64, 0u64);
        for domain in &domains {
            let scan = match self.scanner.scan(domain, now).await {
                Ok(scan) => scan,
                Err(SourceError::RateLimited { retry_after }) => {
                    info!(%domain, ?retry_after, "ats scan stopped, host rate limited");
                    break;
                }
                Err(error) => {
                    debug!(%domain, %error, "ats scan skipped a domain");
                    continue;
                }
            };
            self.targets
                .mark_checked(std::slice::from_ref(domain), now)
                .await
                .map_err(store_error)?;
            scanned += 1;
            with_board += u64::from(!scan.boards.is_empty());
            for board in scan.boards {
                jobs += u64::try_from(board.candidates.len()).unwrap_or(u64::MAX);
                candidates.extend(board.candidates);
            }
        }
        info!(
            due = domains.len(),
            scanned, with_board, jobs, "ats scan finished"
        );
        Ok(candidates)
    }
}

#[cfg(test)]
mod tests;
