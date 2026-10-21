use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use chrono::{DateTime, Utc};
use common::Secret;
use fetch::{ApiFetcher, ApiRequest};
use lodestone_core::{
    ActiveProfile, Candidate, FetchError, FetchResponse, LodestoneUseCase, NormalizedDomain,
    ProviderStatus, SourceError, SourceName, WELCOME_TO_THE_JUNGLE, provider_descriptor,
    triage::CandidateSource,
};
use serde_json::{Map, Value, from_slice};
use thiserror::Error;
use tracing::{debug, info, warn};
use url::{ParseError, Url};

use crate::{buyer_domain::domain_of_url, search::keyword_union};

mod discovery;
mod offer;
mod pacer;

pub use discovery::{AlgoliaEnv, jobs_index_in, parse_env, script_urls, search_base};
pub use offer::{TEXT_MAX_CHARS, contract_label, is_offer_url, remote_label};
pub use pacer::{Pacer, spacing};

use offer::{Company, id_of, parse_hit, website_of};

pub const SITE_URL: &str = "https://www.welcometothejungle.com/";
pub const PAGE_SIZE: usize = 30;
pub const MAX_DISCOVERY_SCRIPTS: usize = 8;
pub const COMPANY_PAGE_SIZE: usize = 5;
pub const DEFAULT_REQUESTS_PER_MINUTE: u32 = 10;
pub const MAX_REQUESTS_PER_MINUTE: u32 = 30;
pub const DEFAULT_MAX_PAGES: u32 = 3;
pub const MAX_PAGES: u32 = 3;
pub const DEFAULT_COMPANY_LOOKUPS: usize = 60;
const DISCOVERY_LANGUAGE: &str = "fr";
const COMPANY_CACHE_CAP: usize = 10_000;
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(60);
const REFERER_REFUSAL: &str = "Method not allowed with this referer";

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub site: Url,
    pub search: Option<Url>,
}

impl Endpoints {
    pub fn official() -> Result<Self, ParseError> {
        Ok(Self {
            site: Url::parse(SITE_URL)?,
            search: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    pub referer: String,
    pub jobs_index: Option<String>,
    pub requests_per_minute: u32,
    pub max_pages: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum NotReady {
    #[error("Welcome to the Jungle is not configured in the provider catalog")]
    NotConfigured,
    #[error("the Welcome to the Jungle authorisation is not declared")]
    NotAuthorised,
    #[error("the Welcome to the Jungle authorisation reference is missing")]
    MissingReference,
    #[error("the Referer authorised by Welcome to the Jungle is not set")]
    MissingReferer,
    #[error("the Referer is not an https URL on welcometothejungle.com")]
    InvalidReferer,
}

impl NotReady {
    pub fn explanation(self) -> &'static str {
        match self {
            Self::NotConfigured => "Welcome to the Jungle n'est pas configuré dans le catalogue.",
            Self::NotAuthorised => {
                "L'accès autorisé par Welcome to the Jungle n'est pas déclaré : cochez la case une fois l'autorisation obtenue."
            }
            Self::MissingReference => "La référence de l'autorisation n'est pas renseignée.",
            Self::MissingReferer => {
                "L'en-tête Referer autorisé par Welcome to the Jungle n'est pas renseigné."
            }
            Self::InvalidReferer => {
                "L'en-tête Referer doit être une adresse https sur welcometothejungle.com."
            }
        }
    }
}

fn bounded(settings: &Map<String, Value>, key: &str, default: u32, max: u32) -> u32 {
    settings
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 1.0)
        .map_or(default, |value| value.min(f64::from(max)) as u32)
}

pub fn access_of(settings: &Map<String, Value>) -> Result<Access, NotReady> {
    if settings.get("authorised") != Some(&Value::Bool(true)) {
        return Err(NotReady::NotAuthorised);
    }
    if text_of_map(settings, "authorisation_reference").is_none() {
        return Err(NotReady::MissingReference);
    }
    let referer = text_of_map(settings, "referer").ok_or(NotReady::MissingReferer)?;
    let field = provider_descriptor(WELCOME_TO_THE_JUNGLE)
        .and_then(|descriptor| descriptor.field("referer"))
        .ok_or(NotReady::NotConfigured)?;
    let valid = Url::parse(referer).is_ok_and(|url| {
        url.scheme() == "https" && url.host_str().is_some_and(|host| field.accepts_host(host))
    });
    if !valid {
        return Err(NotReady::InvalidReferer);
    }
    Ok(Access {
        referer: referer.to_owned(),
        jobs_index: text_of_map(settings, "jobs_index").map(str::to_owned),
        requests_per_minute: bounded(
            settings,
            "requests_per_minute",
            DEFAULT_REQUESTS_PER_MINUTE,
            MAX_REQUESTS_PER_MINUTE,
        ),
        max_pages: bounded(settings, "max_pages", DEFAULT_MAX_PAGES, MAX_PAGES),
    })
}

fn text_of_map<'a>(settings: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    settings
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

#[derive(Clone)]
pub enum AccessSource {
    Fixed(Access),
    Catalog(LodestoneUseCase),
}

impl AccessSource {
    async fn load(&self) -> Result<Access, SourceError> {
        match self {
            Self::Fixed(access) => Ok(access.clone()),
            Self::Catalog(usecase) => {
                let configuration = usecase
                    .get_provider_configuration(WELCOME_TO_THE_JUNGLE)
                    .await
                    .map_err(|error| SourceError::Upstream(format!("provider catalog: {error}")))?
                    .ok_or_else(|| SourceError::Upstream(NotReady::NotConfigured.to_string()))?;
                access_of(&configuration.settings)
                    .map_err(|error| SourceError::Upstream(error.to_string()))
            }
        }
    }

    async fn report_failing(&self, message: &str) {
        let Self::Catalog(usecase) = self else {
            return;
        };
        if let Err(error) = usecase
            .record_provider_status(
                WELCOME_TO_THE_JUNGLE,
                ProviderStatus::Failing,
                Some(message.to_owned()),
                Utc::now(),
            )
            .await
        {
            warn!(%error, "welcome to the jungle could not be marked as failing");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Fetch(FetchError),
    RefererRefused(String),
    Blocked { status: u16, challenge: bool },
    RateLimited(Duration),
    Missing(String),
    Status(u16),
    Truncated,
    Invalid(&'static str),
}

impl Failure {
    pub fn explanation(&self) -> String {
        match self {
            Self::Fetch(error) => format!("Welcome to the Jungle est injoignable ({error})."),
            Self::RefererRefused(detail) => format!(
                "Welcome to the Jungle a refusé l'en-tête Referer configuré (HTTP 403 « {detail} ») : vérifiez la valeur autorisée."
            ),
            Self::Blocked { status, challenge } => format!(
                "Welcome to the Jungle a bloqué la requête (HTTP {status}{}) : la collecte s'arrête, sans contournement.",
                if *challenge { ", défi anti-robot" } else { "" }
            ),
            Self::RateLimited(wait) => format!(
                "Welcome to the Jungle limite le débit (HTTP 429), nouvel essai après {} s.",
                wait.as_secs()
            ),
            Self::Missing(index) => format!(
                "L'index « {index} » est introuvable : corrigez le champ « Index des offres »."
            ),
            Self::Status(status) => format!("Welcome to the Jungle a répondu HTTP {status}."),
            Self::Truncated => {
                "La réponse de Welcome to the Jungle est trop volumineuse.".to_owned()
            }
            Self::Invalid(what) => {
                format!("La réponse de Welcome to the Jungle est invalide ({what}).")
            }
        }
    }

    fn marks_failing(&self) -> bool {
        matches!(
            self,
            Self::RefererRefused(_) | Self::Blocked { .. } | Self::Missing(_)
        )
    }
}

impl From<Failure> for SourceError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Fetch(error) => error.into(),
            Failure::RateLimited(retry_after) => Self::RateLimited { retry_after },
            Failure::RefererRefused(detail) => Self::Upstream(format!(
                "welcome to the jungle refused the configured referer (HTTP 403: {detail})"
            )),
            Failure::Blocked { status, challenge } => Self::Upstream(format!(
                "welcome to the jungle blocked the request (HTTP {status}{}), the run stops",
                if challenge { ", challenge" } else { "" }
            )),
            Failure::Missing(index) => Self::Upstream(format!(
                "welcome to the jungle index {index} does not exist, set jobs_index in the catalog"
            )),
            Failure::Status(status) => {
                Self::Upstream(format!("welcome to the jungle answered {status}"))
            }
            Failure::Truncated => {
                Self::Upstream("welcome to the jungle response exceeded the body cap".to_owned())
            }
            Failure::Invalid(what) => {
                Self::Upstream(format!("welcome to the jungle response is invalid: {what}"))
            }
        }
    }
}

fn retry_after(response: &FetchResponse) -> Duration {
    response
        .header("retry-after")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map_or(DEFAULT_RETRY_AFTER, Duration::from_secs)
}

fn is_challenge(response: &FetchResponse) -> bool {
    if response.header("cf-mitigated").is_some() || response.header("x-amzn-waf-action").is_some() {
        return true;
    }
    let body = String::from_utf8_lossy(&response.body).to_lowercase();
    body.contains("challenge") || body.contains("captcha")
}

fn refusal_detail(response: &FetchResponse) -> Option<String> {
    let body = String::from_utf8_lossy(&response.body);
    if !body.to_lowercase().contains("referer") {
        return None;
    }
    let detail = from_slice::<Value>(&response.body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| REFERER_REFUSAL.to_owned());
    Some(detail)
}

pub fn classify(response: FetchResponse, index: Option<&str>) -> Result<FetchResponse, Failure> {
    match response.status {
        200 => Ok(response),
        429 => Err(Failure::RateLimited(retry_after(&response))),
        403 => Err(match refusal_detail(&response) {
            Some(detail) => Failure::RefererRefused(detail),
            None => Failure::Blocked {
                status: 403,
                challenge: is_challenge(&response),
            },
        }),
        404 => Err(index.map_or(Failure::Status(404), |index| {
            Failure::Missing(index.to_owned())
        })),
        status if is_challenge(&response) => Err(Failure::Blocked {
            status,
            challenge: true,
        }),
        status => Err(Failure::Status(status)),
    }
}

fn json_of(response: &FetchResponse) -> Result<Value, Failure> {
    if response.truncated {
        return Err(Failure::Truncated);
    }
    from_slice(&response.body).map_err(|_| Failure::Invalid("not json"))
}

fn plain_request(url: Url) -> ApiRequest {
    ApiRequest {
        url,
        source: SourceName::WelcomeToTheJungle,
        bearer: None,
        headers: Vec::new(),
        form: None,
    }
}

fn env_request(endpoints: &Endpoints) -> Result<ApiRequest, Failure> {
    endpoints
        .site
        .join("api/env")
        .map(plain_request)
        .map_err(|_| Failure::Invalid("site url"))
}

fn search_root(endpoints: &Endpoints, env: &AlgoliaEnv) -> Result<Url, Failure> {
    match &endpoints.search {
        Some(url) => Ok(url.clone()),
        None => search_base(&env.application_id).ok_or(Failure::Invalid("application id")),
    }
}

fn index_request(
    root: &Url,
    index: &str,
    env: &AlgoliaEnv,
    referer: &str,
    pairs: &[(&str, &str)],
) -> Result<ApiRequest, Failure> {
    let mut url = root.clone();
    url.path_segments_mut()
        .map_err(|_| Failure::Invalid("search url"))?
        .pop_if_empty()
        .extend(["1", "indexes", index]);
    url.query_pairs_mut().extend_pairs(pairs);
    Ok(ApiRequest {
        url,
        source: SourceName::WelcomeToTheJungle,
        bearer: None,
        headers: vec![
            (
                "X-Algolia-Application-Id".to_owned(),
                Secret::new(env.application_id.as_str()),
            ),
            ("X-Algolia-API-Key".to_owned(), env.search_key.clone()),
            ("Referer".to_owned(), Secret::new(referer)),
        ],
        form: None,
    })
}

async fn call<F: ApiFetcher>(
    fetcher: &F,
    request: &ApiRequest,
    index: Option<&str>,
) -> Result<FetchResponse, Failure> {
    let response = fetcher.call(request).await.map_err(|error| match error {
        FetchError::RateLimited { retry_after } => Failure::RateLimited(retry_after),
        other => Failure::Fetch(other),
    })?;
    classify(response, index)
}

fn env_of(response: &FetchResponse) -> Result<AlgoliaEnv, Failure> {
    parse_env(&response.body).ok_or(Failure::Invalid("no algolia search key in /api/env"))
}

pub async fn check<F: ApiFetcher>(
    fetcher: &F,
    endpoints: &Endpoints,
    settings: &Map<String, Value>,
) -> Result<String, String> {
    let access = access_of(settings).map_err(|error| error.explanation().to_owned())?;
    let failed = |failure: Failure| failure.explanation();
    let request = env_request(endpoints).map_err(failed)?;
    let env = env_of(&call(fetcher, &request, None).await.map_err(failed)?).map_err(failed)?;
    let root = search_root(endpoints, &env).map_err(failed)?;
    let (index, later) = match access.jobs_index.as_deref().or(env.jobs_index.as_deref()) {
        Some(index) => (index, false),
        None => match env.organizations_index.as_deref() {
            Some(index) => (index, true),
            None => {
                return Err(
                    "Aucun index de recherche n'est publié : renseignez le champ « Index des offres »."
                        .to_owned(),
                );
            }
        },
    };
    let request = index_request(
        &root,
        index,
        &env,
        &access.referer,
        &[("query", ""), ("page", "0"), ("hitsPerPage", "1")],
    )
    .map_err(failed)?;
    let body =
        json_of(&call(fetcher, &request, Some(index)).await.map_err(failed)?).map_err(failed)?;
    if later {
        return Ok("La clé de recherche publique accepte l'en-tête Referer configuré ; l'index des offres sera découvert dans les scripts publics à la première collecte.".to_owned());
    }
    let total = body
        .get("nbHits")
        .and_then(Value::as_u64)
        .map(|total| format!(", {total} offres"))
        .unwrap_or_default();
    Ok(format!(
        "La recherche d'offres répond avec l'en-tête Referer configuré (index « {index} »{total})."
    ))
}

#[derive(Clone)]
pub struct WelcomeToTheJungleSettings {
    pub endpoints: Endpoints,
    pub access: AccessSource,
    pub max_company_lookups: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WelcomeToTheJungleStats {
    pub queries: u64,
    pub requests: u64,
    pub discovery_requests: u64,
    pub records: u64,
    pub duplicates: u64,
    pub candidates: u64,
    pub skipped_no_domain: u64,
    pub skipped_malformed: u64,
    pub skipped_company_budget: u64,
    pub company_lookups: u64,
    pub company_cache_hits: u64,
}

enum Lookup {
    Found(NormalizedDomain),
    Missing,
    OverBudget,
}

struct Run<'a> {
    access: &'a Access,
    env: &'a AlgoliaEnv,
    root: Url,
    stats: WelcomeToTheJungleStats,
    lookups: usize,
}

pub struct WelcomeToTheJungleSource<F: ApiFetcher> {
    fetcher: Arc<F>,
    settings: WelcomeToTheJungleSettings,
    pacer: Pacer,
    discovered: Mutex<Option<String>>,
    companies: Mutex<HashMap<String, Option<NormalizedDomain>>>,
}

impl<F: ApiFetcher> WelcomeToTheJungleSource<F> {
    pub fn new(fetcher: Arc<F>, settings: WelcomeToTheJungleSettings) -> Self {
        Self {
            fetcher,
            settings,
            pacer: Pacer::default(),
            discovered: Mutex::new(None),
            companies: Mutex::new(HashMap::new()),
        }
    }

    async fn send(
        &self,
        request: &ApiRequest,
        index: Option<&str>,
        access: &Access,
        stats: &mut WelcomeToTheJungleStats,
    ) -> Result<FetchResponse, SourceError> {
        self.pacer
            .wait(
                request.url.host_str().unwrap_or_default(),
                access.requests_per_minute,
            )
            .await;
        stats.requests += 1;
        match call(self.fetcher.as_ref(), request, index).await {
            Ok(response) => Ok(response),
            Err(failure) => {
                if failure.marks_failing() {
                    self.settings
                        .access
                        .report_failing(&failure.explanation())
                        .await;
                }
                Err(failure.into())
            }
        }
    }

    async fn env(
        &self,
        access: &Access,
        stats: &mut WelcomeToTheJungleStats,
    ) -> Result<AlgoliaEnv, SourceError> {
        let request = env_request(&self.settings.endpoints)?;
        let response = self.send(&request, None, access, stats).await?;
        Ok(env_of(&response)?)
    }

    fn remembered_index(&self) -> Option<String> {
        self.discovered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn remember_index(&self, index: Option<String>) {
        *self
            .discovered
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = index;
    }

    async fn discover(
        &self,
        access: &Access,
        stats: &mut WelcomeToTheJungleStats,
    ) -> Result<Option<String>, SourceError> {
        let site = &self.settings.endpoints.site;
        let page = site
            .join(&format!("{DISCOVERY_LANGUAGE}/jobs"))
            .map_err(|_| Failure::Invalid("site url"))?;
        stats.discovery_requests += 1;
        let html = self.send(&plain_request(page), None, access, stats).await?;
        let scripts = script_urls(&String::from_utf8_lossy(&html.body), site);
        for script in scripts.into_iter().take(MAX_DISCOVERY_SCRIPTS) {
            stats.discovery_requests += 1;
            let response = self
                .send(&plain_request(script), None, access, stats)
                .await?;
            if let Some(index) =
                jobs_index_in(&String::from_utf8_lossy(&response.body), DISCOVERY_LANGUAGE)
            {
                info!(index, "welcome to the jungle jobs index discovered");
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    async fn jobs_index(
        &self,
        access: &Access,
        env: &AlgoliaEnv,
        stats: &mut WelcomeToTheJungleStats,
    ) -> Result<String, SourceError> {
        if let Some(index) = access.jobs_index.as_ref().or(env.jobs_index.as_ref()) {
            return Ok(index.clone());
        }
        if let Some(index) = self.remembered_index() {
            return Ok(index);
        }
        match self.discover(access, stats).await? {
            Some(index) => {
                self.remember_index(Some(index.clone()));
                Ok(index)
            }
            None => {
                self.settings
                    .access
                    .report_failing("L'index des offres n'a pas été trouvé dans les scripts publics du site : renseignez le champ « Index des offres ».")
                    .await;
                Err(SourceError::Upstream(
                    "welcome to the jungle jobs index not found in the public scripts, set jobs_index in the catalog".to_owned(),
                ))
            }
        }
    }

    fn cached_company(&self, key: &str) -> Option<Option<NormalizedDomain>> {
        self.companies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned()
    }

    fn cache_company(&self, key: String, domain: Option<NormalizedDomain>) {
        let mut companies = self
            .companies
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if companies.len() >= COMPANY_CACHE_CAP {
            companies.clear();
        }
        companies.insert(key, domain);
    }

    async fn company_domain(
        &self,
        company: &Company<'_>,
        run: &mut Run<'_>,
    ) -> Result<Lookup, SourceError> {
        if let Some(domain) = company.own_domain() {
            return Ok(Lookup::Found(domain));
        }
        let key = company.cache_key();
        if let Some(cached) = self.cached_company(&key) {
            run.stats.company_cache_hits += 1;
            return Ok(cached.map_or(Lookup::Missing, Lookup::Found));
        }
        let Some(index) = run.env.organizations_index.as_deref() else {
            return Ok(Lookup::Missing);
        };
        if run.lookups >= self.settings.max_company_lookups {
            return Ok(Lookup::OverBudget);
        }
        run.lookups += 1;
        run.stats.company_lookups += 1;
        let size = COMPANY_PAGE_SIZE.to_string();
        let request = index_request(
            &run.root,
            index,
            run.env,
            &run.access.referer,
            &[
                ("query", company.name),
                ("page", "0"),
                ("hitsPerPage", &size),
            ],
        )?;
        let response = self
            .send(&request, Some(index), run.access, &mut run.stats)
            .await?;
        let body = json_of(&response)?;
        let domain = body
            .get("hits")
            .and_then(Value::as_array)
            .and_then(|hits| hits.iter().find(|hit| company.is(hit)))
            .and_then(website_of)
            .and_then(domain_of_url);
        self.cache_company(key, domain.clone());
        Ok(domain.map_or(Lookup::Missing, Lookup::Found))
    }

    async fn search_page(
        &self,
        index: &str,
        keyword: &str,
        page: u32,
        run: &mut Run<'_>,
    ) -> Result<Value, SourceError> {
        let page = page.to_string();
        let size = PAGE_SIZE.to_string();
        let request = index_request(
            &run.root,
            index,
            run.env,
            &run.access.referer,
            &[("query", keyword), ("page", &page), ("hitsPerPage", &size)],
        )?;
        let response = self
            .send(&request, Some(index), run.access, &mut run.stats)
            .await;
        if matches!(response, Err(SourceError::Upstream(_))) && run.access.jobs_index.is_none() {
            self.remember_index(None);
        }
        Ok(json_of(&response?)?)
    }

    async fn read_hit(
        &self,
        hit: &Value,
        run: &mut Run<'_>,
        candidates: &mut Vec<Candidate>,
    ) -> Result<(), SourceError> {
        let offer = match parse_hit(hit) {
            Ok(offer) => offer,
            Err(_) => {
                run.stats.skipped_malformed += 1;
                debug!("welcome to the jungle offer skipped, malformed record");
                return Ok(());
            }
        };
        match self.company_domain(&offer.company, run).await? {
            Lookup::Found(domain) => match offer.candidate(domain) {
                Ok(candidate) => {
                    run.stats.candidates += 1;
                    candidates.push(candidate);
                }
                Err(_) => run.stats.skipped_malformed += 1,
            },
            Lookup::Missing => {
                run.stats.skipped_no_domain += 1;
                debug!(
                    offer = offer.id,
                    "welcome to the jungle offer skipped, no company domain"
                );
            }
            Lookup::OverBudget => run.stats.skipped_company_budget += 1,
        }
        Ok(())
    }

    pub async fn collect_with_stats(
        &self,
        profiles: &[ActiveProfile],
    ) -> Result<(Vec<Candidate>, WelcomeToTheJungleStats), SourceError> {
        let keywords = keyword_union(profiles, SourceName::WelcomeToTheJungle);
        if keywords.is_empty() {
            return Ok((Vec::new(), WelcomeToTheJungleStats::default()));
        }
        let access = self.settings.access.load().await?;
        let mut stats = WelcomeToTheJungleStats::default();
        let env = self.env(&access, &mut stats).await?;
        let index = self.jobs_index(&access, &env, &mut stats).await?;
        let mut run = Run {
            access: &access,
            env: &env,
            root: search_root(&self.settings.endpoints, &env)?,
            stats,
            lookups: 0,
        };
        let mut seen: HashSet<String> = HashSet::new();
        let mut candidates = Vec::new();
        for keyword in &keywords {
            run.stats.queries += 1;
            for page in 0..access.max_pages {
                let body = self.search_page(&index, keyword, page, &mut run).await?;
                let hits = body
                    .get("hits")
                    .and_then(Value::as_array)
                    .ok_or(Failure::Invalid("no hits"))?;
                for hit in hits {
                    run.stats.records += 1;
                    if let Some(id) = id_of(hit, "objectID")
                        && !seen.insert(id)
                    {
                        run.stats.duplicates += 1;
                        continue;
                    }
                    self.read_hit(hit, &mut run, &mut candidates).await?;
                }
                let more = match body.get("nbPages").and_then(Value::as_u64) {
                    Some(total) => u64::from(page) + 1 < total,
                    None => hits.len() >= PAGE_SIZE,
                };
                if hits.is_empty() || !more {
                    break;
                }
            }
        }
        let stats = run.stats;
        info!(
            queries = stats.queries,
            requests = stats.requests,
            discovery_requests = stats.discovery_requests,
            records = stats.records,
            duplicates = stats.duplicates,
            candidates = stats.candidates,
            skipped_no_domain = stats.skipped_no_domain,
            skipped_malformed = stats.skipped_malformed,
            skipped_company_budget = stats.skipped_company_budget,
            company_lookups = stats.company_lookups,
            company_cache_hits = stats.company_cache_hits,
            "welcome to the jungle fetch finished"
        );
        Ok((candidates, stats))
    }
}

impl<F: ApiFetcher> CandidateSource for WelcomeToTheJungleSource<F> {
    fn name(&self) -> SourceName {
        SourceName::WelcomeToTheJungle
    }

    async fn collect(
        &self,
        profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        self.collect_with_stats(profiles)
            .await
            .map(|(candidates, _)| candidates)
    }
}

#[cfg(test)]
mod tests;
