use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::AtomicUsize, atomic::Ordering},
    time::Duration,
};

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_core::{
    ActiveProfile, CandidateHost, CandidateHosts, FetchError, FetchPurpose, FetchRequest,
    FetchResponse, Fetcher, Hostname, LodestoneUseCase, Observation, ProfileId, Provenance,
    RevisionNumber, SignalKind, Source, SourceError, SourceName, UnitInterval,
    definition::{DebtRule, Fingerprint, Matcher, SourceTarget, VersionExtractor},
    transient_checked_at,
};
use regex::Regex;
use serde_json::json;
use tokio::{sync::Semaphore, task::JoinSet};
use url::Url;

const PRODUCT_KIND: &str = "product_detected";
const DEBT_KIND: &str = "debt_marker";
const PRODUCT_STRENGTH: f64 = 0.9;
const DEBT_STRENGTH: f64 = 0.6;
const MAX_BODY_BYTES: usize = 256 * 1024;
const MAX_CACHED_PROFILES: usize = 64;

pub trait ProbeGuard: Send + Sync {
    fn reserve(
        &self,
        host: &Hostname,
        now: DateTime<Utc>,
        revisit: Duration,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;
}

impl ProbeGuard for LodestoneUseCase {
    async fn reserve(
        &self,
        host: &Hostname,
        now: DateTime<Utc>,
        revisit: Duration,
    ) -> Result<bool, CoreError> {
        self.reserve_probe(host.as_str(), now, revisit).await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeScheme {
    Https,
    Http,
}

impl ProbeScheme {
    fn as_str(self) -> &'static str {
        match self {
            Self::Https => "https",
            Self::Http => "http",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpProbeSettings {
    pub batch_size: u32,
    pub concurrency: usize,
    pub revisit_interval: Duration,
    pub scheme: ProbeScheme,
    pub port: Option<u16>,
    pub host_override: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedPath {
    pub path: String,
    pub response: Option<ProbedResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedHost {
    pub host: CandidateHost,
    pub responses: Vec<ProbedPath>,
    pub probed_at: DateTime<Utc>,
}

enum CompiledMatcher {
    Status(u16),
    Header(String, Regex),
    Body(Regex),
}

enum CompiledExtractor {
    Header(String, Regex),
    Body(Regex),
}

enum CompiledRule {
    Matches(CompiledMatcher),
    VersionBelow(Vec<u64>),
}

struct CompiledFingerprint {
    id: String,
    path: String,
    product: String,
    matchers: Vec<CompiledMatcher>,
    version: Option<CompiledExtractor>,
    markers: Vec<(String, CompiledRule)>,
}

struct CompiledProfile {
    fingerprints: Vec<CompiledFingerprint>,
    excluded: Vec<String>,
}

fn compile_matcher(matcher: &Matcher) -> Option<CompiledMatcher> {
    Some(match matcher {
        Matcher::Status { equals } => CompiledMatcher::Status(*equals),
        Matcher::Header { name, pattern } => {
            CompiledMatcher::Header(name.clone(), Regex::new(pattern).ok()?)
        }
        Matcher::Body { pattern } => CompiledMatcher::Body(Regex::new(pattern).ok()?),
    })
}

fn compile_extractor(extractor: &VersionExtractor) -> Option<CompiledExtractor> {
    Some(match extractor {
        VersionExtractor::Header { name, pattern } => {
            CompiledExtractor::Header(name.clone(), Regex::new(pattern).ok()?)
        }
        VersionExtractor::Body { pattern } => CompiledExtractor::Body(Regex::new(pattern).ok()?),
    })
}

fn compile_fingerprint(fingerprint: &Fingerprint) -> Option<CompiledFingerprint> {
    let matchers = fingerprint
        .matchers
        .iter()
        .map(compile_matcher)
        .collect::<Option<Vec<_>>>()?;
    let version = match &fingerprint.version {
        Some(extractor) => Some(compile_extractor(extractor)?),
        None => None,
    };
    let mut markers = Vec::new();
    for marker in &fingerprint.debt_markers {
        let rule = match &marker.rule {
            DebtRule::Matches { matcher } => CompiledRule::Matches(compile_matcher(matcher)?),
            DebtRule::VersionBelow { version } => {
                CompiledRule::VersionBelow(parse_version(version)?)
            }
        };
        markers.push((marker.id.clone(), rule));
    }
    Some(CompiledFingerprint {
        id: fingerprint.id.clone(),
        path: fingerprint.path.clone(),
        product: fingerprint.product.clone(),
        matchers,
        version,
        markers,
    })
}

fn parse_version(raw: &str) -> Option<Vec<u64>> {
    let parts = raw
        .trim()
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    (!parts.is_empty()).then_some(parts)
}

fn version_below(found: &[u64], bound: &[u64]) -> bool {
    let length = found.len().max(bound.len());
    for index in 0..length {
        let left = found.get(index).copied().unwrap_or(0);
        let right = bound.get(index).copied().unwrap_or(0);
        if left != right {
            return left < right;
        }
    }
    false
}

fn header_value<'a>(response: &'a ProbedResponse, name: &str) -> Option<&'a str> {
    response
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn matcher_holds(matcher: &CompiledMatcher, response: &ProbedResponse) -> bool {
    match matcher {
        CompiledMatcher::Status(expected) => response.status == *expected,
        CompiledMatcher::Header(name, pattern) => {
            header_value(response, name).is_some_and(|value| pattern.is_match(value))
        }
        CompiledMatcher::Body(pattern) => pattern.is_match(&response.body),
    }
}

fn extract_version(extractor: &CompiledExtractor, response: &ProbedResponse) -> Option<String> {
    let captures = match extractor {
        CompiledExtractor::Header(name, pattern) => {
            pattern.captures(header_value(response, name)?)?
        }
        CompiledExtractor::Body(pattern) => pattern.captures(&response.body)?,
    };
    captures.get(1).map(|found| found.as_str().to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FingerprintMatch {
    pub fingerprint: String,
    pub product: String,
    pub path: String,
    pub status: u16,
    pub version: Option<String>,
    pub markers: Vec<String>,
}

fn matches(compiled: &CompiledProfile, responses: &[ProbedPath]) -> Vec<FingerprintMatch> {
    let mut matched: Vec<(&CompiledFingerprint, &ProbedResponse)> = Vec::new();
    for fingerprint in &compiled.fingerprints {
        let Some(response) = responses
            .iter()
            .find(|probed| probed.path == fingerprint.path)
            .and_then(|probed| probed.response.as_ref())
        else {
            continue;
        };
        if fingerprint
            .matchers
            .iter()
            .all(|matcher| matcher_holds(matcher, response))
        {
            matched.push((fingerprint, response));
        }
    }
    if matched
        .iter()
        .any(|(fingerprint, _)| compiled.excluded.contains(&fingerprint.product))
    {
        return Vec::new();
    }
    matched
        .into_iter()
        .map(|(fingerprint, response)| {
            let version = fingerprint
                .version
                .as_ref()
                .and_then(|extractor| extract_version(extractor, response));
            let parsed = version.as_deref().and_then(parse_version);
            let markers = fingerprint
                .markers
                .iter()
                .filter(|(_, rule)| match rule {
                    CompiledRule::Matches(matcher) => matcher_holds(matcher, response),
                    CompiledRule::VersionBelow(bound) => parsed
                        .as_deref()
                        .is_some_and(|found| version_below(found, bound)),
                })
                .map(|(marker, _)| marker.clone())
                .collect();
            FingerprintMatch {
                fingerprint: fingerprint.id.clone(),
                product: fingerprint.product.clone(),
                path: fingerprint.path.clone(),
                status: response.status,
                version,
                markers,
            }
        })
        .collect()
}

pub fn detect(
    fingerprints: &[Fingerprint],
    excluded: &[String],
    responses: &[ProbedPath],
) -> Vec<FingerprintMatch> {
    let compiled = CompiledProfile {
        fingerprints: fingerprints
            .iter()
            .filter_map(compile_fingerprint)
            .collect(),
        excluded: excluded.to_vec(),
    };
    matches(&compiled, responses)
}

pub struct HttpProbeSource<F: Fetcher, H: CandidateHosts, G: ProbeGuard> {
    fetcher: Arc<F>,
    hosts: Arc<H>,
    guard: Arc<G>,
    settings: HttpProbeSettings,
    compiled: Mutex<HashMap<(ProfileId, RevisionNumber), Arc<CompiledProfile>>>,
    compilations: AtomicUsize,
}

impl<F: Fetcher, H: CandidateHosts, G: ProbeGuard> HttpProbeSource<F, H, G> {
    pub fn new(fetcher: Arc<F>, hosts: Arc<H>, guard: Arc<G>, settings: HttpProbeSettings) -> Self {
        Self {
            fetcher,
            hosts,
            guard,
            settings,
            compiled: Mutex::new(HashMap::new()),
            compilations: AtomicUsize::new(0),
        }
    }

    pub fn compilations(&self) -> usize {
        self.compilations.load(Ordering::SeqCst)
    }

    fn compiled_for(
        &self,
        profile: &ActiveProfile,
        target: &SourceTarget<'_>,
    ) -> Option<Arc<CompiledProfile>> {
        let config = target.http_probe()?;
        let key = (profile.profile.id, profile.revision.number);
        let mut cache = self.compiled.lock().ok()?;
        if let Some(found) = cache.get(&key) {
            return Some(Arc::clone(found));
        }
        self.compilations.fetch_add(1, Ordering::SeqCst);
        let compiled = Arc::new(CompiledProfile {
            fingerprints: config
                .fingerprints
                .iter()
                .filter_map(compile_fingerprint)
                .collect(),
            excluded: config.excluded_products.clone(),
        });
        if cache.len() >= MAX_CACHED_PROFILES {
            cache.clear();
        }
        cache.insert(key, Arc::clone(&compiled));
        Some(compiled)
    }
}

fn upstream(error: impl std::fmt::Display) -> SourceError {
    SourceError::Upstream(error.to_string())
}

fn probe_paths(profiles: &[ActiveProfile]) -> Vec<String> {
    let mut paths: Vec<String> = profiles
        .iter()
        .flat_map(|profile| {
            profile
                .revision
                .definition
                .definition()
                .targets(SourceName::HttpProbe)
        })
        .filter_map(|target| target.http_probe())
        .flat_map(|config| config.fingerprints.iter().map(|item| item.path.clone()))
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

pub fn probed(response: &FetchResponse) -> ProbedResponse {
    let end = response.body.len().min(MAX_BODY_BYTES);
    ProbedResponse {
        status: response.status,
        headers: response.headers.clone(),
        body: String::from_utf8_lossy(&response.body[..end]).into_owned(),
    }
}

enum HostOutcome {
    Probed(ProbedHost),
    Skipped(Hostname),
    Dropped(Hostname),
}

struct ProbeContext<F, G> {
    fetcher: Arc<F>,
    guard: Arc<G>,
    settings: HttpProbeSettings,
    paths: Arc<Vec<String>>,
    permits: Arc<Semaphore>,
    now: DateTime<Utc>,
    revisit: Duration,
}

impl<F: Fetcher, G: ProbeGuard> ProbeContext<F, G> {
    fn url(&self, host: &Hostname, path: &str) -> Option<Url> {
        let authority = match (&self.settings.host_override, self.settings.port) {
            (Some(over), _) => over.clone(),
            (None, Some(port)) => format!("{host}:{port}"),
            (None, None) => host.as_str().to_owned(),
        };
        Url::parse(&format!(
            "{}://{authority}{path}",
            self.settings.scheme.as_str()
        ))
        .ok()
    }

    async fn probe(&self, candidate: CandidateHost) -> Result<HostOutcome, SourceError> {
        let Ok(_permit) = self.permits.acquire().await else {
            return Ok(HostOutcome::Skipped(candidate.host));
        };
        let reserved = self
            .guard
            .reserve(&candidate.host, self.now, self.revisit)
            .await
            .map_err(upstream)?;
        if !reserved {
            return Ok(HostOutcome::Skipped(candidate.host));
        }
        let mut responses = Vec::with_capacity(self.paths.len());
        for path in self.paths.iter() {
            let Some(url) = self.url(&candidate.host, path) else {
                responses.push(ProbedPath {
                    path: path.clone(),
                    response: None,
                });
                continue;
            };
            let request = FetchRequest {
                url,
                purpose: FetchPurpose::Probe,
                source: SourceName::HttpProbe,
            };
            let response = match self.fetcher.fetch(&request).await {
                Ok(response) => Some(probed(&response)),
                Err(FetchError::RateLimited { .. }) => {
                    return Ok(HostOutcome::Dropped(candidate.host));
                }
                Err(error) => {
                    tracing::debug!(host = %candidate.host, path, %error, "probe fetch failed");
                    None
                }
            };
            responses.push(ProbedPath {
                path: path.clone(),
                response,
            });
        }
        Ok(HostOutcome::Probed(ProbedHost {
            host: candidate,
            responses,
            probed_at: self.now,
        }))
    }
}

impl<F, H, G> Source for HttpProbeSource<F, H, G>
where
    F: Fetcher + 'static,
    H: CandidateHosts + 'static,
    G: ProbeGuard + 'static,
{
    type Item = ProbedHost;

    fn name(&self) -> SourceName {
        SourceName::HttpProbe
    }

    async fn collect(
        &self,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<Vec<Self::Item>, SourceError> {
        let paths = probe_paths(profiles);
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        for active in profiles {
            let definition = active.revision.definition.definition();
            if definition.probes_hosts() {
                self.hosts
                    .seed_lead_hosts(
                        active.profile.id,
                        &definition.enrichment.custom.extra_subdomains,
                        now,
                        self.settings.batch_size,
                    )
                    .await
                    .map_err(upstream)?;
            }
        }
        let interval = chrono::Duration::from_std(self.settings.revisit_interval)
            .map_err(|_| upstream("revisit interval is out of range"))?;
        let checked_before = now
            .checked_sub_signed(interval)
            .ok_or_else(|| upstream("revisit interval is out of range"))?;
        let due = self
            .hosts
            .list_due(
                SourceName::HttpProbe,
                checked_before,
                self.settings.batch_size,
            )
            .await
            .map_err(upstream)?;
        let context = Arc::new(ProbeContext {
            fetcher: Arc::clone(&self.fetcher),
            guard: Arc::clone(&self.guard),
            settings: self.settings.clone(),
            paths: Arc::new(paths),
            permits: Arc::new(Semaphore::new(self.settings.concurrency.max(1))),
            now,
            revisit: self.settings.revisit_interval,
        });
        let mut tasks = JoinSet::new();
        for (index, candidate) in due.into_iter().enumerate() {
            let context = Arc::clone(&context);
            tasks.spawn(async move { (index, context.probe(candidate).await) });
        }
        let mut done: Vec<(usize, ProbedHost)> = Vec::new();
        let mut skipped: Vec<Hostname> = Vec::new();
        let mut dropped: Vec<Hostname> = Vec::new();
        while let Some(joined) = tasks.join_next().await {
            match joined {
                Ok((index, Ok(HostOutcome::Probed(item)))) => done.push((index, item)),
                Ok((_, Ok(HostOutcome::Skipped(host)))) => skipped.push(host),
                Ok((_, Ok(HostOutcome::Dropped(host)))) => dropped.push(host),
                Ok((_, Err(error))) => return Err(error),
                Err(error) => tracing::debug!(%error, "probe task aborted"),
            }
        }
        done.sort_by_key(|(index, _)| *index);
        let items: Vec<ProbedHost> = done.into_iter().map(|(_, item)| item).collect();
        let mut names: Vec<Hostname> = items.iter().map(|item| item.host.host.clone()).collect();
        names.extend(skipped);
        if !names.is_empty() {
            self.hosts
                .mark_checked(&names, SourceName::HttpProbe, now)
                .await
                .map_err(upstream)?;
        }
        if !dropped.is_empty() {
            self.hosts
                .mark_checked(
                    &dropped,
                    SourceName::HttpProbe,
                    transient_checked_at(now, self.settings.revisit_interval),
                )
                .await
                .map_err(upstream)?;
        }
        Ok(items)
    }

    fn evaluate(
        &self,
        item: &Self::Item,
        profile: &ActiveProfile,
        target: &SourceTarget<'_>,
    ) -> Vec<Observation> {
        let Some(compiled) = self.compiled_for(profile, target) else {
            return Vec::new();
        };
        let (Ok(product_kind), Ok(debt_kind)) = (
            SignalKind::parse(PRODUCT_KIND),
            SignalKind::parse(DEBT_KIND),
        ) else {
            return Vec::new();
        };
        let (Ok(product_strength), Ok(debt_strength)) = (
            UnitInterval::new(PRODUCT_STRENGTH),
            UnitInterval::new(DEBT_STRENGTH),
        ) else {
            return Vec::new();
        };
        let mut products: Vec<&str> = Vec::new();
        let mut markers: Vec<&str> = Vec::new();
        let mut observations = Vec::new();
        let found = matches(&compiled, &item.responses);
        for detected in &found {
            let Ok(provenance) = Provenance::parse(&format!(
                "{}://{}{}",
                self.settings.scheme.as_str(),
                item.host.host,
                detected.path
            )) else {
                continue;
            };
            let raw = json!({
                "host": item.host.host.as_str(),
                "fingerprint": detected.fingerprint,
                "path": detected.path,
                "status": detected.status,
                "version": detected.version,
            });
            if !products.contains(&detected.product.as_str()) {
                products.push(detected.product.as_str());
                observations.push(Observation {
                    domain: item.host.domain.clone(),
                    kind: product_kind.clone(),
                    value: Some(detected.product.clone()),
                    raw: raw.clone(),
                    provenance: provenance.clone(),
                    strength: product_strength,
                    observed_at: item.probed_at,
                    agent: None,
                });
            }
            for marker in &detected.markers {
                if markers.contains(&marker.as_str()) {
                    continue;
                }
                markers.push(marker.as_str());
                observations.push(Observation {
                    domain: item.host.domain.clone(),
                    kind: debt_kind.clone(),
                    value: Some(marker.clone()),
                    raw: raw.clone(),
                    provenance: provenance.clone(),
                    strength: debt_strength,
                    observed_at: item.probed_at,
                    agent: None,
                });
            }
        }
        observations
    }
}

#[cfg(test)]
mod tests;
