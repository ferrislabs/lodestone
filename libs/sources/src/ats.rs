use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, AtsTargets, Candidate, FetchError, FetchPurpose, FetchRequest, FetchResponse,
    Fetcher, LinkRejection, NormalizedDomain, ProfileId, SourceError, SourceName, UnconfirmedLinks,
    triage::CandidateSource,
};
use serde_json::Value;
use tracing::{debug, info};
use url::Url;

use self::{
    discover::{Board, careers_links, extract_boards},
    jobs::{PagePosting, page_postings},
};
use crate::link_check::{LinkChecker, LinkOrigin, LinkVerdict, fetch_waiting, rejection_of};

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
    pub postings: Vec<PagePosting>,
    pub unconfirmed: UnconfirmedLinks,
}

pub struct AtsScanner<F: Fetcher> {
    fetcher: Arc<F>,
    max_career_paths: usize,
    max_rate_limit_wait: Duration,
}

fn page_url(domain: &NormalizedDomain, path: &str) -> Option<Url> {
    Url::parse(&format!("https://{}{path}", domain.as_str())).ok()
}

enum Visit {
    Confirmed(FetchResponse),
    Shell(FetchResponse),
    Skipped,
}

impl<F: Fetcher> AtsScanner<F> {
    pub fn new(fetcher: Arc<F>, max_career_paths: usize, max_rate_limit_wait: Duration) -> Self {
        Self {
            fetcher,
            max_career_paths,
            max_rate_limit_wait,
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
        match fetch_waiting(self.fetcher.as_ref(), &request, self.max_rate_limit_wait).await {
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

    async fn visit(
        &self,
        checker: &mut LinkChecker<'_, F>,
        url: &Url,
        origin: LinkOrigin,
    ) -> Result<Visit, SourceError> {
        let request = FetchRequest {
            url: url.clone(),
            purpose: FetchPurpose::Crawl,
            source: SourceName::Ats,
        };
        let response =
            match fetch_waiting(self.fetcher.as_ref(), &request, self.max_rate_limit_wait).await {
                Ok(response) => response,
                Err(FetchError::RateLimited { retry_after }) => {
                    return Err(SourceError::RateLimited { retry_after });
                }
                Err(error) => {
                    if let Some(reason) = rejection_of(&error) {
                        checker.reject(reason);
                    }
                    debug!(%url, %error, "career page skipped");
                    return Ok(Visit::Skipped);
                }
            };
        if response.truncated {
            debug!(%url, "career page exceeded the body cap");
            return Ok(Visit::Skipped);
        }
        let verdict =
            checker
                .confirm(url, origin, &response)
                .await
                .map_err(|error| match error {
                    FetchError::RateLimited { retry_after } => {
                        SourceError::RateLimited { retry_after }
                    }
                    other => SourceError::Fetch(other),
                })?;
        Ok(match verdict {
            LinkVerdict::Confirmed(_) => Visit::Confirmed(response),
            LinkVerdict::NotFound(LinkRejection::EmptyContent) => Visit::Shell(response),
            LinkVerdict::NotFound(reason) => {
                debug!(%url, reason = reason.as_str(), "career page not confirmed");
                Visit::Skipped
            }
        })
    }

    async fn boards_of(
        &self,
        checker: &mut LinkChecker<'_, F>,
        scan: &mut DomainScan,
        url: Url,
        origin: LinkOrigin,
    ) -> Result<Option<FetchResponse>, SourceError> {
        Ok(match self.visit(checker, &url, origin).await? {
            Visit::Confirmed(response) => {
                for posting in page_postings(&response.text(), &url) {
                    if !scan.postings.contains(&posting) {
                        scan.postings.push(posting);
                    }
                }
                scan.pages.push(url);
                Some(response)
            }
            Visit::Shell(response) => Some(response),
            Visit::Skipped => None,
        })
    }

    pub async fn scan(
        &self,
        domain: &NormalizedDomain,
        now: DateTime<Utc>,
    ) -> Result<DomainScan, SourceError> {
        let mut scan = DomainScan::default();
        let mut checker = LinkChecker::new(
            self.fetcher.as_ref(),
            SourceName::Ats,
            self.max_rate_limit_wait,
        );
        let mut boards: Vec<Board> = Vec::new();
        let mut linked: Vec<Url> = Vec::new();
        if let Some(home) = page_url(domain, "/")
            && let Some(response) = self
                .boards_of(&mut checker, &mut scan, home, LinkOrigin::Home)
                .await?
        {
            let text = response.text();
            boards = extract_boards(&text);
            linked = careers_links(&text, &response.final_url, domain);
        }
        if boards.is_empty() {
            let mut pages: Vec<(Url, LinkOrigin)> = linked
                .into_iter()
                .map(|url| (url, LinkOrigin::Linked))
                .collect();
            for path in CAREER_PATHS {
                if let Some(url) = page_url(domain, path)
                    && !pages.iter().any(|(known, _)| *known == url)
                {
                    pages.push((url, LinkOrigin::Guessed));
                }
            }
            pages.truncate(self.max_career_paths);
            for (url, origin) in pages {
                if let Some(response) = self.boards_of(&mut checker, &mut scan, url, origin).await?
                {
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
        scan.unconfirmed = checker.unconfirmed();
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

    async fn collect(
        &self,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        let revisit = chrono::Duration::from_std(self.settings.revisit_interval)
            .map_err(|_| store_error("revisit interval out of range"))?;
        let enriched: Vec<ProfileId> = profiles
            .iter()
            .filter(|active| active.revision.definition.definition().reads_company_jobs())
            .map(|active| active.profile.id)
            .collect();
        let domains = self
            .targets
            .list_due(&enriched, now - revisit, self.settings.batch_size)
            .await
            .map_err(store_error)?;
        let mut candidates = Vec::new();
        let (mut scanned, mut with_board, mut jobs) = (0u64, 0u64, 0u64);
        let mut unconfirmed_links = UnconfirmedLinks::default();
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
            unconfirmed_links.merge(&scan.unconfirmed);
            with_board += u64::from(!scan.boards.is_empty());
            for board in scan.boards {
                jobs += u64::try_from(board.candidates.len()).unwrap_or(u64::MAX);
                candidates.extend(board.candidates);
            }
        }
        info!(
            due = domains.len(),
            scanned,
            with_board,
            jobs,
            unconfirmed_links = unconfirmed_links.total(),
            unconfirmed_by_reason = %unconfirmed_links,
            "ats scan finished"
        );
        Ok(candidates)
    }
}

#[cfg(test)]
mod tests;
