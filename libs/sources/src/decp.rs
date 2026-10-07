use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use chrono::{DateTime, Datelike, Duration as ChronoDuration, NaiveDate, Utc};
use lodestone_core::{
    ActiveProfile, Candidate, FetchError, FetchPurpose, FetchRequest, FetchResponse, Fetcher,
    NormalizedDomain, PartnerAward, Provenance, Route, SignalKind, Siret, SourceError, SourceName,
    definition::SearchAgent, route, triage::CandidateSource,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{debug, info, warn};
use url::Url;

use crate::{buyer_domain::domain_of_url, endpoint::Endpoint, search::search_agents};

pub const DATASET_API_URL: &str = "https://www.data.gouv.fr/api/1/datasets/donnees-essentielles-de-la-commande-publique-fichiers-consolides/";
pub const ANNUAIRE_URL: &str = "https://api-lannuaire.service-public.fr/api/explore/v2.1/catalog/datasets/api-lannuaire-administration/records";
pub const ENTREPRISES_URL: &str = "https://recherche-entreprises.api.gouv.fr/search";
pub const DEFAULT_LOOKBACK_DAYS: u32 = 31;
pub const DEFAULT_NOTIFIED_WITHIN_DAYS: u32 = 180;
const RESOURCE_BASE: &str = "https://www.data.gouv.fr/fr/datasets/r/";
const LOOKUP_ATTEMPTS: usize = 5;

#[derive(Debug, Clone)]
pub struct DecpSettings {
    pub dataset_api_url: Endpoint,
    pub annuaire_url: Url,
    pub entreprises_url: Endpoint,
    pub lookback_days: u32,
    pub notified_within_days: u32,
    pub max_body_bytes: usize,
    pub max_rate_limit_wait: Duration,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DecpStats {
    pub files: u64,
    pub bytes: u64,
    pub contracts: u64,
    pub in_window: u64,
    pub matched: u64,
    pub duplicates: u64,
    pub candidates: u64,
    pub unresolved_buyers: u64,
    pub skipped_malformed: u64,
    pub holders: u64,
    pub lookups: u64,
    pub files_used: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holder {
    pub siret: Siret,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AwardedContract {
    pub key: String,
    pub contract_id: String,
    pub buyer: Siret,
    pub object: String,
    pub amount: Option<f64>,
    pub notified_on: NaiveDate,
    pub provenance: Provenance,
    pub content: String,
    pub holders: Vec<Holder>,
}

pub struct DecpSource<F: Fetcher> {
    fetcher: Arc<F>,
    settings: DecpSettings,
    awards: Mutex<Vec<AwardedContract>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Loose {
    Text(String),
    Number(serde_json::Number),
    Other(serde::de::IgnoredAny),
}

impl Loose {
    fn text(self) -> Option<String> {
        match self {
            Self::Text(text) => Some(text),
            Self::Number(number) => Some(number.to_string()),
            Self::Other(_) => None,
        }
    }

    fn number(self) -> Option<f64> {
        match self {
            Self::Text(text) => text.trim().parse().ok(),
            Self::Number(number) => number.as_f64(),
            Self::Other(_) => None,
        }
    }
}

fn loose_text<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Ok(Option::<Loose>::deserialize(deserializer)?.and_then(Loose::text))
}

fn loose_number<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<f64>, D::Error> {
    Ok(Option::<Loose>::deserialize(deserializer)?
        .and_then(Loose::number)
        .filter(|value| value.is_finite()))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LooseList<T> {
    List(Vec<T>),
    Other(serde::de::IgnoredAny),
}

fn loose_list<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(match Option::<LooseList<T>>::deserialize(deserializer)? {
        Some(LooseList::List(list)) => list,
        _ => Vec::new(),
    })
}

#[derive(Deserialize)]
struct DecpFile {
    marches: Marches,
}

#[derive(Deserialize)]
struct Marches {
    #[serde(default)]
    marche: Vec<Marche>,
}

#[derive(Deserialize)]
struct Marche {
    #[serde(default, deserialize_with = "loose_text")]
    id: Option<String>,
    acheteur: Option<Party>,
    #[serde(default, deserialize_with = "loose_text")]
    objet: Option<String>,
    #[serde(default, rename = "codeCPV", deserialize_with = "loose_text")]
    code_cpv: Option<String>,
    #[serde(default, deserialize_with = "loose_text")]
    procedure: Option<String>,
    #[serde(default, rename = "dateNotification", deserialize_with = "loose_text")]
    date_notification: Option<String>,
    #[serde(default, deserialize_with = "loose_number")]
    montant: Option<f64>,
    #[serde(default, deserialize_with = "loose_list")]
    titulaires: Vec<Titulaire>,
}

#[derive(Deserialize)]
struct Party {
    #[serde(default, deserialize_with = "loose_text")]
    id: Option<String>,
}

#[derive(Deserialize)]
struct Titulaire {
    titulaire: Option<HolderId>,
}

#[derive(Deserialize)]
struct HolderId {
    #[serde(default, rename = "typeIdentifiant", deserialize_with = "loose_text")]
    kind: Option<String>,
    #[serde(default, deserialize_with = "loose_text")]
    id: Option<String>,
}

#[derive(Deserialize)]
struct Dataset {
    #[serde(default)]
    resources: Vec<Resource>,
}

#[derive(Deserialize)]
struct Resource {
    id: String,
    title: Option<String>,
    url: Option<String>,
    filesize: Option<u64>,
}

#[derive(Deserialize)]
struct AnnuaireAnswer {
    #[serde(default)]
    results: Vec<AnnuaireEntry>,
}

#[derive(Deserialize)]
struct AnnuaireEntry {
    site_internet: Option<Value>,
}

#[derive(Deserialize)]
struct Site {
    valeur: Option<String>,
}

#[derive(Deserialize)]
struct EntreprisesAnswer {
    #[serde(default)]
    results: Vec<Entreprise>,
}

#[derive(Deserialize)]
struct Entreprise {
    nom_complet: Option<String>,
}

fn upstream(message: impl Into<String>) -> SourceError {
    SourceError::Upstream(message.into())
}

pub fn resource_title(year: i32, month: u32) -> String {
    format!("decp-{year}-{month:02}.json")
}

fn months_newest_first(since: NaiveDate, today: NaiveDate) -> Vec<(i32, u32)> {
    let mut months = Vec::new();
    let (mut year, mut month) = (today.year(), today.month());
    while (year, month) >= (since.year(), since.month()) {
        months.push((year, month));
        if month == 1 {
            year -= 1;
            month = 12;
        } else {
            month -= 1;
        }
    }
    months
}

fn rules_of(profiles: &[ActiveProfile]) -> Vec<&SearchAgent> {
    search_agents(profiles, SourceName::Decp).collect()
}

fn candidate_content(title: &str, text: &str) -> String {
    format!("{title}\n\n{text}")
}

fn midnight(date: NaiveDate) -> Option<DateTime<Utc>> {
    date.and_hms_opt(0, 0, 0).map(|naive| naive.and_utc())
}

pub fn partner_awards(rules: &SearchAgent, contracts: &[AwardedContract]) -> Vec<PartnerAward> {
    let mut awards = Vec::new();
    for contract in contracts {
        let matched = match route(&contract.content, rules) {
            Route::Reject => continue,
            Route::Accept { matched } | Route::Ambiguous { matched, .. } => matched,
        };
        for holder in &contract.holders {
            awards.push(PartnerAward {
                holder: holder.siret.clone(),
                holder_name: holder.name.clone(),
                contract_id: contract.key.clone(),
                buyer: contract.buyer.clone(),
                object: contract.object.clone(),
                amount: contract.amount,
                notified_on: contract.notified_on,
                provenance: contract.provenance.clone(),
                matched: matched.clone(),
            });
        }
    }
    awards
}

#[derive(Default)]
struct Lookups {
    entreprises_url: Option<Url>,
    domains: HashMap<Siret, Option<NormalizedDomain>>,
    names: HashMap<Siret, Option<String>>,
    requests: u64,
}

#[derive(Default)]
struct RunState {
    stats: DecpStats,
    seen: HashSet<String>,
    lookups: Lookups,
    candidates: Vec<Candidate>,
    awards: Vec<AwardedContract>,
}

struct Window {
    since: NaiveDate,
    today: NaiveDate,
}

impl Window {
    fn contains(&self, date: NaiveDate) -> bool {
        date >= self.since && date <= self.today
    }
}

struct Parsed {
    contract_id: String,
    buyer: Siret,
    object: String,
    cpv: Option<String>,
    text: String,
    amount: Option<f64>,
    holders: Vec<Siret>,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

fn parse_marche(marche: Marche) -> Option<Parsed> {
    let contract_id = non_empty(marche.id)?;
    let buyer = marche
        .acheteur
        .and_then(|party| party.id)
        .and_then(|id| Siret::parse(&id).ok())?;
    let object = non_empty(marche.objet)?;
    let cpv = non_empty(marche.code_cpv);
    let procedure = non_empty(marche.procedure);
    let mut text = object.clone();
    if let Some(cpv) = &cpv {
        text.push_str("\nCPV: ");
        text.push_str(cpv);
    }
    if let Some(procedure) = &procedure {
        text.push_str("\nProcédure: ");
        text.push_str(procedure);
    }
    let mut holders: Vec<Siret> = Vec::new();
    for siret in marche
        .titulaires
        .into_iter()
        .filter_map(|entry| entry.titulaire)
        .filter(|holder| {
            holder
                .kind
                .as_deref()
                .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("SIRET"))
        })
        .filter_map(|holder| holder.id)
        .filter_map(|id| Siret::parse(&id).ok())
    {
        if !holders.contains(&siret) {
            holders.push(siret);
        }
    }
    Some(Parsed {
        contract_id,
        buyer,
        object,
        cpv,
        text,
        amount: marche.montant,
        holders,
    })
}

fn site_domain(value: &Value) -> Option<NormalizedDomain> {
    let parsed: Vec<Site> = match value {
        Value::String(raw) => serde_json::from_str(raw).ok()?,
        Value::Array(_) => serde_json::from_value(value.clone()).ok()?,
        _ => return None,
    };
    parsed
        .iter()
        .filter_map(|site| site.valeur.as_deref())
        .find_map(domain_of_url)
}

impl<F: Fetcher> DecpSource<F> {
    pub fn new(fetcher: Arc<F>, settings: DecpSettings) -> Self {
        Self {
            fetcher,
            settings,
            awards: Mutex::new(Vec::new()),
        }
    }

    pub fn take_awards(&self) -> Vec<AwardedContract> {
        std::mem::take(&mut *self.awards.lock().unwrap_or_else(PoisonError::into_inner))
    }

    async fn fetch_waiting(&self, url: Url) -> Result<FetchResponse, SourceError> {
        let request = FetchRequest {
            url,
            purpose: FetchPurpose::Api,
            source: SourceName::Decp,
        };
        let mut attempt = 1;
        loop {
            match self.fetcher.fetch(&request).await {
                Err(FetchError::RateLimited { retry_after })
                    if attempt < LOOKUP_ATTEMPTS
                        && retry_after <= self.settings.max_rate_limit_wait =>
                {
                    attempt += 1;
                    tokio::time::sleep(retry_after).await;
                }
                other => return other.map_err(SourceError::from),
            }
        }
    }

    async fn fetch_json<T: for<'de> Deserialize<'de>>(&self, url: Url) -> Result<T, SourceError> {
        let response = self.fetch_waiting(url).await?;
        if response.status != 200 {
            return Err(upstream(format!("answered {}", response.status)));
        }
        if response.truncated {
            return Err(upstream("response exceeded the body cap"));
        }
        serde_json::from_slice(&response.body)
            .map_err(|error| upstream(format!("response is not the expected json: {error}")))
    }

    async fn buyer_domain(&self, siret: &Siret, lookups: &mut Lookups) -> Option<NormalizedDomain> {
        if let Some(known) = lookups.domains.get(siret) {
            return known.clone();
        }
        let mut url = self.settings.annuaire_url.clone();
        url.query_pairs_mut()
            .append_pair("where", &format!("siret=\"{siret}\""))
            .append_pair("limit", "1")
            .append_pair("select", "nom,siret,site_internet");
        lookups.requests += 1;
        let domain = match self.fetch_json::<AnnuaireAnswer>(url).await {
            Ok(answer) => answer
                .results
                .iter()
                .filter_map(|entry| entry.site_internet.as_ref())
                .find_map(site_domain),
            Err(error) => {
                warn!(%siret, %error, "annuaire lookup failed");
                None
            }
        };
        lookups.domains.insert(siret.clone(), domain.clone());
        domain
    }

    async fn name_of(&self, siret: &Siret, lookups: &mut Lookups) -> Option<String> {
        if let Some(known) = lookups.names.get(siret) {
            return known.clone();
        }
        let mut url = lookups.entreprises_url.clone()?;
        url.query_pairs_mut()
            .append_pair("q", siret.as_str())
            .append_pair("per_page", "1");
        lookups.requests += 1;
        let name = match self.fetch_json::<EntreprisesAnswer>(url).await {
            Ok(answer) => answer
                .results
                .into_iter()
                .find_map(|entry| non_empty(entry.nom_complet)),
            Err(error) => {
                warn!(%siret, %error, "company name lookup failed");
                None
            }
        };
        lookups.names.insert(siret.clone(), name.clone());
        name
    }

    async fn download(&self, resource: &Resource, url: Url) -> Result<Vec<u8>, SourceError> {
        let too_large = resource.filesize.is_some_and(|size| {
            usize::try_from(size).map_or(true, |size| size > self.settings.max_body_bytes)
        });
        if too_large {
            return Err(upstream(format!(
                "decp file {} is larger than the {} byte cap",
                resource.title.as_deref().unwrap_or(&resource.id),
                self.settings.max_body_bytes
            )));
        }
        let response = self.fetch_waiting(url).await?;
        if response.status != 200 {
            return Err(upstream(format!(
                "decp download answered {}",
                response.status
            )));
        }
        if response.truncated {
            return Err(upstream("decp file exceeded the body cap"));
        }
        Ok(response.body)
    }

    async fn process_file(
        &self,
        resource: &Resource,
        body: &[u8],
        rules: &[&SearchAgent],
        window: &Window,
        state: &mut RunState,
    ) -> Result<(), SourceError> {
        let file: DecpFile = serde_json::from_slice(body)
            .map_err(|error| upstream(format!("decp file is not the expected json: {error}")))?;
        for marche in file.marches.marche {
            state.stats.contracts += 1;
            let Some(notified_on) = marche
                .date_notification
                .as_deref()
                .and_then(|raw| NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").ok())
            else {
                state.stats.skipped_malformed += 1;
                continue;
            };
            if !window.contains(notified_on) {
                continue;
            }
            state.stats.in_window += 1;
            let Some(parsed) = parse_marche(marche) else {
                state.stats.skipped_malformed += 1;
                continue;
            };
            let content = candidate_content(&parsed.object, &parsed.text);
            if !rules
                .iter()
                .any(|rules| route(&content, rules) != Route::Reject)
            {
                continue;
            }
            state.stats.matched += 1;
            let key = format!("{}-{}", parsed.buyer, parsed.contract_id);
            if !state.seen.insert(key.clone()) {
                state.stats.duplicates += 1;
                continue;
            }
            let Ok(provenance) =
                Provenance::parse(&format!("{RESOURCE_BASE}{}#{key}", resource.id))
            else {
                state.stats.skipped_malformed += 1;
                continue;
            };
            let mut holders = Vec::with_capacity(parsed.holders.len());
            for siret in &parsed.holders {
                let name = self.name_of(siret, &mut state.lookups).await;
                holders.push(Holder {
                    siret: siret.clone(),
                    name,
                });
            }
            state.stats.holders += u64::try_from(holders.len()).unwrap_or(u64::MAX);
            match self.buyer_domain(&parsed.buyer, &mut state.lookups).await {
                Some(domain) => {
                    let buyer_name = self.name_of(&parsed.buyer, &mut state.lookups).await;
                    let candidate = SignalKind::parse("awarded_contract")
                        .ok()
                        .zip(midnight(notified_on));
                    if let Some((kind, observed_at)) = candidate {
                        state.candidates.push(Candidate {
                            domain,
                            source: SourceName::Decp,
                            kind,
                            title: parsed.object.clone(),
                            text: parsed.text.clone(),
                            provenance: provenance.clone(),
                            observed_at,
                            details: json!({
                                "contract_id": parsed.contract_id,
                                "buyer_siret": parsed.buyer.as_str(),
                                "buyer_name": buyer_name,
                                "amount": parsed.amount,
                                "notified_on": notified_on.format("%Y-%m-%d").to_string(),
                                "cpv": parsed.cpv,
                                "holders": holders
                                    .iter()
                                    .map(|holder| json!({
                                        "siret": holder.siret.as_str(),
                                        "name": holder.name,
                                    }))
                                    .collect::<Vec<_>>(),
                            }),
                        });
                        state.stats.candidates += 1;
                    }
                }
                None => {
                    state.stats.unresolved_buyers += 1;
                    debug!(buyer = %parsed.buyer, "decp contract has no buyer domain");
                }
            }
            state.awards.push(AwardedContract {
                key,
                contract_id: parsed.contract_id,
                buyer: parsed.buyer,
                object: parsed.object,
                amount: parsed.amount,
                notified_on,
                provenance,
                content,
                holders,
            });
        }
        Ok(())
    }

    pub async fn collect_with_stats(
        &self,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<(Vec<Candidate>, DecpStats), SourceError> {
        let rules = rules_of(profiles);
        if rules.is_empty() {
            return Ok((Vec::new(), DecpStats::default()));
        }
        let window = Window {
            since: (now - ChronoDuration::days(i64::from(self.settings.notified_within_days)))
                .date_naive(),
            today: now.date_naive(),
        };
        let files_since =
            (now - ChronoDuration::days(i64::from(self.settings.lookback_days))).date_naive();
        let dataset_url = self.settings.dataset_api_url.resolve().await?;
        let dataset: Dataset = self
            .fetch_json(dataset_url)
            .await
            .map_err(|error| upstream(format!("decp dataset listing: {error}")))?;
        let mut state = RunState::default();
        state.lookups.entreprises_url = Some(self.settings.entreprises_url.resolve().await?);
        for (year, month) in months_newest_first(files_since, window.today) {
            let title = resource_title(year, month);
            let Some(resource) = dataset
                .resources
                .iter()
                .find(|resource| resource.title.as_deref() == Some(title.as_str()))
            else {
                warn!(%title, "decp monthly file is not published");
                continue;
            };
            let Some(url) = resource.url.as_deref().and_then(|raw| Url::parse(raw).ok()) else {
                return Err(upstream(format!("decp resource {title} has no usable url")));
            };
            let body = self.download(resource, url).await?;
            state.stats.files += 1;
            state.stats.bytes += u64::try_from(body.len()).unwrap_or(u64::MAX);
            state.stats.files_used.push(title);
            self.process_file(resource, &body, &rules, &window, &mut state)
                .await?;
        }
        if state.stats.files == 0 {
            return Err(upstream("no decp monthly file is published for the window"));
        }
        state.stats.lookups = state.lookups.requests;
        info!(
            files = state.stats.files,
            bytes = state.stats.bytes,
            contracts = state.stats.contracts,
            in_window = state.stats.in_window,
            matched = state.stats.matched,
            candidates = state.stats.candidates,
            unresolved_buyers = state.stats.unresolved_buyers,
            "decp fetch finished"
        );
        *self.awards.lock().unwrap_or_else(PoisonError::into_inner) = state.awards;
        Ok((state.candidates, state.stats))
    }
}

impl<F: Fetcher> CandidateSource for DecpSource<F> {
    fn name(&self) -> SourceName {
        SourceName::Decp
    }

    async fn collect(
        &self,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        self.collect_with_stats(profiles, now)
            .await
            .map(|(candidates, _)| candidates)
    }
}

#[cfg(test)]
mod tests;
