use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, ContactFinder, ContactMethod, FetchError, FetchPurpose, FetchRequest, Fetcher,
    FoundAddress, NormalizedDomain, Provenance, SourceError, SourceName, UnitInterval,
};
use tracing::{debug, info};
use url::Url;

use crate::link_check::{LinkChecker, LinkOrigin, LinkVerdict, fetch_waiting, rejection_of};

pub mod extract;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactPageSettings {
    pub paths: Vec<String>,
    pub max_pages: usize,
    pub max_rate_limit_wait: Duration,
}

pub struct PublicPageFinder<F: Fetcher> {
    fetcher: Arc<F>,
    settings: ContactPageSettings,
}

impl<F: Fetcher> PublicPageFinder<F> {
    pub fn new(fetcher: Arc<F>, settings: ContactPageSettings) -> Self {
        Self { fetcher, settings }
    }
}

fn page_url(domain: &NormalizedDomain, path: &str) -> Option<Url> {
    let separator = if path.starts_with('/') { "" } else { "/" };
    Url::parse(&format!("https://{}{separator}{path}", domain.as_str())).ok()
}

fn origin_of(path: &str) -> LinkOrigin {
    if path.is_empty() || path == "/" {
        LinkOrigin::Home
    } else {
        LinkOrigin::Guessed
    }
}

fn merge(found: &mut Vec<FoundAddress>, candidate: FoundAddress) {
    match found
        .iter_mut()
        .find(|existing| existing.address == candidate.address)
    {
        Some(existing) if existing.confidence < candidate.confidence => {
            existing.confidence = candidate.confidence;
        }
        Some(_) => {}
        None => found.push(candidate),
    }
}

impl<F: Fetcher> ContactFinder for PublicPageFinder<F> {
    fn name(&self) -> SourceName {
        SourceName::PublicPage
    }

    async fn find(
        &self,
        domain: &NormalizedDomain,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<FoundAddress>, SourceError> {
        let mut found: Vec<FoundAddress> = Vec::new();
        let mut checker = LinkChecker::new(
            self.fetcher.as_ref(),
            SourceName::PublicPage,
            self.settings.max_rate_limit_wait,
        );
        for path in self.settings.paths.iter().take(self.settings.max_pages) {
            let Some(url) = page_url(domain, path) else {
                debug!(domain = domain.as_str(), path, "invalid contact page path");
                continue;
            };
            let request = FetchRequest {
                url,
                purpose: FetchPurpose::Crawl,
                source: SourceName::PublicPage,
            };
            let response = match fetch_waiting(
                self.fetcher.as_ref(),
                &request,
                self.settings.max_rate_limit_wait,
            )
            .await
            {
                Ok(response) => response,
                Err(FetchError::RateLimited { retry_after }) => {
                    return Err(SourceError::RateLimited { retry_after });
                }
                Err(error) => {
                    if let Some(reason) = rejection_of(&error) {
                        checker.reject(reason);
                    }
                    debug!(url = %request.url, %error, "contact page skipped");
                    continue;
                }
            };
            let verdict = match checker
                .confirm(&request.url, origin_of(path), &response)
                .await
            {
                Ok(verdict) => verdict,
                Err(FetchError::RateLimited { retry_after }) => {
                    return Err(SourceError::RateLimited { retry_after });
                }
                Err(error) => return Err(SourceError::Fetch(error)),
            };
            if let LinkVerdict::NotFound(reason) = verdict {
                debug!(url = %request.url, status = response.status, reason = reason.as_str(), "contact page not confirmed");
                continue;
            }
            let Ok(provenance) = Provenance::parse(response.final_url.as_str()) else {
                continue;
            };
            for item in extract::extract_addresses(&response.text()) {
                let Ok(confidence) = UnitInterval::new(item.confidence) else {
                    continue;
                };
                merge(
                    &mut found,
                    FoundAddress {
                        address: item.address,
                        provenance: provenance.clone(),
                        method: ContactMethod::PublicPage,
                        confidence,
                    },
                );
            }
        }
        let unconfirmed = checker.unconfirmed();
        if unconfirmed.total() > 0 {
            info!(
                domain = domain.as_str(),
                unconfirmed_links = unconfirmed.total(),
                unconfirmed_by_reason = %unconfirmed,
                "contact pages not confirmed were ignored"
            );
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests;
