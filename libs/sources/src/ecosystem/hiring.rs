use std::cmp::Reverse;

use chrono::{DateTime, Utc};
use lodestone_core::{EcosystemFinding, EcosystemModule, Fetcher, InspectionTarget, SourceError};
use serde_json::{Value, json};

use crate::ats::{AtsScanner, DomainScan};

pub const MAX_JOB_OFFERS: usize = 50;
pub const MAX_CAREER_PAGES: usize = 3;
pub const MAX_HISTORY: usize = 12;
pub const MAX_LOCATIONS: usize = 5;
pub const SUMMARY_KIND: &str = "hiring_summary";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum JobFunction {
    Engineering,
    Sales,
    Marketing,
    Ops,
    Data,
    Product,
    Support,
    Other,
}

impl JobFunction {
    pub const ALL: [Self; 8] = [
        Self::Engineering,
        Self::Sales,
        Self::Marketing,
        Self::Ops,
        Self::Data,
        Self::Product,
        Self::Support,
        Self::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Engineering => "engineering",
            Self::Sales => "sales",
            Self::Marketing => "marketing",
            Self::Ops => "ops",
            Self::Data => "data",
            Self::Product => "product",
            Self::Support => "support",
            Self::Other => "other",
        }
    }
}

pub const FUNCTION_KEYWORDS: [(JobFunction, &[&str]); 7] = [
    (
        JobFunction::Data,
        &[
            "data",
            "donnees",
            "analytics",
            "analyst",
            "analyste",
            "machine learning",
            "ml",
            "bi",
            "business intelligence",
            "scientist",
            "statisticien",
            "intelligence artificielle",
        ],
    ),
    (
        JobFunction::Sales,
        &[
            "sales",
            "vente",
            "ventes",
            "vendeur",
            "vendeuse",
            "commercial",
            "commerciale",
            "account executive",
            "account manager",
            "business developer",
            "business development",
            "bizdev",
            "sdr",
            "bdr",
            "key account",
            "avant vente",
            "presales",
            "pre sales",
            "charge d affaires",
        ],
    ),
    (
        JobFunction::Marketing,
        &[
            "marketing",
            "growth",
            "seo",
            "sea",
            "communication",
            "content",
            "contenu",
            "brand",
            "marque",
            "community manager",
            "social media",
            "relations presse",
            "evenementiel",
            "demand generation",
        ],
    ),
    (
        JobFunction::Product,
        &[
            "product manager",
            "product owner",
            "product designer",
            "head of product",
            "chef de produit",
            "produit",
            "ux",
            "ui",
            "designer",
        ],
    ),
    (
        JobFunction::Engineering,
        &[
            "engineer",
            "engineering",
            "ingenieur",
            "developer",
            "developpeur",
            "developpeuse",
            "dev",
            "developpement",
            "software",
            "logiciel",
            "backend",
            "back end",
            "frontend",
            "front end",
            "fullstack",
            "full stack",
            "devops",
            "sre",
            "architect",
            "architecte",
            "qa",
            "testeur",
            "cto",
            "tech lead",
            "programmeur",
            "informatique",
            "systemes",
            "reseaux",
            "cybersecurite",
            "security",
            "securite",
            "cloud",
            "embarque",
            "embedded",
            "firmware",
            "mobile",
            "ios",
            "android",
            "it",
        ],
    ),
    (
        JobFunction::Support,
        &[
            "support",
            "customer success",
            "service client",
            "relation client",
            "helpdesk",
            "help desk",
            "customer care",
            "csm",
            "onboarding",
            "technicien",
            "technicienne",
        ],
    ),
    (
        JobFunction::Ops,
        &[
            "operations",
            "ops",
            "office manager",
            "rh",
            "ressources humaines",
            "hr",
            "people",
            "recrutement",
            "recruiter",
            "talent",
            "finance",
            "financier",
            "comptable",
            "comptabilite",
            "accountant",
            "controleur de gestion",
            "controller",
            "juridique",
            "legal",
            "juriste",
            "administratif",
            "administrative",
            "assistant",
            "assistante",
            "achats",
            "acheteur",
            "procurement",
            "logistique",
            "logistics",
            "supply",
            "paie",
            "payroll",
        ],
    ),
];

pub const CONTRACT_KEYWORDS: [(&str, &[&str]); 7] = [
    ("CDI", &["cdi", "permanent"]),
    ("CDD", &["cdd", "fixed term", "temporary", "temporaire"]),
    ("Stage", &["stage", "stagiaire", "intern", "internship"]),
    (
        "Alternance",
        &[
            "alternance",
            "alternant",
            "alternante",
            "apprenti",
            "apprentie",
            "apprentissage",
            "apprenticeship",
            "work study",
        ],
    ),
    ("Freelance", &["freelance", "independant", "contractor"]),
    ("Temps plein", &["full time", "fulltime", "temps plein"]),
    ("Temps partiel", &["part time", "parttime", "temps partiel"]),
];

pub const REMOTE_KEYWORDS: [&str; 7] = [
    "remote",
    "teletravail",
    "full remote",
    "a distance",
    "work from home",
    "home office",
    "distanciel",
];

fn folded(text: &str) -> String {
    let lowered: String = text
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            other if other.is_alphanumeric() => other,
            _ => ' ',
        })
        .collect();
    format!(
        " {} ",
        lowered.split_whitespace().collect::<Vec<_>>().join(" ")
    )
}

fn mentions(haystack: &str, keyword: &str) -> bool {
    haystack.contains(&format!(" {keyword} "))
}

fn function_in(text: &str) -> Option<JobFunction> {
    let haystack = folded(text);
    FUNCTION_KEYWORDS
        .iter()
        .find(|(_, keywords)| keywords.iter().any(|keyword| mentions(&haystack, keyword)))
        .map(|(function, _)| *function)
}

pub const TITLE_SEPARATORS: [&str; 5] = [" - ", " – ", " — ", " | ", "("];

fn main_part(title: &str) -> &str {
    TITLE_SEPARATORS
        .iter()
        .filter_map(|separator| title.find(separator))
        .min()
        .map_or(title, |end| &title[..end])
}

pub fn classify(title: &str, team: Option<&str>) -> JobFunction {
    function_in(main_part(title))
        .or_else(|| function_in(title))
        .or_else(|| team.and_then(function_in))
        .unwrap_or(JobFunction::Other)
}

pub fn contract_types(texts: &[&str]) -> Vec<&'static str> {
    let haystack = folded(&texts.join(" "));
    CONTRACT_KEYWORDS
        .iter()
        .filter(|(_, keywords)| keywords.iter().any(|keyword| mentions(&haystack, keyword)))
        .map(|(label, _)| *label)
        .collect()
}

pub fn remote_mentioned(texts: &[&str]) -> bool {
    let haystack = folded(&texts.join(" "));
    REMOTE_KEYWORDS
        .iter()
        .any(|keyword| mentions(&haystack, keyword))
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

#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub title: String,
    pub url: String,
    pub location: Option<String>,
    pub team: Option<String>,
    pub ats: Option<String>,
    pub function: JobFunction,
    pub contracts: Vec<&'static str>,
    pub remote: bool,
    pub posted_at: Option<DateTime<Utc>>,
    pub keywords: Vec<String>,
}

fn detail_text<'a>(details: &'a Value, key: &str) -> Option<&'a str> {
    details
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn posted(details: &Value) -> Option<DateTime<Utc>> {
    detail_text(details, "posted_at")
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|time| time.with_timezone(&Utc))
}

pub fn offers_of(scan: &DomainScan, highlight: &[String]) -> Vec<Offer> {
    let mut offers: Vec<Offer> = Vec::new();
    for board in &scan.boards {
        for candidate in &board.candidates {
            let details = &candidate.details;
            let location = detail_text(details, "location").map(str::to_owned);
            let team = detail_text(details, "team").map(str::to_owned);
            let contract = detail_text(details, "contract").unwrap_or_default();
            let flagged = details.get("remote").and_then(Value::as_bool) == Some(true);
            let place = location.as_deref().unwrap_or_default();
            offers.push(Offer {
                function: classify(&candidate.title, team.as_deref()),
                contracts: contract_types(&[contract, &candidate.title]),
                remote: flagged || remote_mentioned(&[&candidate.title, place]),
                posted_at: posted(details),
                keywords: matched_keywords(
                    &format!("{} {}", candidate.title, candidate.text),
                    highlight,
                )
                .into_iter()
                .map(str::to_owned)
                .collect(),
                title: candidate.title.clone(),
                url: candidate.provenance.as_str().to_owned(),
                location,
                team,
                ats: detail_text(details, "ats").map(str::to_owned),
            });
        }
    }
    for posting in &scan.postings {
        let url = posting.url.as_str();
        if offers.iter().any(|known| known.url == url) {
            continue;
        }
        let place = posting.location.as_deref().unwrap_or_default();
        let contract = posting.contract.as_deref().unwrap_or_default();
        offers.push(Offer {
            function: classify(&posting.title, None),
            contracts: contract_types(&[contract, &posting.title]),
            remote: posting.remote || remote_mentioned(&[&posting.title, place]),
            posted_at: posting.posted_at,
            keywords: matched_keywords(&posting.title, highlight)
                .into_iter()
                .map(str::to_owned)
                .collect(),
            title: posting.title.clone(),
            url: url.to_owned(),
            location: posting.location.clone(),
            team: None,
            ats: None,
        });
    }
    offers
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPoint {
    pub at: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HiringSummary {
    pub count: usize,
    pub functions: Vec<(JobFunction, usize)>,
    pub locations: Vec<(String, usize)>,
    pub remote: usize,
    pub contracts: Vec<(&'static str, usize)>,
    pub previous: Option<usize>,
    pub history: Vec<HistoryPoint>,
}

pub fn previous_history(previous: &[EcosystemFinding]) -> (Option<usize>, Vec<HistoryPoint>) {
    let Some(summary) = previous.iter().find(|finding| finding.kind == SUMMARY_KIND) else {
        return (None, Vec::new());
    };
    let count = summary
        .details
        .get("count")
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok());
    let history = summary
        .details
        .get("history")
        .and_then(Value::as_array)
        .map(|points| {
            points
                .iter()
                .filter_map(|point| {
                    Some(HistoryPoint {
                        at: point.get("at")?.as_str()?.to_owned(),
                        count: usize::try_from(point.get("count")?.as_u64()?).ok()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    (count, history)
}

fn tally<T: PartialEq + Copy>(items: impl Iterator<Item = T>) -> Vec<(T, usize)> {
    let mut counts: Vec<(T, usize)> = Vec::new();
    for item in items {
        match counts.iter_mut().find(|(known, _)| *known == item) {
            Some((_, count)) => *count += 1,
            None => counts.push((item, 1)),
        }
    }
    counts
}

pub fn summarize(
    offers: &[Offer],
    previous: &[EcosystemFinding],
    now: DateTime<Utc>,
) -> HiringSummary {
    let mut functions = tally(offers.iter().map(|offer| offer.function));
    functions.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    let mut locations: Vec<(String, usize)> =
        tally(offers.iter().filter_map(|offer| offer.location.as_deref()))
            .into_iter()
            .map(|(place, count)| (place.to_owned(), count))
            .collect();
    locations.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    locations.truncate(MAX_LOCATIONS);
    let mut contracts = tally(
        offers
            .iter()
            .flat_map(|offer| offer.contracts.iter().copied()),
    );
    contracts.sort_by_key(|(_, count)| Reverse(*count));
    let (previous_count, mut history) = previous_history(previous);
    let today = now.date_naive().to_string();
    history.retain(|point| point.at != today);
    history.push(HistoryPoint {
        at: today,
        count: offers.len(),
    });
    let excess = history.len().saturating_sub(MAX_HISTORY);
    history.drain(..excess);
    HiringSummary {
        count: offers.len(),
        functions,
        locations,
        remote: offers.iter().filter(|offer| offer.remote).count(),
        contracts,
        previous: previous_count,
        history,
    }
}

impl HiringSummary {
    pub fn delta(&self) -> Option<i64> {
        let current = i64::try_from(self.count).ok()?;
        let before = i64::try_from(self.previous?).ok()?;
        Some(current - before)
    }

    pub fn sentence(&self) -> String {
        let mut sentence = match self.count {
            0 => "Aucune offre ouverte".to_owned(),
            1 => "1 offre ouverte".to_owned(),
            count => format!("{count} offres ouvertes"),
        };
        if !self.functions.is_empty() {
            let parts: Vec<String> = self
                .functions
                .iter()
                .map(|(function, count)| format!("{count} {}", function.as_str()))
                .collect();
            sentence.push_str(" : ");
            sentence.push_str(&parts.join(", "));
        }
        match self.delta() {
            Some(0) => sentence.push_str(" (stable depuis le dernier passage)"),
            Some(delta) if delta > 0 => {
                sentence.push_str(&format!(" (+{delta} depuis le dernier passage)"));
            }
            Some(delta) => sentence.push_str(&format!(" ({delta} depuis le dernier passage)")),
            None => {}
        }
        sentence
    }

    pub fn details(&self) -> Value {
        json!({
            "count": self.count,
            "previous_count": self.previous,
            "delta": self.delta(),
            "functions": self
                .functions
                .iter()
                .map(|(function, count)| json!({"function": function.as_str(), "count": count}))
                .collect::<Vec<_>>(),
            "locations": self
                .locations
                .iter()
                .map(|(name, count)| json!({"name": name, "count": count}))
                .collect::<Vec<_>>(),
            "remote": self.remote,
            "contracts": self
                .contracts
                .iter()
                .map(|(label, count)| json!({"label": label, "count": count}))
                .collect::<Vec<_>>(),
            "history": self
                .history
                .iter()
                .map(|point| json!({"at": point.at, "count": point.count}))
                .collect::<Vec<_>>(),
        })
    }
}

fn offer_finding(offer: &Offer, now: DateTime<Utc>) -> EcosystemFinding {
    EcosystemFinding::new(
        EcosystemModule::HiringActivity,
        "job_offer",
        "Offre d'emploi",
        offer.title.as_str(),
        offer.url.as_str(),
        offer.posted_at.unwrap_or(now),
    )
    .keyed(format!("job_offer:{}", offer.url))
    .with_details(json!({
        "highlighted": !offer.keywords.is_empty(),
        "keywords": offer.keywords,
        "location": offer.location,
        "team": offer.team,
        "ats": offer.ats,
        "function": offer.function.as_str(),
        "contracts": offer.contracts,
        "remote": offer.remote,
        "posted_at": offer.posted_at.map(|time| time.date_naive().to_string()),
    }))
}

pub fn hiring_findings(
    scan: &DomainScan,
    offers: &[Offer],
    previous: &[EcosystemFinding],
    now: DateTime<Utc>,
) -> Vec<EcosystemFinding> {
    let module = EcosystemModule::HiringActivity;
    let careers: Vec<&str> = scan
        .pages
        .iter()
        .filter(|page| page.path() != "/" && !page.path().is_empty())
        .take(MAX_CAREER_PAGES)
        .map(|page| page.as_str())
        .collect();
    let mut findings = Vec::new();
    if !scan.boards.is_empty() || !offers.is_empty() {
        let summary = summarize(offers, previous, now);
        let summary_source = scan
            .boards
            .first()
            .and_then(|board| board.board.api_url())
            .map(|url| url.to_string())
            .or_else(|| careers.first().map(|page| (*page).to_owned()))
            .or_else(|| offers.first().map(|offer| offer.url.clone()))
            .unwrap_or_default();
        findings.push(
            EcosystemFinding::new(
                module,
                SUMMARY_KIND,
                "Recrutement",
                summary.sentence(),
                summary_source,
                now,
            )
            .keyed(SUMMARY_KIND)
            .with_details(summary.details()),
        );
    }
    findings.extend(careers.iter().map(|page| {
        EcosystemFinding::new(module, "careers_page", "Page carrières", *page, *page, now)
    }));
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
            .keyed(format!("ats_board:{name}"))
            .with_details(json!({"open_positions": board.candidates.len()})),
        );
    }
    let mut ranked: Vec<&Offer> = offers.iter().collect();
    ranked.sort_by(|left, right| {
        left.keywords
            .is_empty()
            .cmp(&right.keywords.is_empty())
            .then(right.posted_at.cmp(&left.posted_at))
    });
    findings.extend(
        ranked
            .into_iter()
            .take(MAX_JOB_OFFERS)
            .map(|offer| offer_finding(offer, now)),
    );
    findings
}

pub struct HiringModule<F: Fetcher> {
    scanner: AtsScanner<F>,
}

impl<F: Fetcher> HiringModule<F> {
    pub fn new(scanner: AtsScanner<F>) -> Self {
        Self { scanner }
    }

    pub async fn inspect(
        &self,
        target: &InspectionTarget<'_>,
    ) -> Result<Vec<EcosystemFinding>, SourceError> {
        let scan = self.scanner.scan(target.domain, target.now).await?;
        let offers = offers_of(&scan, &target.settings.highlight_keywords);
        Ok(hiring_findings(&scan, &offers, target.previous, target.now))
    }
}

#[cfg(test)]
mod tests;
