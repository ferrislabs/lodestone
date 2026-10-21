use std::{sync::Arc, time::Duration};

use common::Secret;
use lodestone_core::{
    FetchError, FetchPurpose, FetchRequest, FetchResponse, Fetcher, NormalizedDomain, SourceName,
};
use rate_limit::{InMemoryRateLimiter, Quota, RateLimitKey, RateLimiter};
use reqwest::{
    Client, Response, StatusCode,
    header::{CONTENT_TYPE, LOCATION},
    redirect::Policy,
};
use thiserror::Error;
use tracing::{Instrument, info_span};
use url::{Host, Url, form_urlencoded};

use crate::{
    address::ensure_public,
    robots::{RobotsCache, RobotsRule},
    settings::FetchSettings,
};

const ROBOTS_MAX_REDIRECTS: u8 = 5;
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostRequest {
    pub url: Url,
    pub body: Vec<u8>,
    pub source: SourceName,
}

pub trait PostFetcher: Send + Sync {
    fn post(
        &self,
        request: &PostRequest,
    ) -> impl Future<Output = Result<FetchResponse, FetchError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRequest {
    pub url: Url,
    pub source: SourceName,
    pub bearer: Option<Secret>,
    pub headers: Vec<(String, Secret)>,
    pub form: Option<Vec<(String, Secret)>>,
}

pub trait ApiFetcher: Send + Sync {
    fn call(
        &self,
        request: &ApiRequest,
    ) -> impl Future<Output = Result<FetchResponse, FetchError>> + Send;
}

#[derive(Clone, Copy)]
struct Credentials<'a> {
    bearer: Option<&'a str>,
    headers: &'a [(&'a str, &'a str)],
}

impl Credentials<'_> {
    const NONE: Credentials<'static> = Credentials {
        bearer: None,
        headers: &[],
    };
}

#[derive(Clone, Copy)]
enum Payload<'a> {
    Empty,
    Json(&'a [u8]),
    Form(&'a [u8]),
}

impl Payload<'_> {
    fn is_empty(&self) -> bool {
        matches!(self, Payload::Empty)
    }
}

#[derive(Debug, Error)]
pub enum FetchBuildError {
    #[error("failed to build http client: {0}")]
    Client(#[from] reqwest::Error),
}

struct Inner<L> {
    client: Client,
    limiter: L,
    settings: FetchSettings,
    robots: RobotsCache,
}

pub struct PoliteFetcher<L: RateLimiter = InMemoryRateLimiter> {
    inner: Arc<Inner<L>>,
}

impl<L: RateLimiter> Clone for PoliteFetcher<L> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl PoliteFetcher<InMemoryRateLimiter> {
    pub fn new(settings: FetchSettings) -> Result<Self, FetchBuildError> {
        Self::with_limiter(settings, InMemoryRateLimiter::new())
    }
}

impl<L: RateLimiter> PoliteFetcher<L> {
    pub fn with_limiter(settings: FetchSettings, limiter: L) -> Result<Self, FetchBuildError> {
        let user_agent = format!(
            "Lodestone/{} (+{})",
            env!("CARGO_PKG_VERSION"),
            settings.contact
        );
        let client = Client::builder()
            .user_agent(user_agent)
            .redirect(Policy::none())
            .timeout(settings.request_timeout)
            .build()?;
        Ok(Self {
            inner: Arc::new(Inner {
                client,
                limiter,
                settings,
                robots: RobotsCache::default(),
            }),
        })
    }

    async fn throttle(&self, url: &Url, source: SourceName) -> Result<(), FetchError> {
        let settings = &self.inner.settings;
        let host = url.host_str().ok_or(FetchError::InvalidUrl)?.to_lowercase();
        let host_key = RateLimitKey {
            scope: "host".to_owned(),
            identifier: host,
        };
        let host_quota = Quota::per_minute(settings.requests_per_host_per_minute.get());
        self.admit(&host_key, host_quota).await?;
        if let Some((_, quota)) = settings
            .source_quotas
            .iter()
            .find(|(name, _)| *name == source)
        {
            let source_key = RateLimitKey {
                scope: "source".to_owned(),
                identifier: source.as_str().to_owned(),
            };
            self.admit(&source_key, *quota).await?;
        }
        Ok(())
    }

    async fn admit(&self, key: &RateLimitKey, quota: Quota) -> Result<(), FetchError> {
        let decision = self
            .inner
            .limiter
            .check(key, quota)
            .await
            .map_err(|_| FetchError::Network("rate limiter unavailable".to_owned()))?;
        if decision.allowed {
            Ok(())
        } else {
            Err(FetchError::RateLimited {
                retry_after: decision.retry_after.unwrap_or(DEFAULT_RETRY_AFTER),
            })
        }
    }

    async fn send(
        &self,
        url: &Url,
        payload: Payload<'_>,
        credentials: Credentials<'_>,
    ) -> Result<Response, FetchError> {
        let builder = match payload {
            Payload::Json(body) => self
                .inner
                .client
                .post(url.clone())
                .header(CONTENT_TYPE, "application/json")
                .body(body.to_vec()),
            Payload::Form(body) => self
                .inner
                .client
                .post(url.clone())
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(body.to_vec()),
            Payload::Empty => self.inner.client.get(url.clone()),
        };
        let builder = match credentials.bearer {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        };
        let builder = credentials
            .headers
            .iter()
            .fold(builder, |builder, (name, value)| {
                builder.header(*name, *value)
            });
        builder.send().await.map_err(map_error)
    }

    async fn read_body(&self, mut response: Response) -> Result<(Vec<u8>, bool), FetchError> {
        let cap = self.inner.settings.max_body_bytes;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(map_error)? {
            let room = cap - body.len();
            if chunk.len() > room {
                body.extend_from_slice(&chunk[..room]);
                return Ok((body, true));
            }
            body.extend_from_slice(&chunk);
        }
        Ok((body, false))
    }

    async fn robots_rule(
        &self,
        url: &Url,
        source: SourceName,
    ) -> Result<Arc<RobotsRule>, FetchError> {
        let origin = url.origin().ascii_serialization();
        if let Some(rule) = self.inner.robots.get(&origin) {
            return Ok(rule);
        }
        let robots_url =
            Url::parse(&format!("{origin}/robots.txt")).map_err(|_| FetchError::InvalidUrl)?;
        let settings = &self.inner.settings;
        let failure = || (RobotsRule::DisallowAll, RobotsCache::failure_ttl());
        let mut current = robots_url.clone();
        let mut hops: u8 = 0;
        let (rule, ttl) = loop {
            self.throttle(&robots_url, source).await?;
            if !settings.allow_private_targets {
                match ensure_public(&current).await {
                    Ok(()) => {}
                    Err(FetchError::NonPublicTarget) | Err(FetchError::Network(_)) => {
                        break failure();
                    }
                    Err(error) => return Err(error),
                }
            }
            let Ok(response) = self.send(&current, Payload::Empty, Credentials::NONE).await else {
                break failure();
            };
            let status = response.status();
            if is_redirect(status) {
                match redirect_target(&response, &current) {
                    Ok(Some(next)) if hops < ROBOTS_MAX_REDIRECTS => {
                        hops += 1;
                        current = next;
                        continue;
                    }
                    _ => break failure(),
                }
            }
            break if status.is_success() {
                match self.read_body(response).await {
                    Ok((body, _)) => (RobotsRule::from_body(&body), settings.robots_ttl),
                    Err(_) => failure(),
                }
            } else if status.is_client_error() {
                (RobotsRule::AllowAll, settings.robots_ttl)
            } else {
                failure()
            };
        };
        let rule = Arc::new(rule);
        self.inner.robots.insert(&origin, Arc::clone(&rule), ttl);
        Ok(rule)
    }

    async fn run(
        &self,
        request: &FetchRequest,
        payload: Payload<'_>,
        credentials: Credentials<'_>,
    ) -> Result<FetchResponse, FetchError> {
        let settings = &self.inner.settings;
        let mut current = request.url.clone();
        let origin = request.url.origin();
        let mut hops: u8 = 0;
        loop {
            validate(&current)?;
            if !settings.allow_private_targets {
                ensure_public(&current).await?;
            }
            if request.purpose == FetchPurpose::Crawl {
                let rule = self.robots_rule(&current, request.source).await?;
                if !rule.allows(current.as_str()) {
                    return Err(FetchError::DisallowedByRobots);
                }
            }
            self.throttle(&current, request.source).await?;
            let hop_credentials = if current.origin() == origin {
                credentials
            } else {
                Credentials::NONE
            };
            let response = self.send(&current, payload, hop_credentials).await?;
            if let Some(next) = redirect_target(&response, &current)? {
                if !payload.is_empty() || hops >= settings.max_redirects {
                    return Err(FetchError::Redirect);
                }
                hops += 1;
                current = next;
                continue;
            }
            let status = response.status().as_u16();
            let headers = response
                .headers()
                .iter()
                .map(|(name, value)| {
                    (
                        name.as_str().to_ascii_lowercase(),
                        String::from_utf8_lossy(value.as_bytes()).into_owned(),
                    )
                })
                .collect();
            let (body, truncated) = self.read_body(response).await?;
            return Ok(FetchResponse {
                status,
                headers,
                body,
                final_url: current,
                truncated,
            });
        }
    }
}

impl<L: RateLimiter + 'static> Fetcher for PoliteFetcher<L> {
    async fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let span = info_span!(
            "fetch",
            host = request.url.host_str().unwrap_or(""),
            purpose = ?request.purpose,
            source = %request.source,
        );
        self.run(request, Payload::Empty, Credentials::NONE)
            .instrument(span)
            .await
    }
}

impl<L: RateLimiter + 'static> PostFetcher for PoliteFetcher<L> {
    async fn post(&self, request: &PostRequest) -> Result<FetchResponse, FetchError> {
        let span = info_span!(
            "fetch",
            host = request.url.host_str().unwrap_or(""),
            purpose = ?FetchPurpose::Api,
            source = %request.source,
        );
        let fetch = FetchRequest {
            url: request.url.clone(),
            purpose: FetchPurpose::Api,
            source: request.source,
        };
        self.run(&fetch, Payload::Json(&request.body), Credentials::NONE)
            .instrument(span)
            .await
    }
}

impl<L: RateLimiter + 'static> ApiFetcher for PoliteFetcher<L> {
    async fn call(&self, request: &ApiRequest) -> Result<FetchResponse, FetchError> {
        let span = info_span!(
            "fetch",
            host = request.url.host_str().unwrap_or(""),
            purpose = ?FetchPurpose::Api,
            source = %request.source,
        );
        let fetch = FetchRequest {
            url: request.url.clone(),
            purpose: FetchPurpose::Api,
            source: request.source,
        };
        let encoded = request.form.as_ref().map(|fields| {
            let mut serializer = form_urlencoded::Serializer::new(String::new());
            for (name, value) in fields {
                serializer.append_pair(name, value.expose());
            }
            serializer.finish().into_bytes()
        });
        let payload = match &encoded {
            Some(body) => Payload::Form(body),
            None => Payload::Empty,
        };
        let headers: Vec<(&str, &str)> = request
            .headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.expose()))
            .collect();
        let credentials = Credentials {
            bearer: request.bearer.as_ref().map(Secret::expose),
            headers: &headers,
        };
        self.run(&fetch, payload, credentials)
            .instrument(span)
            .await
    }
}

fn validate(url: &Url) -> Result<(), FetchError> {
    if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() {
        Ok(())
    } else {
        Err(FetchError::InvalidUrl)
    }
}

fn is_redirect(status: StatusCode) -> bool {
    matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
}

fn redirect_target(response: &Response, current: &Url) -> Result<Option<Url>, FetchError> {
    if !is_redirect(response.status()) {
        return Ok(None);
    }
    let Some(location) = response.headers().get(LOCATION) else {
        return Ok(None);
    };
    let location = location.to_str().map_err(|_| FetchError::Redirect)?;
    let next = current.join(location).map_err(|_| FetchError::Redirect)?;
    if !matches!(next.scheme(), "http" | "https") || next.host_str().is_none() {
        return Err(FetchError::Redirect);
    }
    if same_site(current, &next) {
        Ok(Some(next))
    } else {
        Err(FetchError::Redirect)
    }
}

fn same_site(from: &Url, to: &Url) -> bool {
    match (from.host(), to.host()) {
        (Some(Host::Domain(_)), Some(Host::Domain(_))) => {
            match (
                NormalizedDomain::from_url(from.as_str()),
                NormalizedDomain::from_url(to.as_str()),
            ) {
                (Ok(left), Ok(right)) => left == right,
                _ => false,
            }
        }
        (Some(Host::Ipv4(left)), Some(Host::Ipv4(right))) => left == right,
        (Some(Host::Ipv6(left)), Some(Host::Ipv6(right))) => left == right,
        _ => false,
    }
}

fn map_error(error: reqwest::Error) -> FetchError {
    if error.is_timeout() {
        FetchError::Timeout
    } else if error.is_connect() {
        FetchError::Network("connection failed".to_owned())
    } else if error.is_body() || error.is_decode() {
        FetchError::Network("body read failed".to_owned())
    } else {
        FetchError::Network("request failed".to_owned())
    }
}
