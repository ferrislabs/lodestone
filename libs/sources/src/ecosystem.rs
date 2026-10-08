use std::{iter::once, sync::Arc, sync::LazyLock, time::Duration};

use chrono::{DateTime, Utc};
use lodestone_core::{
    CompanyBuilder, EcosystemFinding, EcosystemModule, FetchError, FetchPurpose, FetchRequest,
    FetchResponse, Fetcher, InspectionSettings, InspectionTarget, ModuleRunner, NormalizedDomain,
    OfficialData, SourceError, SourceName,
};
use regex::Regex;
use serde_json::json;
use tracing::debug;
use url::Url;

use crate::{
    ats::AtsScanner,
    company::{CompanyResearch, CompanySettings},
    http_probe::{ProbedPath, detect, probed},
    link_check::{LinkChecker, LinkOrigin, LinkVerdict, fetch_waiting},
};

pub const MAX_JOB_OFFERS: usize = 20;
const MAX_CAREER_PAGES: usize = 3;

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

fn rate_limited(error: FetchError) -> SourceError {
    match error {
        FetchError::RateLimited { retry_after } => SourceError::RateLimited { retry_after },
        other => SourceError::Fetch(other),
    }
}

pub fn official_findings(
    official: &OfficialData,
    observed_at: DateTime<Utc>,
) -> Vec<EcosystemFinding> {
    let module = EcosystemModule::OfficialIdentity;
    let source = official.source_url();
    let public_page = official.public_page();
    let mut findings = vec![
        EcosystemFinding::new(
            module,
            "legal_name",
            "Raison sociale",
            official.legal_name.as_str(),
            source.as_str(),
            observed_at,
        )
        .keyed("legal_name"),
        EcosystemFinding::new(
            module,
            "siren",
            "SIREN",
            official.siren.as_str(),
            source.as_str(),
            observed_at,
        )
        .keyed("siren")
        .with_details(json!({"public_page": public_page})),
    ];
    let activity = match (&official.naf_code, &official.naf_label) {
        (Some(code), Some(label)) => Some(format!("{code}, {label}")),
        (Some(code), None) => Some(code.clone()),
        _ => None,
    };
    if let Some(activity) = activity {
        findings.push(
            EcosystemFinding::new(
                module,
                "activity",
                "Activité principale",
                activity,
                source.as_str(),
                observed_at,
            )
            .keyed("activity"),
        );
    }
    if let Some(range) = &official.headcount_range {
        findings.push(
            EcosystemFinding::new(
                module,
                "headcount",
                "Tranche d'effectif",
                range.as_str(),
                source.as_str(),
                observed_at,
            )
            .keyed("headcount"),
        );
    }
    let place: Vec<&str> = [
        official.head_office_postcode.as_deref(),
        official.head_office_city.as_deref(),
        official.head_office_country.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !place.is_empty() {
        findings.push(
            EcosystemFinding::new(
                module,
                "head_office",
                "Siège",
                place.join(" "),
                source.as_str(),
                observed_at,
            )
            .keyed("head_office"),
        );
    }
    if let Some(created) = official.created_on {
        findings.push(
            EcosystemFinding::new(
                module,
                "created_on",
                "Date de création",
                created.to_string(),
                source.as_str(),
                observed_at,
            )
            .keyed("created_on"),
        );
    }
    findings
}

pub struct IdentityModule<F: Fetcher> {
    research: CompanyResearch<F>,
}

impl<F: Fetcher> IdentityModule<F> {
    pub fn new(research: CompanyResearch<F>) -> Self {
        Self { research }
    }

    pub async fn inspect(
        &self,
        target: &InspectionTarget<'_>,
    ) -> Result<Vec<EcosystemFinding>, SourceError> {
        if let Some(dossier) = target.dossier
            && let Some(built_at) = dossier.built_at
        {
            let observed_at = dossier.content_at.unwrap_or(built_at);
            return Ok(dossier
                .official
                .as_ref()
                .map(|official| official_findings(official, observed_at))
                .unwrap_or_default());
        }
        let built = self.research.build(target.domain).await?;
        match (&built.official, built.failure) {
            (Some(official), _) => Ok(official_findings(official, target.now)),
            (None, Some(failure)) => Err(SourceError::Upstream(failure)),
            (None, None) => Ok(Vec::new()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WebsiteSettings {
    pub max_rate_limit_wait: Duration,
    pub origin: Option<Url>,
}

pub struct WebsiteModule<F: Fetcher> {
    fetcher: Arc<F>,
    settings: WebsiteSettings,
}

impl<F: Fetcher> WebsiteModule<F> {
    pub fn new(fetcher: Arc<F>, settings: WebsiteSettings) -> Self {
        Self { fetcher, settings }
    }

    fn url(&self, host: &str, path: &str) -> Option<Url> {
        match &self.settings.origin {
            Some(origin) => origin.join(path).ok(),
            None => Url::parse(&format!("https://{host}{path}")).ok(),
        }
    }

    async fn home_findings(
        &self,
        domain: &NormalizedDomain,
        now: DateTime<Utc>,
    ) -> Result<Vec<EcosystemFinding>, SourceError> {
        let module = EcosystemModule::WebsiteStack;
        let Some(url) = self.url(domain.as_str(), "/") else {
            return Ok(Vec::new());
        };
        let request = FetchRequest {
            url: url.clone(),
            purpose: FetchPurpose::Crawl,
            source: SourceName::HttpProbe,
        };
        let response: FetchResponse = match fetch_waiting(
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
                debug!(%domain, %error, "home page of the site not read");
                return Ok(Vec::new());
            }
        };
        let mut checker = LinkChecker::new(
            self.fetcher.as_ref(),
            SourceName::HttpProbe,
            self.settings.max_rate_limit_wait,
        );
        let verdict = checker
            .confirm(&url, LinkOrigin::Home, &response)
            .await
            .map_err(rate_limited)?;
        if !matches!(verdict, LinkVerdict::Confirmed(_)) {
            return Ok(Vec::new());
        }
        let source = response.final_url.as_str();
        let mut findings = vec![
            EcosystemFinding::new(module, "website", "Site web", source, source, now)
                .keyed("website"),
        ];
        if let Some(server) = response.header("server") {
            findings.push(
                EcosystemFinding::new(module, "server", "Serveur web", server, source, now)
                    .keyed("server"),
            );
        }
        if let Some(powered) = response.header("x-powered-by") {
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
        let html = response.text();
        if let Some(generator) = GENERATOR
            .captures(&html)
            .and_then(|captures| captures.get(1))
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
            let Some(url) = self.url(host, path) else {
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
                let Some(url) = self.url(host, &found.path) else {
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
        let mut findings = self.home_findings(target.domain, target.now).await?;
        findings.extend(
            self.fingerprint_findings(target.domain, target.settings, target.now)
                .await?,
        );
        Ok(findings)
    }
}

pub struct HiringModule<F: Fetcher> {
    scanner: AtsScanner<F>,
}

fn matched_keywords<'a>(text: &str, keywords: &'a [String]) -> Vec<&'a str> {
    let lowered = text.to_lowercase();
    keywords
        .iter()
        .filter(|keyword| {
            let wanted = keyword.trim().to_lowercase();
            !wanted.is_empty() && lowered.contains(&wanted)
        })
        .map(String::as_str)
        .collect()
}

impl<F: Fetcher> HiringModule<F> {
    pub fn new(scanner: AtsScanner<F>) -> Self {
        Self { scanner }
    }

    pub async fn inspect(
        &self,
        target: &InspectionTarget<'_>,
    ) -> Result<Vec<EcosystemFinding>, SourceError> {
        let module = EcosystemModule::HiringActivity;
        let now = target.now;
        let scan = self.scanner.scan(target.domain, now).await?;
        let mut findings: Vec<EcosystemFinding> = scan
            .pages
            .iter()
            .take(MAX_CAREER_PAGES)
            .map(|page| {
                EcosystemFinding::new(
                    module,
                    "careers_page",
                    "Page carrières",
                    page.as_str(),
                    page.as_str(),
                    now,
                )
            })
            .collect();
        let mut offers: Vec<(bool, EcosystemFinding)> = Vec::new();
        for board in &scan.boards {
            let Some(api) = board.board.api_url() else {
                continue;
            };
            let name = format!("{} ({})", board.board.vendor.label(), board.board.slug);
            findings.push(
                EcosystemFinding::new(
                    module,
                    "ats_board",
                    "Outil de recrutement",
                    name.as_str(),
                    api.as_str(),
                    now,
                )
                .keyed(format!("ats_board:{name}")),
            );
            findings.push(
                EcosystemFinding::new(
                    module,
                    "open_positions",
                    "Offres publiées",
                    board.candidates.len().to_string(),
                    api.as_str(),
                    now,
                )
                .keyed(format!("open_positions:{name}")),
            );
            for offer in &board.candidates {
                let keywords = matched_keywords(
                    &format!("{} {}", offer.title, offer.text),
                    &target.settings.highlight_keywords,
                );
                let highlighted = !keywords.is_empty();
                offers.push((
                    highlighted,
                    EcosystemFinding::new(
                        module,
                        "job_offer",
                        "Offre d'emploi",
                        offer.title.as_str(),
                        offer.provenance.as_str(),
                        offer.observed_at,
                    )
                    .keyed(format!("job_offer:{}", offer.provenance.as_str()))
                    .with_details(json!({
                        "highlighted": highlighted,
                        "keywords": keywords,
                        "location": offer.details.get("location"),
                        "team": offer.details.get("team"),
                        "ats": offer.details.get("ats"),
                    })),
                ));
            }
        }
        offers.sort_by_key(|(highlighted, _)| !*highlighted);
        findings.extend(
            offers
                .into_iter()
                .take(MAX_JOB_OFFERS)
                .map(|(_, finding)| finding),
        );
        Ok(findings)
    }
}

pub struct EcosystemModules<F: Fetcher> {
    pub identity: IdentityModule<F>,
    pub website: WebsiteModule<F>,
    pub hiring: HiringModule<F>,
}

#[derive(Debug, Clone)]
pub struct EcosystemConfig {
    pub company: CompanySettings,
    pub website: WebsiteSettings,
    pub max_career_paths: usize,
    pub max_rate_limit_wait: Duration,
}

impl<F: Fetcher> EcosystemModules<F> {
    pub fn new(fetcher: Arc<F>, config: EcosystemConfig) -> Self {
        Self {
            identity: IdentityModule::new(CompanyResearch::new(
                Arc::clone(&fetcher),
                config.company,
            )),
            website: WebsiteModule::new(Arc::clone(&fetcher), config.website),
            hiring: HiringModule::new(AtsScanner::new(
                fetcher,
                config.max_career_paths,
                config.max_rate_limit_wait,
            )),
        }
    }

    pub fn runners(&self) -> [EcosystemRunner<'_, F>; 3] {
        [
            EcosystemRunner::OfficialIdentity(&self.identity),
            EcosystemRunner::WebsiteStack(&self.website),
            EcosystemRunner::HiringActivity(&self.hiring),
        ]
    }
}

pub enum EcosystemRunner<'a, F: Fetcher> {
    OfficialIdentity(&'a IdentityModule<F>),
    WebsiteStack(&'a WebsiteModule<F>),
    HiringActivity(&'a HiringModule<F>),
}

impl<F: Fetcher> ModuleRunner for EcosystemRunner<'_, F> {
    fn module(&self) -> EcosystemModule {
        match self {
            Self::OfficialIdentity(_) => EcosystemModule::OfficialIdentity,
            Self::WebsiteStack(_) => EcosystemModule::WebsiteStack,
            Self::HiringActivity(_) => EcosystemModule::HiringActivity,
        }
    }

    async fn inspect(
        &self,
        target: &InspectionTarget<'_>,
    ) -> Result<Vec<EcosystemFinding>, SourceError> {
        match self {
            Self::OfficialIdentity(module) => module.inspect(target).await,
            Self::WebsiteStack(module) => module.inspect(target).await,
            Self::HiringActivity(module) => module.inspect(target).await,
        }
    }
}

#[cfg(test)]
mod tests;
