use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, ContactFinder, ContactMethod, FetchError, FetchPurpose, FetchRequest,
    FetchResponse, Fetcher, FoundAddress, NormalizedDomain, Provenance, SourceError, SourceName,
    UnitInterval,
};
use url::Url;

pub mod extract;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactPageSettings {
    pub paths: Vec<String>,
    pub max_pages: usize,
    pub max_rate_limit_wait: Duration,
}

const FETCH_ATTEMPTS: usize = 3;

pub struct PublicPageFinder<F: Fetcher> {
    fetcher: Arc<F>,
    settings: ContactPageSettings,
}

impl<F: Fetcher> PublicPageFinder<F> {
    pub fn new(fetcher: Arc<F>, settings: ContactPageSettings) -> Self {
        Self { fetcher, settings }
    }

    async fn fetch_waiting(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let mut attempt = 1;
        loop {
            match self.fetcher.fetch(request).await {
                Err(FetchError::RateLimited { retry_after })
                    if attempt < FETCH_ATTEMPTS
                        && retry_after <= self.settings.max_rate_limit_wait =>
                {
                    attempt += 1;
                    tokio::time::sleep(retry_after).await;
                }
                other => return other,
            }
        }
    }
}

fn page_url(domain: &NormalizedDomain, path: &str) -> Option<Url> {
    let separator = if path.starts_with('/') { "" } else { "/" };
    Url::parse(&format!("https://{}{separator}{path}", domain.as_str())).ok()
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
        for path in self.settings.paths.iter().take(self.settings.max_pages) {
            let Some(url) = page_url(domain, path) else {
                tracing::debug!(domain = domain.as_str(), path, "invalid contact page path");
                continue;
            };
            let request = FetchRequest {
                url,
                purpose: FetchPurpose::Crawl,
                source: SourceName::PublicPage,
            };
            let response = match self.fetch_waiting(&request).await {
                Ok(response) => response,
                Err(FetchError::RateLimited { retry_after }) => {
                    return Err(SourceError::RateLimited { retry_after });
                }
                Err(error) => {
                    tracing::debug!(url = %request.url, %error, "contact page skipped");
                    continue;
                }
            };
            if response.status >= 400 || !is_parsable(&response) {
                tracing::debug!(url = %request.url, status = response.status, "contact page ignored");
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
        Ok(found)
    }
}

#[cfg(test)]
mod tests;
