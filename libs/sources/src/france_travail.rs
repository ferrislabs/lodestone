use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{Duration as StdDuration, Instant},
};

use chrono::{DateTime, Utc};
use common::Secret;
use fetch::{ApiFetcher, ApiRequest};
use lodestone_core::{
    ActiveProfile, Candidate, FetchError, FetchResponse, Provenance, SignalKind, SourceError,
    SourceName,
    definition::{KeywordSource, ProfileDefinition},
    triage::CandidateSource,
};
use serde_json::{Value, json};
use tokio::time::sleep;
use tracing::{debug, info};
use url::Url;

use crate::buyer_domain::{domain_of_email, domain_of_url};

pub const PAGE_SIZE: usize = 150;
pub const LAST_INDEX_CAP: usize = 3149;
pub const TEXT_MAX_CHARS: usize = 4000;
pub const SCOPE: &str = "api_offresdemploiv2 o2dsoffre";
pub const TOKEN_EXPIRY_MARGIN: StdDuration = StdDuration::from_secs(60);
const ALLOWED_PUBLISHED_SINCE: [u32; 5] = [1, 3, 7, 14, 31];
const DEFAULT_EXPIRES_IN: u64 = 1499;
const MAX_RATE_LIMIT_WAIT: StdDuration = StdDuration::from_secs(75);
const MAX_THROTTLE_RETRIES: u8 = 2;
const DEFAULT_RETRY_AFTER: StdDuration = StdDuration::from_secs(60);
const OFFER_URL: &str = "https://candidat.francetravail.fr/offres/recherche/detail/";

#[derive(Debug, Clone)]
pub struct FranceTravailSettings {
    pub token_url: Url,
    pub api_base_url: Url,
    pub client_id: Secret,
    pub client_secret: Secret,
    pub published_since_days: u32,
    pub max_pages: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FranceTravailStats {
    pub queries: u64,
    pub requests: u64,
    pub total_matching: u64,
    pub records: u64,
    pub duplicates: u64,
    pub candidates: u64,
    pub skipped_no_domain: u64,
    pub skipped_malformed: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Skip {
    Malformed,
    NoDomain,
}

struct CachedToken {
    value: Secret,
    expires_at: Instant,
}

enum Page {
    Offers {
        offers: Vec<Value>,
        total: Option<u64>,
    },
    Empty,
}

pub struct FranceTravailSource<F: ApiFetcher> {
    fetcher: Arc<F>,
    settings: FranceTravailSettings,
    token: Mutex<Option<CachedToken>>,
}

pub fn closest_published_since(days: u32) -> u32 {
    ALLOWED_PUBLISHED_SINCE
        .iter()
        .copied()
        .min_by_key(|allowed| (allowed.abs_diff(days), u32::MAX - allowed))
        .unwrap_or(7)
}

pub fn query_keywords(profiles: &[ActiveProfile]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut queries = Vec::new();
    for rules in profiles
        .iter()
        .filter_map(|active| active.revision.definition.definition().france_travail())
    {
        for keyword in &rules.keywords {
            let keyword = keyword.trim();
            if !keyword.is_empty() && seen.insert(keyword.to_lowercase()) {
                queries.push(keyword.to_owned());
            }
        }
    }
    queries
}

pub fn content_range_total(header: &str) -> Option<u64> {
    header.rsplit_once('/')?.1.trim().parse().ok()
}

fn text_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn bounded_text(parts: &[Option<&str>]) -> String {
    let mut text = String::new();
    for part in parts.iter().flatten() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(part);
        if text.chars().count() >= TEXT_MAX_CHARS {
            break;
        }
    }
    text.chars().take(TEXT_MAX_CHARS).collect()
}

pub fn parse_offer(offer: &Value) -> Result<Candidate, Skip> {
    let id = text_of(offer, "id").ok_or(Skip::Malformed)?;
    let title = text_of(offer, "intitule").ok_or(Skip::Malformed)?;
    let created = text_of(offer, "dateCreation").ok_or(Skip::Malformed)?;
    let observed_at = DateTime::parse_from_rfc3339(created)
        .map_err(|_| Skip::Malformed)?
        .with_timezone(&Utc);
    let company = offer.get("entreprise");
    let domain = company
        .and_then(|company| text_of(company, "url"))
        .and_then(domain_of_url)
        .or_else(|| {
            offer
                .get("contact")
                .and_then(|contact| text_of(contact, "courriel"))
                .and_then(domain_of_email)
        })
        .ok_or(Skip::NoDomain)?;
    let provenance = Provenance::parse(&format!("{OFFER_URL}{id}")).map_err(|_| Skip::Malformed)?;
    let kind = SignalKind::parse("job_offer").map_err(|_| Skip::Malformed)?;
    let text = bounded_text(&[
        text_of(offer, "romeLibelle"),
        text_of(offer, "description"),
        company.and_then(|company| text_of(company, "description")),
    ]);
    Ok(Candidate {
        domain,
        source: SourceName::FranceTravail,
        kind,
        title: title.to_owned(),
        text,
        provenance,
        observed_at,
        details: json!({
            "offer_id": id,
            "company": company.and_then(|company| text_of(company, "nom")),
            "contract": text_of(offer, "typeContrat"),
            "location": offer.get("lieuTravail").and_then(|place| text_of(place, "libelle")),
            "published": created,
            "rome": text_of(offer, "romeCode"),
        }),
    })
}

fn upstream(message: impl Into<String>) -> SourceError {
    SourceError::Upstream(message.into())
}

fn retry_after(response: &FetchResponse) -> StdDuration {
    response
        .header("retry-after")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map_or(DEFAULT_RETRY_AFTER, StdDuration::from_secs)
}

impl<F: ApiFetcher> FranceTravailSource<F> {
    pub fn new(fetcher: Arc<F>, settings: FranceTravailSettings) -> Self {
        Self {
            fetcher,
            settings,
            token: Mutex::new(None),
        }
    }

    async fn call(&self, request: &ApiRequest) -> Result<FetchResponse, SourceError> {
        let mut attempts: u8 = 0;
        loop {
            match self.fetcher.call(request).await {
                Ok(response) => return Ok(response),
                Err(FetchError::RateLimited { retry_after })
                    if attempts < MAX_THROTTLE_RETRIES && retry_after <= MAX_RATE_LIMIT_WAIT =>
                {
                    attempts += 1;
                    sleep(retry_after).await;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    fn cached_token(&self) -> Option<Secret> {
        let guard = self.token.lock().ok()?;
        let cached = guard.as_ref()?;
        let deadline = Instant::now().checked_add(TOKEN_EXPIRY_MARGIN)?;
        (cached.expires_at > deadline).then(|| cached.value.clone())
    }

    fn forget_token(&self) {
        if let Ok(mut guard) = self.token.lock() {
            *guard = None;
        }
    }

    async fn token(&self) -> Result<Secret, SourceError> {
        if let Some(token) = self.cached_token() {
            return Ok(token);
        }
        let request = ApiRequest {
            url: self.settings.token_url.clone(),
            source: SourceName::FranceTravail,
            bearer: None,
            form: Some(vec![
                ("grant_type".to_owned(), Secret::new("client_credentials")),
                ("client_id".to_owned(), self.settings.client_id.clone()),
                (
                    "client_secret".to_owned(),
                    self.settings.client_secret.clone(),
                ),
                ("scope".to_owned(), Secret::new(SCOPE)),
            ]),
        };
        let response = self.call(&request).await?;
        if response.status != 200 {
            return Err(upstream(format!(
                "france travail token endpoint answered {}",
                response.status
            )));
        }
        let body: Value = serde_json::from_slice(&response.body)
            .map_err(|_| upstream("france travail token response is not json"))?;
        let value = body
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .ok_or_else(|| upstream("france travail token response has no access_token"))?;
        let lifetime = body
            .get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_EXPIRES_IN);
        let token = Secret::new(value);
        let expires_at = Instant::now()
            .checked_add(StdDuration::from_secs(lifetime))
            .unwrap_or_else(Instant::now);
        if let Ok(mut guard) = self.token.lock() {
            *guard = Some(CachedToken {
                value: token.clone(),
                expires_at,
            });
        }
        Ok(token)
    }

    fn search_url(&self, keyword: &str, first: usize) -> Result<Url, SourceError> {
        let mut url = self.settings.api_base_url.clone();
        url.path_segments_mut()
            .map_err(|_| upstream("france travail api base url cannot carry a path"))?
            .pop_if_empty()
            .push("offres")
            .push("search");
        let last = first + PAGE_SIZE - 1;
        url.query_pairs_mut()
            .append_pair("motsCles", keyword)
            .append_pair("range", &format!("{first}-{last}"))
            .append_pair(
                "publieeDepuis",
                &closest_published_since(self.settings.published_since_days).to_string(),
            );
        Ok(url)
    }

    async fn fetch_page(&self, keyword: &str, first: usize) -> Result<Page, SourceError> {
        let url = self.search_url(keyword, first)?;
        let mut refreshed = false;
        let mut throttled: u8 = 0;
        loop {
            let request = ApiRequest {
                url: url.clone(),
                source: SourceName::FranceTravail,
                bearer: Some(self.token().await?),
                form: None,
            };
            let response = self.call(&request).await?;
            match response.status {
                200 | 206 => {
                    if response.truncated {
                        return Err(upstream("france travail response exceeded the body cap"));
                    }
                    let mut body: Value = serde_json::from_slice(&response.body)
                        .map_err(|_| upstream("france travail search response is not json"))?;
                    let offers = match body.get_mut("resultats").map(Value::take) {
                        Some(Value::Array(offers)) => offers,
                        _ => return Err(upstream("france travail response has no resultats")),
                    };
                    let total = response
                        .header("content-range")
                        .and_then(content_range_total)
                        .filter(|_| response.status == 206);
                    return Ok(Page::Offers { offers, total });
                }
                204 => return Ok(Page::Empty),
                401 if !refreshed => {
                    refreshed = true;
                    self.forget_token();
                }
                429 => {
                    let wait = retry_after(&response);
                    if throttled >= MAX_THROTTLE_RETRIES || wait > MAX_RATE_LIMIT_WAIT {
                        return Err(SourceError::RateLimited { retry_after: wait });
                    }
                    throttled += 1;
                    sleep(wait).await;
                }
                status => {
                    return Err(upstream(format!("france travail answered {status}")));
                }
            }
        }
    }

    pub async fn collect_with_stats(
        &self,
        profiles: &[ActiveProfile],
    ) -> Result<(Vec<Candidate>, FranceTravailStats), SourceError> {
        let mut stats = FranceTravailStats::default();
        let mut seen: HashSet<String> = HashSet::new();
        let mut candidates = Vec::new();
        for keyword in query_keywords(profiles) {
            stats.queries += 1;
            for page in 0..self.settings.max_pages {
                let first = usize::try_from(page)
                    .unwrap_or(usize::MAX)
                    .saturating_mul(PAGE_SIZE);
                if first.saturating_add(PAGE_SIZE - 1) > LAST_INDEX_CAP {
                    break;
                }
                stats.requests += 1;
                let (offers, total) = match self.fetch_page(&keyword, first).await? {
                    Page::Empty => break,
                    Page::Offers { offers, total } => (offers, total),
                };
                if page == 0 {
                    stats.total_matching += total.unwrap_or(offers.len() as u64);
                }
                for offer in &offers {
                    stats.records += 1;
                    let id = text_of(offer, "id").unwrap_or_default();
                    if !id.is_empty() && !seen.insert(id.to_owned()) {
                        stats.duplicates += 1;
                        continue;
                    }
                    match parse_offer(offer) {
                        Ok(candidate) => {
                            stats.candidates += 1;
                            candidates.push(candidate);
                        }
                        Err(Skip::NoDomain) => {
                            stats.skipped_no_domain += 1;
                            debug!(id, "france travail offer skipped, no employer domain");
                        }
                        Err(Skip::Malformed) => {
                            stats.skipped_malformed += 1;
                            debug!(id, "france travail offer skipped, malformed record");
                        }
                    }
                }
                let reached = u64::try_from(first + offers.len()).unwrap_or(u64::MAX);
                match total {
                    Some(total) if reached < total && !offers.is_empty() => {}
                    _ => break,
                }
            }
        }
        info!(
            queries = stats.queries,
            requests = stats.requests,
            total_matching = stats.total_matching,
            records = stats.records,
            duplicates = stats.duplicates,
            candidates = stats.candidates,
            skipped_no_domain = stats.skipped_no_domain,
            skipped_malformed = stats.skipped_malformed,
            "france travail fetch finished"
        );
        Ok((candidates, stats))
    }
}

impl<F: ApiFetcher> CandidateSource for FranceTravailSource<F> {
    fn name(&self) -> SourceName {
        SourceName::FranceTravail
    }

    fn rules<'a>(&self, profile: &'a ProfileDefinition) -> Option<&'a KeywordSource> {
        profile.france_travail()
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
