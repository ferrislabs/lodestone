use std::time::Duration;

use thiserror::Error;
use url::Url;

use crate::domain::values::SourceName;

pub mod ports;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FetchPurpose {
    Probe,
    Crawl,
    Api,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchRequest {
    pub url: Url,
    pub purpose: FetchPurpose,
    pub source: SourceName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub final_url: Url,
    pub truncated: bool,
}

impl FetchResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn text(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.body)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FetchError {
    #[error("URL is not an http or https URL with a host")]
    InvalidUrl,
    #[error("target resolves to a non-public address")]
    NonPublicTarget,
    #[error("robots.txt disallows this URL")]
    DisallowedByRobots,
    #[error("rate limited, retry after {retry_after:?}")]
    RateLimited { retry_after: Duration },
    #[error("request timed out")]
    Timeout,
    #[error("too many redirects or a redirect left the site")]
    Redirect,
    #[error("network error: {0}")]
    Network(String),
}
