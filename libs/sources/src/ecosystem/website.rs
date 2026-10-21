use std::{iter::once, sync::Arc, sync::LazyLock, time::Duration};

use chrono::{DateTime, Utc};
use lodestone_core::{
    EcosystemFinding, EcosystemModule, FetchError, FetchPurpose, FetchRequest, FetchResponse,
    Fetcher, InspectionSettings, InspectionTarget, NormalizedDomain, SourceError, SourceName,
};
use regex::Regex;
use serde_json::json;
use tracing::debug;
use url::Url;

use super::tech::{Detection, Evidence, PageEvidence, detect as detect_technologies, explains};
use crate::{
    http_probe::{ProbedPath, detect, probed},
    link_check::{LinkChecker, LinkOrigin, LinkVerdict, fetch_waiting},
};

pub const MAX_INTERNAL_PAGES: usize = 2;
const SKIPPED_EXTENSIONS: [&str; 16] = [
    ".pdf", ".jpg", ".jpeg", ".png", ".gif", ".svg", ".webp", ".ico", ".css", ".js", ".xml",
    ".zip", ".mp4", ".mp3", ".doc", ".docx",
];

fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => unreachable!("static pattern {pattern} is invalid: {error}"),
    }
}

static GENERATOR: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r#"(?is)<meta\s[^>]*?name\s*=\s*["']generator["'][^>]*?content\s*=\s*["']([^"']{1,120})["']"#,
    )
});
static ANCHOR: LazyLock<Regex> =
    LazyLock::new(|| compile(r#"(?is)<a\s[^>]*?href\s*=\s*(?:"([^"]*)"|'([^']*)')"#));

pub(crate) fn rate_limited(error: FetchError) -> SourceError {
    match error {
        FetchError::RateLimited { retry_after } => SourceError::RateLimited { retry_after },
        other => SourceError::Fetch(other),
    }
}

#[derive(Debug, Clone)]
pub struct WebsiteSettings {
    pub max_rate_limit_wait: Duration,
    pub origin: Option<Url>,
}

impl WebsiteSettings {
    pub fn url(&self, host: &str, path: &str) -> Option<Url> {
        match &self.origin {
            Some(origin) => origin.join(path).ok(),
            None => Url::parse(&format!("https://{host}{path}")).ok(),
        }
    }
}

pub(crate) fn same_site(url: &Url, base: &Url, domain: &NormalizedDomain) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    base.host_str() == Some(host)
        || NormalizedDomain::from_host(host).is_ok_and(|registrable| registrable == *domain)
}

pub(crate) fn page_links(html: &str, base: &Url, domain: &NormalizedDomain) -> Vec<Url> {
    let mut links: Vec<Url> = Vec::new();
    for captures in ANCHOR.captures_iter(html) {
        let Some(raw) = captures.get(1).or_else(|| captures.get(2)) else {
            continue;
        };
        let Ok(mut url) = base.join(raw.as_str().trim()) else {
            continue;
        };
        url.set_fragment(None);
        if !matches!(url.scheme(), "http" | "https") || !same_site(&url, base, domain) {
            continue;
        }
        if !links.contains(&url) {
            links.push(url);
        }
    }
    links
}

pub fn internal_pages(html: &str, base: &Url, domain: &NormalizedDomain) -> Vec<Url> {
    let mut chosen: Vec<Url> = Vec::new();
    let mut sections: Vec<String> = Vec::new();
    for mut url in page_links(html, base, domain) {
        url.set_query(None);
        let path = url.path().to_ascii_lowercase();
        if path == "/" || path.is_empty() {
            continue;
        }
        if SKIPPED_EXTENSIONS
            .iter()
            .any(|extension| path.ends_with(extension))
        {
            continue;
        }
        let section = path
            .trim_start_matches('/')
            .split('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        if sections.contains(&section) || chosen.contains(&url) {
            continue;
        }
        sections.push(section);
        chosen.push(url);
        if chosen.len() == MAX_INTERNAL_PAGES {
            break;
        }
    }
    chosen
}

pub fn technology_findings(detections: &[Detection], now: DateTime<Utc>) -> Vec<EcosystemFinding> {
    detections
        .iter()
        .map(|detection| {
            let signature = detection.signature;
            EcosystemFinding::new(
                EcosystemModule::WebsiteStack,
                "technology",
                signature.category.label(),
                detection.display(),
                detection.source.as_str(),
                now,
            )
            .keyed(format!("technology:{}", signature.id))
            .with_details(json!({
                "category": signature.category.as_str(),
                "tech": signature.id,
                "name": signature.name,
                "version": detection.version,
                "evidence": detection.evidence.describe(),
            }))
        })
        .collect()
}

pub struct WebsiteModule<F: Fetcher> {
    fetcher: Arc<F>,
    settings: WebsiteSettings,
}

impl<F: Fetcher> WebsiteModule<F> {
    pub fn new(fetcher: Arc<F>, settings: WebsiteSettings) -> Self {
        Self { fetcher, settings }
    }

    async fn crawl(&self, url: &Url) -> Result<Option<FetchResponse>, SourceError> {
        let request = FetchRequest {
            url: url.clone(),
            purpose: FetchPurpose::Crawl,
            source: SourceName::HttpProbe,
        };
        match fetch_waiting(
            self.fetcher.as_ref(),
            &request,
            self.settings.max_rate_limit_wait,
        )
        .await
        {
            Ok(response) => Ok(Some(response)),
            Err(FetchError::RateLimited { retry_after }) => {
                Err(SourceError::RateLimited { retry_after })
            }
            Err(error) => {
                debug!(%url, %error, "page of the site not read");
                Ok(None)
            }
        }
    }

    async fn confirmed_pages(
        &self,
        domain: &NormalizedDomain,
    ) -> Result<Vec<FetchResponse>, SourceError> {
        let Some(home) = self.settings.url(domain.as_str(), "/") else {
            return Ok(Vec::new());
        };
        let Some(response) = self.crawl(&home).await? else {
            return Ok(Vec::new());
        };
        let mut checker = LinkChecker::new(
            self.fetcher.as_ref(),
            SourceName::HttpProbe,
            self.settings.max_rate_limit_wait,
        );
        let verdict = checker
            .confirm(&home, LinkOrigin::Home, &response)
            .await
            .map_err(rate_limited)?;
        if !matches!(verdict, LinkVerdict::Confirmed(_)) {
            return Ok(Vec::new());
        }
        let links = internal_pages(&response.text(), &response.final_url, domain);
        let mut pages = vec![response];
        for link in links {
            let Some(page) = self.crawl(&link).await? else {
                continue;
            };
            let verdict = checker
                .confirm(&link, LinkOrigin::Linked, &page)
                .await
                .map_err(rate_limited)?;
            if matches!(verdict, LinkVerdict::Confirmed(_)) {
                pages.push(page);
            }
        }
        Ok(pages)
    }

    async fn site_findings(
        &self,
        domain: &NormalizedDomain,
        now: DateTime<Utc>,
    ) -> Result<Vec<EcosystemFinding>, SourceError> {
        let module = EcosystemModule::WebsiteStack;
        let pages = self.confirmed_pages(domain).await?;
        let Some(home) = pages.first() else {
            return Ok(Vec::new());
        };
        let texts: Vec<_> = pages.iter().map(FetchResponse::text).collect();
        let evidence: Vec<PageEvidence<'_>> = pages
            .iter()
            .zip(&texts)
            .map(|(page, text)| PageEvidence {
                url: page.final_url.as_str(),
                headers: &page.headers,
                html: text,
            })
            .collect();
        let detections = detect_technologies(&evidence);
        let source = home.final_url.as_str();
        let mut findings = vec![
            EcosystemFinding::new(module, "website", "Site web", source, source, now)
                .keyed("website"),
        ];
        findings.extend(technology_findings(&detections, now));
        if let Some(server) = home.header("server").map(str::trim)
            && !server.is_empty()
            && !explains(Evidence::Header("server"), server)
        {
            findings.push(
                EcosystemFinding::new(module, "server", "Serveur web", server, source, now)
                    .keyed("server"),
            );
        }
        if let Some(powered) = home.header("x-powered-by").map(str::trim)
            && !powered.is_empty()
            && !explains(Evidence::Header("x-powered-by"), powered)
        {
            findings.push(
                EcosystemFinding::new(
                    module,
                    "powered_by",
                    "Technologie annoncée",
                    powered,
                    source,
                    now,
                )
                .keyed("powered_by"),
            );
        }
        if let Some(generator) = texts
            .first()
            .and_then(|html| GENERATOR.captures(html))
            .and_then(|captures| captures.get(1))
            && !explains(Evidence::Meta("generator"), generator.as_str())
        {
            findings.push(
                EcosystemFinding::new(
                    module,
                    "generator",
                    "Générateur du site",
                    generator.as_str().trim(),
                    source,
                    now,
                )
                .keyed("generator"),
            );
        }
        Ok(findings)
    }

    async fn probe_host(&self, host: &str, paths: &[&str]) -> Result<Vec<ProbedPath>, SourceError> {
        let mut responses = Vec::with_capacity(paths.len());
        for path in paths {
            let Some(url) = self.settings.url(host, path) else {
                continue;
            };
            let request = FetchRequest {
                url,
                purpose: FetchPurpose::Probe,
                source: SourceName::HttpProbe,
            };
            let response = match self.fetcher.fetch(&request).await {
                Ok(response) => Some(probed(&response)),
                Err(FetchError::RateLimited { retry_after }) => {
                    return Err(SourceError::RateLimited { retry_after });
                }
                Err(error) => {
                    debug!(host, path, %error, "fingerprint probe failed");
                    None
                }
            };
            responses.push(ProbedPath {
                path: (*path).to_owned(),
                response,
            });
        }
        Ok(responses)
    }

    async fn fingerprint_findings(
        &self,
        domain: &NormalizedDomain,
        settings: &InspectionSettings,
        now: DateTime<Utc>,
    ) -> Result<Vec<EcosystemFinding>, SourceError> {
        let module = EcosystemModule::WebsiteStack;
        if settings.fingerprints.is_empty() {
            return Ok(Vec::new());
        }
        let mut paths: Vec<&str> = settings
            .fingerprints
            .iter()
            .map(|fingerprint| fingerprint.path.as_str())
            .collect();
        paths.sort_unstable();
        paths.dedup();
        let hosts: Vec<String> = once(domain.as_str().to_owned())
            .chain(
                settings
                    .extra_subdomains
                    .iter()
                    .map(|label| format!("{label}.{domain}")),
            )
            .collect();
        let mut findings = Vec::new();
        for host in &hosts {
            let responses = self.probe_host(host, &paths).await?;
            for found in detect(
                &settings.fingerprints,
                &settings.excluded_products,
                &responses,
            ) {
                let Some(url) = self.settings.url(host, &found.path) else {
                    continue;
                };
                let value = match &found.version {
                    Some(version) => format!("{} {version}", found.product),
                    None => found.product.clone(),
                };
                let details = json!({
                    "host": host,
                    "fingerprint": found.fingerprint,
                    "path": found.path,
                    "status": found.status,
                    "version": found.version,
                });
                findings.push(
                    EcosystemFinding::new(
                        module,
                        "product",
                        "Produit détecté",
                        value,
                        url.as_str(),
                        now,
                    )
                    .keyed(format!("product:{host}:{}", found.fingerprint))
                    .with_details(details.clone()),
                );
                for marker in &found.markers {
                    findings.push(
                        EcosystemFinding::new(
                            module,
                            "debt_marker",
                            "Signe d'obsolescence",
                            marker.as_str(),
                            url.as_str(),
                            now,
                        )
                        .keyed(format!("debt_marker:{host}:{marker}"))
                        .with_details(details.clone()),
                    );
                }
            }
        }
        Ok(findings)
    }

    pub async fn inspect(
        &self,
        target: &InspectionTarget<'_>,
    ) -> Result<Vec<EcosystemFinding>, SourceError> {
        let mut findings = self.site_findings(target.domain, target.now).await?;
        let profile = self
            .fingerprint_findings(target.domain, target.settings, target.now)
            .await?;
        let at = findings.len().min(1);
        findings.splice(at..at, profile);
        Ok(findings)
    }
}

#[cfg(test)]
mod tests;
