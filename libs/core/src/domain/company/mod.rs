use aws_lc_rs::digest::{SHA256, digest};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::{
    NormalizedDomain,
    fetch::link::{LinkRejection, UnconfirmedLinks},
    qualification::{SUMMARY_FAILED_AFTER, summary_retry_delay},
    source::SourceError,
};

mod entity;
mod labels;
pub mod ports;
mod siren;

pub use entity::{CompanyEntity, SiblingLead};
pub use labels::{headcount_label, naf_section_label};
pub use siren::{InvalidSiren, Siren};

pub const PAGE_STORE_CHARS: usize = 8000;
pub const PAGE_PROMPT_CHARS: usize = 3000;
pub const OFFICIAL_SOURCE: &str = "https://recherche-entreprises.api.gouv.fr/search";
const ERROR_MAX_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DossierState {
    Pending,
    Complete,
    Partial,
    Unreachable,
    HostingPlatform,
}

impl DossierState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Unreachable => "unreachable",
            Self::HostingPlatform => "hosting_platform",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [
            Self::Pending,
            Self::Complete,
            Self::Partial,
            Self::Unreachable,
            Self::HostingPlatform,
        ]
        .into_iter()
        .find(|state| state.as_str() == raw)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanyPage {
    pub url: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficialData {
    pub siren: Siren,
    pub legal_name: String,
    pub naf_code: Option<String>,
    pub naf_label: Option<String>,
    pub headcount_code: Option<String>,
    pub headcount_range: Option<String>,
    pub head_office_city: Option<String>,
    pub head_office_postcode: Option<String>,
    pub head_office_country: Option<String>,
    pub head_office_country_code: Option<String>,
    pub created_on: Option<NaiveDate>,
}

impl OfficialData {
    pub fn source_url(&self) -> String {
        format!("{OFFICIAL_SOURCE}?q={}", self.siren)
    }

    pub fn public_page(&self) -> String {
        format!(
            "https://annuaire-entreprises.data.gouv.fr/entreprise/{}",
            self.siren
        )
    }

    pub fn render(&self) -> String {
        let mut lines = vec![format!(
            "Company dossier, official data from the French company register (SIREN {}):",
            self.siren
        )];
        lines.push(format!("- legal name: {}", self.legal_name));
        match (&self.naf_code, &self.naf_label) {
            (Some(code), Some(label)) => {
                lines.push(format!("- main activity (NAF): {code}, {label}"))
            }
            (Some(code), None) => lines.push(format!("- main activity (NAF): {code}")),
            _ => {}
        }
        if let Some(range) = &self.headcount_range {
            lines.push(format!("- headcount range: {range}"));
        }
        let place: Vec<&str> = [
            self.head_office_postcode.as_deref(),
            self.head_office_city.as_deref(),
            self.head_office_country.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !place.is_empty() {
            lines.push(format!("- head office: {}", place.join(" ")));
        }
        if let Some(created) = self.created_on {
            lines.push(format!("- created on: {created}"));
        }
        lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyDossier {
    pub domain: NormalizedDomain,
    pub state: DossierState,
    pub siren: Option<Siren>,
    pub official: Option<OfficialData>,
    pub pages: Vec<CompanyPage>,
    pub content_hash: Option<String>,
    pub content_at: Option<DateTime<Utc>>,
    pub built_at: Option<DateTime<Utc>>,
    pub attempts: u32,
    pub next_attempt_at: DateTime<Utc>,
    pub error: Option<String>,
    pub ignored_links: u32,
    pub pages_checked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DossierDocument {
    pub source: String,
    pub content: String,
}

fn truncated(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

impl CompanyDossier {
    pub fn has_content(&self) -> bool {
        self.official.is_some() || !self.pages.is_empty()
    }

    pub fn documents(&self, page_chars: usize) -> Vec<DossierDocument> {
        let official = self.official.as_ref().map(|official| DossierDocument {
            source: official.source_url(),
            content: official.render(),
        });
        let pages = self.pages.iter().map(|page| DossierDocument {
            source: page.url.clone(),
            content: format!(
                "Company dossier, page of the company site {}:\n{}",
                page.url,
                truncated(&page.text, page_chars)
            ),
        });
        official.into_iter().chain(pages).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DossierBuild {
    pub siren: Option<Siren>,
    pub official: Option<OfficialData>,
    pub pages: Vec<CompanyPage>,
    pub reachable: bool,
    pub failure: Option<String>,
    pub unconfirmed: UnconfirmedLinks,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageReview {
    pub rejected: Vec<(String, LinkRejection)>,
}

impl PageReview {
    pub fn unconfirmed(&self) -> UnconfirmedLinks {
        let mut links = UnconfirmedLinks::default();
        for (_, reason) in &self.rejected {
            links.record(*reason);
        }
        links
    }

    fn rejects(&self, url: &str) -> bool {
        self.rejected.iter().any(|(rejected, _)| rejected == url)
    }
}

fn content_hash_of(official: Option<&OfficialData>, pages: &[CompanyPage]) -> Option<String> {
    if official.is_none() && pages.is_empty() {
        return None;
    }
    let material = json!({
        "official": official,
        "pages": pages,
    });
    let hashed = digest(&SHA256, material.to_string().as_bytes());
    Some(
        hashed
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

fn ignored_count(links: &UnconfirmedLinks) -> u32 {
    u32::try_from(links.total()).unwrap_or(u32::MAX)
}

impl DossierBuild {
    pub fn state(&self) -> DossierState {
        if !self.reachable {
            DossierState::Unreachable
        } else if self.official.is_some() {
            DossierState::Complete
        } else {
            DossierState::Partial
        }
    }

    pub fn content_hash(&self) -> Option<String> {
        content_hash_of(self.official.as_ref(), &self.pages)
    }
}

impl CompanyDossier {
    pub fn pending(domain: NormalizedDomain, next_attempt_at: DateTime<Utc>) -> Self {
        Self {
            domain,
            state: DossierState::Pending,
            siren: None,
            official: None,
            pages: Vec::new(),
            content_hash: None,
            content_at: None,
            built_at: None,
            attempts: 0,
            next_attempt_at,
            error: None,
            ignored_links: 0,
            pages_checked_at: None,
        }
    }

    pub fn on_hosting_platform(
        domain: NormalizedDomain,
        now: DateTime<Utc>,
        refresh: Duration,
    ) -> Self {
        Self {
            state: DossierState::HostingPlatform,
            built_at: Some(now),
            pages_checked_at: Some(now),
            ..Self::pending(domain, now + refresh)
        }
    }

    pub fn reviewed(&self, review: &PageReview, now: DateTime<Utc>) -> Self {
        let pages: Vec<CompanyPage> = self
            .pages
            .iter()
            .filter(|page| !review.rejects(&page.url))
            .cloned()
            .collect();
        let content_hash = content_hash_of(self.official.as_ref(), &pages);
        let content_at = if content_hash == self.content_hash {
            self.content_at
        } else {
            Some(now)
        };
        Self {
            pages,
            content_hash,
            content_at,
            ignored_links: self
                .ignored_links
                .saturating_add(ignored_count(&review.unconfirmed())),
            pages_checked_at: Some(now),
            ..self.clone()
        }
    }

    pub fn deferred(mut self, until: DateTime<Utc>) -> Self {
        self.next_attempt_at = until;
        self
    }
}

pub fn settle(
    domain: NormalizedDomain,
    build: DossierBuild,
    previous: Option<&CompanyDossier>,
    now: DateTime<Utc>,
    refresh: Duration,
) -> CompanyDossier {
    let failure = match (&build.failure, build.reachable) {
        (Some(message), _) => Some(bounded_error(message)),
        (None, false) => Some("company site unreachable".to_owned()),
        (None, true) => None,
    };
    let attempts = if failure.is_some() {
        previous
            .map_or(0, |previous| previous.attempts)
            .saturating_add(1)
    } else {
        0
    };
    let next_attempt_at = match failure {
        Some(_) if attempts <= SUMMARY_FAILED_AFTER => now + summary_retry_delay(attempts),
        _ => now + refresh,
    };
    let kept = previous.filter(|previous| previous.has_content());
    if let Some(previous) = kept.filter(|_| !build.reachable) {
        return CompanyDossier {
            built_at: Some(now),
            attempts,
            next_attempt_at,
            error: failure,
            ..previous.clone()
        };
    }
    let official = build.official.clone().or_else(|| {
        let siren = build.siren.as_ref()?;
        kept?
            .official
            .clone()
            .filter(|official| &official.siren == siren && build.failure.is_some())
    });
    let build = DossierBuild { official, ..build };
    let content_hash = build.content_hash();
    let content_at = match (&content_hash, previous) {
        (Some(hash), Some(previous)) if previous.content_hash.as_ref() == Some(hash) => {
            previous.content_at
        }
        (Some(_), _) => Some(now),
        (None, _) => None,
    };
    CompanyDossier {
        domain,
        state: build.state(),
        siren: build.siren.clone().or_else(|| {
            build
                .official
                .as_ref()
                .map(|official| official.siren.clone())
        }),
        official: build.official,
        pages: build.pages,
        content_hash,
        content_at,
        built_at: Some(now),
        attempts,
        next_attempt_at,
        error: failure,
        ignored_links: ignored_count(&build.unconfirmed),
        pages_checked_at: Some(now),
    }
}

pub fn bounded_error(message: &str) -> String {
    truncated(message, ERROR_MAX_CHARS)
}

pub trait CompanyBuilder: Send + Sync {
    fn build(
        &self,
        domain: &NormalizedDomain,
    ) -> impl Future<Output = Result<DossierBuild, SourceError>> + Send;
}

pub trait PageVerifier: Send + Sync {
    fn review(
        &self,
        pages: &[CompanyPage],
    ) -> impl Future<Output = Result<PageReview, SourceError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn official() -> OfficialData {
        OfficialData {
            siren: Siren::parse("552081317").unwrap(),
            legal_name: "ELECTRICITE DE FRANCE".to_owned(),
            naf_code: Some("35.11Z".to_owned()),
            naf_label: Some("Production et distribution d'électricité".to_owned()),
            headcount_code: Some("53".to_owned()),
            headcount_range: Some("10 000 salariés et plus".to_owned()),
            head_office_city: Some("PARIS".to_owned()),
            head_office_postcode: Some("75008".to_owned()),
            head_office_country: Some("France".to_owned()),
            head_office_country_code: Some("FR".to_owned()),
            created_on: NaiveDate::from_ymd_opt(1955, 1, 1),
        }
    }

    fn dossier(pages: Vec<CompanyPage>, official: Option<OfficialData>) -> CompanyDossier {
        CompanyDossier {
            domain: NormalizedDomain::from_host("edf.fr").unwrap(),
            state: DossierState::Complete,
            siren: official.as_ref().map(|official| official.siren.clone()),
            official,
            pages,
            content_hash: None,
            content_at: None,
            built_at: None,
            attempts: 0,
            next_attempt_at: DateTime::from_timestamp(0, 0).unwrap(),
            error: None,
            ignored_links: 0,
            pages_checked_at: None,
        }
    }

    #[test]
    fn states_round_trip() {
        for state in [
            DossierState::Pending,
            DossierState::Complete,
            DossierState::Partial,
            DossierState::Unreachable,
            DossierState::HostingPlatform,
        ] {
            assert_eq!(DossierState::parse(state.as_str()), Some(state));
        }
        assert_eq!(DossierState::parse("nope"), None);
    }

    #[test]
    fn the_official_data_renders_as_lines() {
        let text = official().render();
        assert!(text.starts_with(
            "Company dossier, official data from the French company register (SIREN 552081317):"
        ));
        assert!(text.contains("- legal name: ELECTRICITE DE FRANCE"));
        assert!(text.contains("- main activity (NAF): 35.11Z, Production"));
        assert!(text.contains("- headcount range: 10 000 salariés et plus"));
        assert!(text.contains("- head office: 75008 PARIS France"));
        assert!(text.contains("- created on: 1955-01-01"));
    }

    #[test]
    fn documents_hold_the_official_data_then_each_page_truncated_with_its_url() {
        let pages = vec![
            CompanyPage {
                url: "https://edf.fr/".to_owned(),
                text: "a".repeat(50),
            },
            CompanyPage {
                url: "https://edf.fr/a-propos".to_owned(),
                text: "b".repeat(5),
            },
        ];
        let documents = dossier(pages, Some(official())).documents(10);
        assert_eq!(documents.len(), 3);
        assert_eq!(
            documents[0].source,
            "https://recherche-entreprises.api.gouv.fr/search?q=552081317"
        );
        assert_eq!(documents[1].source, "https://edf.fr/");
        assert!(
            documents[1]
                .content
                .ends_with(&format!("\n{}", "a".repeat(10)))
        );
        assert!(documents[2].content.ends_with("\nbbbbb"));
    }

    #[test]
    fn a_dossier_without_content_has_no_document() {
        let empty = dossier(Vec::new(), None);
        assert!(!empty.has_content());
        assert!(empty.documents(10).is_empty());
    }

    fn build(reachable: bool, official: Option<OfficialData>, pages: usize) -> DossierBuild {
        DossierBuild {
            siren: None,
            official,
            pages: (0..pages)
                .map(|index| CompanyPage {
                    url: format!("https://edf.fr/{index}"),
                    text: "text".to_owned(),
                })
                .collect(),
            reachable,
            failure: None,
            unconfirmed: UnconfirmedLinks::default(),
        }
    }

    #[test]
    fn the_state_follows_reachability_and_official_data() {
        assert_eq!(build(false, None, 0).state(), DossierState::Unreachable);
        assert_eq!(build(true, None, 1).state(), DossierState::Partial);
        assert_eq!(
            build(true, Some(official()), 1).state(),
            DossierState::Complete
        );
    }

    fn domain() -> NormalizedDomain {
        NormalizedDomain::from_host("edf.fr").unwrap()
    }

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    const REFRESH: Duration = Duration::days(90);

    #[test]
    fn a_complete_build_is_refreshed_after_the_interval_and_dates_its_content() {
        let settled = settle(
            domain(),
            build(true, Some(official()), 2),
            None,
            at(1_000),
            REFRESH,
        );
        assert_eq!(settled.state, DossierState::Complete);
        assert_eq!(settled.content_at, Some(at(1_000)));
        assert_eq!(settled.built_at, Some(at(1_000)));
        assert_eq!(settled.attempts, 0);
        assert_eq!(settled.next_attempt_at, at(1_000) + REFRESH);
        assert_eq!(settled.siren, Some(official().siren));
    }

    #[test]
    fn an_identical_rebuild_keeps_the_content_date() {
        let first = settle(
            domain(),
            build(true, Some(official()), 2),
            None,
            at(1_000),
            REFRESH,
        );
        let again = settle(
            domain(),
            build(true, Some(official()), 2),
            Some(&first),
            at(9_000),
            REFRESH,
        );
        assert_eq!(again.content_at, Some(at(1_000)));
        assert_eq!(again.built_at, Some(at(9_000)));
        let changed = settle(
            domain(),
            build(true, Some(official()), 3),
            Some(&first),
            at(9_000),
            REFRESH,
        );
        assert_eq!(changed.content_at, Some(at(9_000)));
    }

    #[test]
    fn an_unreachable_site_backs_off_along_the_failure_schedule() {
        let mut build = build(false, None, 0);
        build.failure = Some("robots.txt".to_owned());
        let first = settle(domain(), build.clone(), None, at(0), REFRESH);
        assert_eq!(first.state, DossierState::Unreachable);
        assert_eq!(first.attempts, 1);
        assert_eq!(first.next_attempt_at, at(60));
        assert_eq!(first.content_at, None);
        let second = settle(domain(), build.clone(), Some(&first), at(0), REFRESH);
        assert_eq!(second.attempts, 2);
        assert_eq!(second.next_attempt_at, at(300));
        let mut previous = first;
        previous.attempts = SUMMARY_FAILED_AFTER;
        let exhausted = settle(domain(), build, Some(&previous), at(0), REFRESH);
        assert_eq!(exhausted.next_attempt_at, at(0) + REFRESH);
    }

    #[test]
    fn a_failed_refresh_keeps_the_content_already_collected() {
        let first = settle(
            domain(),
            build(true, Some(official()), 2),
            None,
            at(1_000),
            REFRESH,
        );
        let mut down = build(false, None, 0);
        down.failure = Some("timeout".to_owned());
        let kept = settle(domain(), down, Some(&first), at(2_000), REFRESH);
        assert_eq!(kept.state, DossierState::Complete);
        assert_eq!(kept.pages.len(), 2);
        assert_eq!(kept.content_at, Some(at(1_000)));
        assert_eq!(kept.attempts, 1);
        assert_eq!(kept.error.as_deref(), Some("timeout"));
    }

    #[test]
    fn a_failed_lookup_keeps_the_official_data_of_the_same_siren() {
        let first = settle(
            domain(),
            build(true, Some(official()), 1),
            None,
            at(1_000),
            REFRESH,
        );
        let mut lookup_failed = build(true, None, 1);
        lookup_failed.siren = Some(official().siren);
        lookup_failed.failure = Some("lookup failed".to_owned());
        let settled = settle(domain(), lookup_failed, Some(&first), at(2_000), REFRESH);
        assert_eq!(settled.state, DossierState::Complete);
        assert_eq!(settled.official, Some(official()));
        assert_eq!(settled.attempts, 1);
    }

    #[test]
    fn a_partial_build_without_failure_waits_for_the_refresh_interval() {
        let settled = settle(domain(), build(true, None, 1), None, at(0), REFRESH);
        assert_eq!(settled.state, DossierState::Partial);
        assert_eq!(settled.attempts, 0);
        assert_eq!(settled.next_attempt_at, at(0) + REFRESH);
    }

    #[test]
    fn the_content_hash_tracks_the_content_only() {
        assert_eq!(build(false, None, 0).content_hash(), None);
        let first = build(true, None, 1).content_hash();
        assert!(first.is_some());
        assert_eq!(first, build(true, None, 1).content_hash());
        assert_ne!(first, build(true, None, 2).content_hash());
    }

    #[test]
    fn a_build_records_its_ignored_links_and_marks_its_pages_checked() {
        let mut built = build(true, None, 1);
        built.unconfirmed.record(LinkRejection::Soft404Marker);
        built.unconfirmed.record(LinkRejection::Status);
        let settled = settle(domain(), built, None, at(1_000), REFRESH);
        assert_eq!(settled.ignored_links, 2);
        assert_eq!(settled.pages_checked_at, Some(at(1_000)));
    }

    #[test]
    fn a_review_drops_rejected_pages_and_dates_the_content_only_when_it_changed() {
        let first = settle(domain(), build(true, None, 2), None, at(1_000), REFRESH);
        let unchanged = first.reviewed(&PageReview::default(), at(5_000));
        assert_eq!(unchanged.pages.len(), 2);
        assert_eq!(unchanged.content_at, Some(at(1_000)));
        assert_eq!(unchanged.content_hash, first.content_hash);
        assert_eq!(unchanged.pages_checked_at, Some(at(5_000)));
        let review = PageReview {
            rejected: vec![(
                "https://edf.fr/1".to_owned(),
                LinkRejection::Soft404Baseline,
            )],
        };
        let pruned = first.reviewed(&review, at(6_000));
        assert_eq!(pruned.pages.len(), 1);
        assert_eq!(pruned.pages[0].url, "https://edf.fr/0");
        assert_eq!(pruned.content_at, Some(at(6_000)));
        assert_ne!(pruned.content_hash, first.content_hash);
        assert_eq!(pruned.ignored_links, 1);
        assert_eq!(review.unconfirmed().soft_404_baseline, 1);
    }

    #[test]
    fn a_review_that_removes_every_page_still_dates_the_change() {
        let first = settle(domain(), build(true, None, 1), None, at(1_000), REFRESH);
        let review = PageReview {
            rejected: vec![("https://edf.fr/0".to_owned(), LinkRejection::EmptyContent)],
        };
        let emptied = first.reviewed(&review, at(2_000));
        assert!(emptied.pages.is_empty());
        assert_eq!(emptied.content_hash, None);
        assert_eq!(emptied.content_at, Some(at(2_000)));
    }

    #[test]
    fn a_platform_dossier_is_terminal_and_reads_nothing() {
        let now = DateTime::from_timestamp(1_000, 0).unwrap();
        let dossier = CompanyDossier::on_hosting_platform(
            NormalizedDomain::from_host("acme.github.io").unwrap(),
            now,
            REFRESH,
        );
        assert_eq!(dossier.state, DossierState::HostingPlatform);
        assert_eq!(dossier.built_at, Some(now));
        assert_eq!(dossier.next_attempt_at, now + REFRESH);
        assert!(!dossier.has_content());
        assert!(dossier.documents(PAGE_PROMPT_CHARS).is_empty());
    }
}
