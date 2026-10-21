use std::{collections::BTreeSet, collections::HashSet, sync::Arc, time::Duration as StdDuration};

use chrono::{DateTime, Utc};
use common::Secret;
use fetch::{ApiFetcher, ApiRequest};
use lodestone_core::{
    ActiveProfile, Candidate, FetchError, FetchResponse, Provenance, SignalKind, SourceError,
    SourceName,
    definition::{ContractFilter, SearchAgent},
    triage::CandidateSource,
};
use serde_json::{Value, json};
use tokio::time::sleep;
use tracing::{debug, info};
use url::Url;

use crate::{
    buyer_domain::{domain_of_email, domain_of_url},
    search::query_groups,
};

pub const PAGE_SIZE: usize = 150;
pub const LAST_INDEX_CAP: usize = 3149;
pub const TEXT_MAX_CHARS: usize = 4000;
const ALLOWED_PUBLISHED_SINCE: [u32; 5] = [1, 3, 7, 14, 31];
const MAX_RATE_LIMIT_WAIT: StdDuration = StdDuration::from_secs(75);
const MAX_THROTTLE_RETRIES: u8 = 2;
const DEFAULT_RETRY_AFTER: StdDuration = StdDuration::from_secs(60);
const OFFER_URL: &str = "https://candidat.francetravail.fr/offres/recherche/detail/";

#[derive(Debug, Clone)]
pub struct FranceTravailSettings {
    pub published_since_days: u32,
    pub max_pages: u32,
}

#[derive(Debug, Clone)]
pub struct FranceTravailAccess {
    pub token: Secret,
    pub api_base_url: Url,
}

pub trait AccessProvider: Send + Sync {
    fn access(&self) -> impl Future<Output = Result<FranceTravailAccess, SourceError>> + Send;

    fn invalidate(&self);
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

enum Page {
    Offers {
        offers: Vec<Value>,
        total: Option<u64>,
    },
    Empty,
}

pub struct FranceTravailSource<F: ApiFetcher, A: AccessProvider> {
    fetcher: Arc<F>,
    access: A,
    settings: FranceTravailSettings,
}

pub fn closest_published_since(days: u32) -> u32 {
    ALLOWED_PUBLISHED_SINCE
        .iter()
        .copied()
        .min_by_key(|allowed| (allowed.abs_diff(days), u32::MAX - allowed))
        .unwrap_or(7)
}

const FREELANCE_CODES: [&str; 2] = ["LIB", "FRA"];
const EMPLOYEE_CODES: [&str; 4] = ["CDI", "CDD", "MIS", "SAI"];

pub fn native_contract(agent: &SearchAgent) -> Option<String> {
    if agent.filters.wants_any_contract() {
        return None;
    }
    let mut codes: BTreeSet<&str> = BTreeSet::new();
    for contract in &agent.filters.contracts {
        match contract {
            ContractFilter::Freelance => codes.extend(FREELANCE_CODES),
            ContractFilter::Employee => codes.extend(EMPLOYEE_CODES),
            ContractFilter::PublicContract | ContractFilter::Any => {}
        }
    }
    if codes.is_empty() {
        None
    } else {
        Some(codes.into_iter().collect::<Vec<_>>().join(","))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub keyword: String,
    pub contract: Option<String>,
}

pub fn query_plan(profiles: &[ActiveProfile]) -> Vec<Query> {
    query_groups(profiles, SourceName::FranceTravail, native_contract)
        .into_iter()
        .flat_map(|group| {
            let contract = group.native;
            group.keywords.into_iter().map(move |keyword| Query {
                keyword,
                contract: contract.clone(),
            })
        })
        .collect()
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

impl<F: ApiFetcher, A: AccessProvider> FranceTravailSource<F, A> {
    pub fn new(fetcher: Arc<F>, access: A, settings: FranceTravailSettings) -> Self {
        Self {
            fetcher,
            access,
            settings,
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

    fn search_url(&self, base: &Url, query: &Query, first: usize) -> Result<Url, SourceError> {
        let mut url = base.clone();
        url.path_segments_mut()
            .map_err(|_| upstream("france travail api base url cannot carry a path"))?
            .pop_if_empty()
            .push("offres")
            .push("search");
        let last = first + PAGE_SIZE - 1;
        url.query_pairs_mut()
            .append_pair("motsCles", &query.keyword)
            .append_pair("range", &format!("{first}-{last}"))
            .append_pair(
                "publieeDepuis",
                &closest_published_since(self.settings.published_since_days).to_string(),
            );
        if let Some(contract) = &query.contract {
            url.query_pairs_mut().append_pair("typeContrat", contract);
        }
        Ok(url)
    }

    async fn fetch_page(&self, query: &Query, first: usize) -> Result<Page, SourceError> {
        let mut refreshed = false;
        let mut throttled: u8 = 0;
        loop {
            let access = self.access.access().await?;
            let request = ApiRequest {
                url: self.search_url(&access.api_base_url, query, first)?,
                source: SourceName::FranceTravail,
                bearer: Some(access.token),
                headers: Vec::new(),
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
                    self.access.invalidate();
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
        for query in query_plan(profiles) {
            stats.queries += 1;
            for page in 0..self.settings.max_pages {
                let first = usize::try_from(page)
                    .unwrap_or(usize::MAX)
                    .saturating_mul(PAGE_SIZE);
                if first.saturating_add(PAGE_SIZE - 1) > LAST_INDEX_CAP {
                    break;
                }
                stats.requests += 1;
                let (offers, total) = match self.fetch_page(&query, first).await? {
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

impl<F: ApiFetcher, A: AccessProvider> CandidateSource for FranceTravailSource<F, A> {
    fn name(&self) -> SourceName {
        SourceName::FranceTravail
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
