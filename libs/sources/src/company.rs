use std::{sync::Arc, sync::LazyLock, time::Duration};

use lodestone_core::{
    CompanyBuilder, CompanyPage, DossierBuild, FetchError, FetchPurpose, FetchRequest,
    FetchResponse, Fetcher, LinkRejection, NormalizedDomain, OfficialData, PAGE_STORE_CHARS,
    PageReview, PageVerifier, Siren, SourceError, SourceName, UnconfirmedLinks, bounded_error,
    headcount_label, naf_section_label,
};
use regex::Regex;
use serde::Deserialize;
use tracing::debug;
use url::Url;

use crate::{
    ats::jobs::html_to_text,
    endpoint::Endpoint,
    link_check::{LinkChecker, LinkOrigin, LinkVerdict, fetch_waiting, rejection_of},
};

const SIREN_LABEL_WINDOW: usize = 40;

#[derive(Debug, Clone)]
pub struct CompanySettings {
    pub entreprises_url: Endpoint,
    pub max_pages: usize,
    pub max_rate_limit_wait: Duration,
    pub origin: Option<Url>,
}

pub struct CompanyResearch<F: Fetcher> {
    fetcher: Arc<F>,
    settings: CompanySettings,
}

impl<F: Fetcher> Clone for CompanyResearch<F> {
    fn clone(&self) -> Self {
        Self {
            fetcher: Arc::clone(&self.fetcher),
            settings: self.settings.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PageKind {
    Legal,
    Terms,
    About,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageLink {
    pub url: Url,
    pub kind: PageKind,
}

const LEGAL_WORDS: &[&str] = &[
    "mentions-legales",
    "mentions_legales",
    "mentionslegales",
    "mentions-legale",
    "informations-legales",
    "legal-notice",
    "legal",
    "mentions",
    "imprint",
];
const TERMS_WORDS: &[&str] = &[
    "cgu",
    "cgv",
    "conditions-generales",
    "conditions-d-utilisation",
];
const ABOUT_WORDS: &[&str] = &[
    "a-propos",
    "apropos",
    "qui-sommes-nous",
    "quisommesnous",
    "about",
    "societe",
    "entreprise",
    "notre-entreprise",
    "notre-histoire",
    "presentation",
];
const FALLBACK_LEGAL: &[&str] = &["/mentions-legales", "/legal"];
const FALLBACK_ABOUT: &[&str] = &["/a-propos", "/about"];

fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => unreachable!("static pattern {pattern} is invalid: {error}"),
    }
}

static ANCHOR: LazyLock<Regex> =
    LazyLock::new(|| compile(r#"(?is)<a\s[^>]*?href\s*=\s*(?:"([^"]*)"|'([^']*)')"#));
static SIREN_LABELLED: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(
        r"(?i)(?:siret|siren|r\.?\s?c\.?\s?s\.?|immatricul\w*)[^0-9]{{0,{SIREN_LABEL_WINDOW}}}?(\d{{3}}[ .\u{{a0}}\u{{202f}}]?\d{{3}}[ .\u{{a0}}\u{{202f}}]?\d{{3}})(?:[ .\u{{a0}}\u{{202f}}]?\d{{5}})?"
    ))
});

fn classify(path: &str) -> Option<PageKind> {
    let lowered = path.to_ascii_lowercase();
    let segments: Vec<&str> = lowered
        .split(['/', '.'])
        .filter(|segment| !segment.is_empty())
        .collect();
    let has = |words: &[&str]| {
        segments
            .iter()
            .any(|segment| words.iter().any(|word| segment.contains(word)))
    };
    if has(LEGAL_WORDS) {
        Some(PageKind::Legal)
    } else if has(TERMS_WORDS) {
        Some(PageKind::Terms)
    } else if has(ABOUT_WORDS) {
        Some(PageKind::About)
    } else {
        None
    }
}

fn same_site(url: &Url, home: &Url, domain: &NormalizedDomain) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if home.host_str() == Some(host) && home.port_or_known_default() == url.port_or_known_default()
    {
        return true;
    }
    NormalizedDomain::from_host(host).is_ok_and(|candidate| &candidate == domain)
}

pub fn page_links(html: &str, home: &Url, domain: &NormalizedDomain) -> Vec<PageLink> {
    let mut links: Vec<PageLink> = Vec::new();
    for capture in ANCHOR.captures_iter(html) {
        let Some(raw) = capture.get(1).or_else(|| capture.get(2)) else {
            continue;
        };
        let Ok(mut url) = home.join(raw.as_str().trim()) else {
            continue;
        };
        if !matches!(url.scheme(), "http" | "https") || !same_site(&url, home, domain) {
            continue;
        }
        url.set_fragment(None);
        url.set_query(None);
        if url.path() == "/" || url.path() == home.path() {
            continue;
        }
        let Some(kind) = classify(url.path()) else {
            continue;
        };
        if links.iter().any(|link| link.url == url) {
            continue;
        }
        links.push(PageLink { url, kind });
    }
    links.sort_by_key(|link| link.kind);
    links
}

fn fallback(home: &Url, paths: &[&str]) -> Vec<Url> {
    paths
        .iter()
        .filter_map(|path| home.join(path).ok())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub url: Url,
    pub kind: PageKind,
    pub origin: LinkOrigin,
}

pub fn candidates(links: &[PageLink], home: &Url, limit: usize) -> Vec<Candidate> {
    let mut chosen: Vec<Candidate> = Vec::new();
    let mut push = |url: Url, kind: PageKind, origin: LinkOrigin| {
        if !chosen.iter().any(|known| known.url == url) {
            chosen.push(Candidate { url, kind, origin });
        }
    };
    let of_kind = |kind: PageKind| links.iter().filter(move |link| link.kind == kind);
    let mut legal = of_kind(PageKind::Legal).peekable();
    if legal.peek().is_none() {
        for url in fallback(home, FALLBACK_LEGAL) {
            push(url, PageKind::Legal, LinkOrigin::Guessed);
        }
    }
    for link in legal {
        push(link.url.clone(), PageKind::Legal, LinkOrigin::Linked);
    }
    for link in of_kind(PageKind::Terms) {
        push(link.url.clone(), PageKind::Terms, LinkOrigin::Linked);
    }
    let mut about = of_kind(PageKind::About).peekable();
    if about.peek().is_none() {
        for url in fallback(home, FALLBACK_ABOUT) {
            push(url, PageKind::About, LinkOrigin::Guessed);
        }
    }
    for link in about {
        push(link.url.clone(), PageKind::About, LinkOrigin::Linked);
    }
    chosen.truncate(limit);
    chosen
}

fn stored_origin(url: &Url) -> LinkOrigin {
    let path = url.path();
    if path == "/" || path.is_empty() {
        LinkOrigin::Home
    } else if FALLBACK_LEGAL.contains(&path) || FALLBACK_ABOUT.contains(&path) {
        LinkOrigin::Guessed
    } else {
        LinkOrigin::Linked
    }
}

pub fn find_siren(text: &str) -> Option<Siren> {
    SIREN_LABELLED.captures_iter(text).find_map(|capture| {
        let whole = capture.get(0)?;
        let digits = capture.get(1)?;
        let next_is_digit = text[whole.end()..]
            .chars()
            .next()
            .is_some_and(|next| next.is_ascii_digit());
        if next_is_digit {
            return None;
        }
        let compact: String = digits
            .as_str()
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        Siren::parse(&compact).ok()
    })
}

#[derive(Debug, Deserialize)]
struct Answer {
    #[serde(default)]
    results: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    siren: Option<String>,
    nom_complet: Option<String>,
    nom_raison_sociale: Option<String>,
    activite_principale: Option<String>,
    section_activite_principale: Option<String>,
    tranche_effectif_salarie: Option<String>,
    date_creation: Option<String>,
    siege: Option<Siege>,
}

#[derive(Debug, Deserialize)]
struct Siege {
    code_postal: Option<String>,
    libelle_commune: Option<String>,
    libelle_pays_etranger: Option<String>,
    tranche_effectif_salarie: Option<String>,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

pub fn parse_official(body: &[u8], siren: &Siren) -> Result<Option<OfficialData>, String> {
    let answer: Answer = serde_json::from_slice(body)
        .map_err(|error| format!("response is not the expected json: {error}"))?;
    let Some(entry) = answer
        .results
        .into_iter()
        .find(|entry| entry.siren.as_deref() == Some(siren.as_str()))
    else {
        return Ok(None);
    };
    let Some(legal_name) =
        non_empty(entry.nom_complet).or_else(|| non_empty(entry.nom_raison_sociale))
    else {
        return Ok(None);
    };
    let siege = entry.siege;
    let headcount_code = non_empty(entry.tranche_effectif_salarie).or_else(|| {
        siege
            .as_ref()
            .and_then(|siege| non_empty(siege.tranche_effectif_salarie.clone()))
    });
    let foreign = siege
        .as_ref()
        .and_then(|siege| non_empty(siege.libelle_pays_etranger.clone()));
    let (country, country_code) = match foreign {
        Some(name) => (Some(name), None),
        None => (Some("France".to_owned()), Some("FR".to_owned())),
    };
    Ok(Some(OfficialData {
        siren: siren.clone(),
        legal_name,
        naf_code: non_empty(entry.activite_principale),
        naf_label: non_empty(entry.section_activite_principale)
            .and_then(|section| naf_section_label(&section).map(str::to_owned)),
        headcount_range: headcount_code
            .as_deref()
            .and_then(headcount_label)
            .map(str::to_owned),
        headcount_code,
        head_office_city: siege
            .as_ref()
            .and_then(|siege| non_empty(siege.libelle_commune.clone())),
        head_office_postcode: siege
            .as_ref()
            .and_then(|siege| non_empty(siege.code_postal.clone())),
        head_office_country: country,
        head_office_country_code: country_code,
        created_on: non_empty(entry.date_creation)
            .and_then(|raw| chrono::NaiveDate::parse_from_str(&raw, "%Y-%m-%d").ok()),
    }))
}

struct Fetched {
    page: CompanyPage,
    html: String,
    full_text: String,
}

enum Read {
    Page(Box<Fetched>),
    Unconfirmed(LinkRejection),
}

impl<F: Fetcher> CompanyResearch<F> {
    pub fn new(fetcher: Arc<F>, settings: CompanySettings) -> Self {
        Self { fetcher, settings }
    }

    fn home_url(&self, domain: &NormalizedDomain) -> Result<Url, SourceError> {
        let url = match &self.settings.origin {
            Some(origin) => origin.join("/"),
            None => Url::parse(&format!("https://{}/", domain.as_str())),
        };
        url.map_err(|_| SourceError::Fetch(FetchError::InvalidUrl))
    }

    async fn fetch(&self, url: Url, purpose: FetchPurpose) -> Result<FetchResponse, FetchError> {
        let request = FetchRequest {
            url,
            purpose,
            source: SourceName::PublicPage,
        };
        fetch_waiting(
            self.fetcher.as_ref(),
            &request,
            self.settings.max_rate_limit_wait,
        )
        .await
    }

    fn checker(&self) -> LinkChecker<'_, F> {
        LinkChecker::new(
            self.fetcher.as_ref(),
            SourceName::PublicPage,
            self.settings.max_rate_limit_wait,
        )
    }

    async fn read(
        &self,
        checker: &mut LinkChecker<'_, F>,
        url: Url,
        origin: LinkOrigin,
    ) -> Result<Read, FetchError> {
        let response = match self.fetch(url.clone(), FetchPurpose::Crawl).await {
            Ok(response) => response,
            Err(error) => {
                if origin != LinkOrigin::Home
                    && let Some(reason) = rejection_of(&error)
                {
                    checker.reject(reason);
                    debug!(%url, %error, reason = reason.as_str(), "company page not confirmed");
                    return Ok(Read::Unconfirmed(reason));
                }
                return Err(error);
            }
        };
        if let LinkVerdict::NotFound(reason) = checker.confirm(&url, origin, &response).await? {
            debug!(%url, final_url = %response.final_url, status = response.status, reason = reason.as_str(), "company page not confirmed");
            return Ok(Read::Unconfirmed(reason));
        }
        let html = response.text().into_owned();
        let full_text = html_to_text(&html);
        let text: String = full_text.chars().take(PAGE_STORE_CHARS).collect();
        Ok(Read::Page(Box::new(Fetched {
            page: CompanyPage {
                url: response.final_url.to_string(),
                text,
            },
            html,
            full_text,
        })))
    }

    async fn lookup(&self, siren: &Siren) -> Result<Option<OfficialData>, SourceError> {
        let mut url = self.settings.entreprises_url.resolve().await?;
        url.query_pairs_mut()
            .append_pair("q", siren.as_str())
            .append_pair("per_page", "1");
        let response = self.fetch(url, FetchPurpose::Api).await?;
        if response.status != 200 {
            return Err(SourceError::Upstream(format!(
                "company register answered {}",
                response.status
            )));
        }
        if response.truncated {
            return Err(SourceError::Upstream(
                "company register response exceeded the body cap".to_owned(),
            ));
        }
        parse_official(&response.body, siren).map_err(SourceError::Upstream)
    }
}

fn unreachable_build(reason: &str, unconfirmed: UnconfirmedLinks) -> DossierBuild {
    DossierBuild {
        siren: None,
        official: None,
        pages: Vec::new(),
        reachable: false,
        failure: Some(bounded_error(reason)),
        unconfirmed,
    }
}

impl<F: Fetcher> CompanyBuilder for CompanyResearch<F> {
    async fn build(&self, domain: &NormalizedDomain) -> Result<DossierBuild, SourceError> {
        let home_url = self.home_url(domain)?;
        let mut checker = self.checker();
        let home = match self
            .read(&mut checker, home_url.clone(), LinkOrigin::Home)
            .await
        {
            Ok(Read::Page(fetched)) => *fetched,
            Ok(Read::Unconfirmed(reason)) => {
                return Ok(unreachable_build(
                    &format!("home page not confirmed: {reason}"),
                    checker.unconfirmed(),
                ));
            }
            Err(FetchError::RateLimited { retry_after }) => {
                return Err(SourceError::RateLimited { retry_after });
            }
            Err(FetchError::DisallowedByRobots) => {
                return Ok(unreachable_build(
                    "robots.txt disallows the home page",
                    checker.unconfirmed(),
                ));
            }
            Err(error) => {
                return Ok(unreachable_build(&error.to_string(), checker.unconfirmed()));
            }
        };
        let base = Url::parse(&home.page.url).unwrap_or(home_url);
        let links = page_links(&home.html, &base, domain);
        let wanted = candidates(&links, &base, self.settings.max_pages.saturating_sub(1));
        let mut others: Vec<(Fetched, PageKind)> = Vec::new();
        for candidate in wanted {
            match self
                .read(&mut checker, candidate.url.clone(), candidate.origin)
                .await
            {
                Ok(Read::Page(fetched)) => {
                    let known = fetched.page.url == home.page.url
                        || others
                            .iter()
                            .any(|(other, _)| other.page.url == fetched.page.url);
                    if !known {
                        others.push((*fetched, candidate.kind));
                    }
                }
                Ok(Read::Unconfirmed(_)) => {}
                Err(FetchError::RateLimited { retry_after }) => {
                    return Err(SourceError::RateLimited { retry_after });
                }
                Err(error) => debug!(url = %candidate.url, %error, "company page skipped"),
            }
        }
        let legal_first = others
            .iter()
            .filter(|(_, kind)| *kind != PageKind::About)
            .chain(others.iter().filter(|(_, kind)| *kind == PageKind::About));
        let siren = legal_first
            .into_iter()
            .find_map(|(fetched, _)| find_siren(&fetched.full_text))
            .or_else(|| find_siren(&home.full_text));
        let mut failure = None;
        let official = match &siren {
            Some(siren) => match self.lookup(siren).await {
                Ok(official) => official,
                Err(SourceError::RateLimited { retry_after }) => {
                    return Err(SourceError::RateLimited { retry_after });
                }
                Err(error) => {
                    failure = Some(bounded_error(&error.to_string()));
                    None
                }
            },
            None => None,
        };
        let mut all = vec![home.page];
        all.extend(others.into_iter().map(|(fetched, _)| fetched.page));
        Ok(DossierBuild {
            siren,
            official,
            pages: all,
            reachable: true,
            failure,
            unconfirmed: checker.unconfirmed(),
        })
    }
}

impl<F: Fetcher> PageVerifier for CompanyResearch<F> {
    async fn review(&self, pages: &[CompanyPage]) -> Result<PageReview, SourceError> {
        let mut checker = self.checker();
        let mut review = PageReview::default();
        for page in pages {
            let Ok(url) = Url::parse(&page.url) else {
                continue;
            };
            let origin = stored_origin(&url);
            match self.read(&mut checker, url, origin).await {
                Ok(Read::Unconfirmed(reason)) => review.rejected.push((page.url.clone(), reason)),
                Ok(Read::Page(_)) => {}
                Err(FetchError::RateLimited { retry_after }) => {
                    return Err(SourceError::RateLimited { retry_after });
                }
                Err(error) => {
                    debug!(url = %page.url, %error, "stored company page kept, it could not be checked")
                }
            }
        }
        Ok(review)
    }
}

#[cfg(test)]
mod tests;
