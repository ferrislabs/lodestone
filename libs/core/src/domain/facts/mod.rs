use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, SecondsFormat, Utc};

use crate::domain::{
    NormalizedDomain,
    company::{CompanyDossier, DossierState},
    host::{CandidateHost, Hostname},
    lead::{Lead, Signal},
    values::{SignalKind, SourceName, UnitInterval},
};

mod country;
mod environment;
mod sentence;

pub use country::Country;
pub use environment::{Environment, environment_of};
pub use sentence::signal_sentence;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFact {
    pub host: Hostname,
    pub environment: Environment,
    pub signals: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountryBasis {
    HeadOffice,
    Tld,
}

impl CountryBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HeadOffice => "head_office",
            Self::Tld => "tld",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficialFacts {
    pub siren: String,
    pub legal_name: String,
    pub naf_code: Option<String>,
    pub naf_label: Option<String>,
    pub headcount_range: Option<String>,
    pub head_office_country: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyFacts {
    pub state: DossierState,
    pub siren_found: bool,
    pub official: Option<OfficialFacts>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndependentSignal {
    pub kind: SignalKind,
    pub strength: UnitInterval,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct SiblingSignals<'a> {
    pub domain: &'a NormalizedDomain,
    pub signals: &'a [Signal],
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeadFacts {
    pub hosts: Vec<HostFact>,
    pub distinct_subdomains: usize,
    pub signal_count: usize,
    pub independent: Vec<IndependentSignal>,
    pub kinds: Vec<SignalKind>,
    pub sources: Vec<SourceName>,
    pub sibling_domains: Vec<NormalizedDomain>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub probable_country: Option<Country>,
    pub country_basis: Option<CountryBasis>,
    pub company: Option<CompanyFacts>,
}

fn belongs_to(host: &Hostname, domain: &NormalizedDomain) -> bool {
    host.registrable_domain().as_ref() == Some(domain)
}

fn signal_host(signal: &Signal, domain: &NormalizedDomain) -> Option<Hostname> {
    sentence::host_candidates(signal)
        .iter()
        .filter_map(|raw| Hostname::parse(raw).ok())
        .find(|host| belongs_to(host, domain))
}

impl CompanyFacts {
    fn render(&self) -> String {
        match (&self.official, self.state) {
            (Some(official), _) => {
                let mut parts = vec![format!("legal name {}", official.legal_name)];
                match (&official.naf_code, &official.naf_label) {
                    (Some(code), Some(label)) => parts.push(format!("activity NAF {code} ({label})")),
                    (Some(code), None) => parts.push(format!("activity NAF {code}")),
                    _ => {}
                }
                if let Some(range) = &official.headcount_range {
                    parts.push(format!("headcount range {range}"));
                }
                if let Some(country) = &official.head_office_country {
                    parts.push(format!("head office country {country}"));
                }
                format!(
                    "- official data (French company register, SIREN {}): {}",
                    official.siren,
                    parts.join("; ")
                )
            }
            (None, DossierState::HostingPlatform) => {
                "- company dossier: none, the domain is hosted on a platform, no company site was read".to_owned()
            }
            (None, DossierState::Unreachable) => {
                "- company dossier: unreachable, the company site could not be read".to_owned()
            }
            (None, _) if self.siren_found => {
                "- company dossier: partial, the official lookup of the SIREN found on the site failed".to_owned()
            }
            (None, _) => {
                "- company dossier: partial, SIREN not found on the company site (SIREN introuvable), no official data".to_owned()
            }
        }
    }
}

fn normalised_value(value: Option<&str>) -> String {
    value
        .map(|raw| raw.trim().to_lowercase())
        .unwrap_or_default()
}

pub fn independent_signals<'a>(
    signals: impl Iterator<Item = &'a Signal>,
) -> Vec<IndependentSignal> {
    let mut latest: BTreeMap<(&str, String, &str), &Signal> = BTreeMap::new();
    for signal in signals {
        let key = (
            signal.kind.as_str(),
            normalised_value(signal.value.as_deref()),
            signal.source.as_str(),
        );
        latest
            .entry(key)
            .and_modify(|kept| {
                if (signal.observed_at, signal.strength.get())
                    .partial_cmp(&(kept.observed_at, kept.strength.get()))
                    .is_some_and(std::cmp::Ordering::is_gt)
                {
                    *kept = signal;
                }
            })
            .or_insert(signal);
    }
    latest
        .into_values()
        .map(|signal| IndependentSignal {
            kind: signal.kind.clone(),
            strength: signal.strength,
            observed_at: signal.observed_at,
        })
        .collect()
}

impl LeadFacts {
    pub fn compute(lead: &Lead, signals: &[Signal], known_hosts: &[CandidateHost]) -> Self {
        Self::compute_aggregated(lead, signals, known_hosts, &[])
    }

    pub fn compute_aggregated(
        lead: &Lead,
        signals: &[Signal],
        known_hosts: &[CandidateHost],
        siblings: &[SiblingSignals<'_>],
    ) -> Self {
        let pools: Vec<(&NormalizedDomain, &[Signal])> = std::iter::once((&lead.domain, signals))
            .chain(
                siblings
                    .iter()
                    .map(|sibling| (sibling.domain, sibling.signals)),
            )
            .collect();
        let domains: Vec<&NormalizedDomain> = pools.iter().map(|(domain, _)| *domain).collect();
        let all_signals = || pools.iter().flat_map(|(_, pool)| pool.iter());
        let mut counts: BTreeMap<Hostname, usize> = BTreeMap::new();
        for (domain, pool) in &pools {
            for signal in *pool {
                if let Some(host) = signal_host(signal, domain) {
                    *counts.entry(host).or_default() += 1;
                }
            }
        }
        let mut subdomains: BTreeSet<&Hostname> = counts.keys().collect();
        subdomains.extend(
            known_hosts
                .iter()
                .filter(|known| domains.contains(&&known.domain))
                .map(|known| &known.host),
        );
        let distinct_subdomains = subdomains
            .iter()
            .filter(|host| {
                !domains
                    .iter()
                    .any(|domain| host.as_str() == domain.as_str())
            })
            .count();
        let mut sources: Vec<SourceName> = all_signals().map(|signal| signal.source).collect();
        sources.sort_by_key(|source| source.as_str());
        sources.dedup();
        let mut kinds: Vec<SignalKind> = all_signals().map(|signal| signal.kind.clone()).collect();
        kinds.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        kinds.dedup();
        let probable_country = country::country_of(&lead.domain);
        Self {
            hosts: counts
                .into_iter()
                .map(|(host, signals)| HostFact {
                    environment: environment_of(&host),
                    host,
                    signals,
                })
                .collect(),
            distinct_subdomains,
            signal_count: all_signals().count(),
            independent: independent_signals(all_signals()),
            kinds,
            sources,
            sibling_domains: siblings
                .iter()
                .map(|sibling| sibling.domain.clone())
                .collect(),
            first_seen_at: lead.first_seen_at,
            last_seen_at: lead.last_seen_at,
            country_basis: probable_country.map(|_| CountryBasis::Tld),
            probable_country,
            company: None,
        }
    }

    pub fn independent_signals(&self) -> usize {
        self.independent.len()
    }

    pub fn with_dossier(mut self, dossier: Option<&CompanyDossier>) -> Self {
        let Some(dossier) = dossier.filter(|dossier| dossier.state != DossierState::Pending) else {
            return self;
        };
        let official = dossier.official.as_ref().map(|official| OfficialFacts {
            siren: official.siren.to_string(),
            legal_name: official.legal_name.clone(),
            naf_code: official.naf_code.clone(),
            naf_label: official.naf_label.clone(),
            headcount_range: official.headcount_range.clone(),
            head_office_country: official.head_office_country.clone(),
        });
        let head_office = dossier
            .official
            .as_ref()
            .and_then(|official| official.head_office_country_code.as_deref())
            .and_then(Country::from_code);
        if let Some(country) = head_office {
            self.probable_country = Some(country);
            self.country_basis = Some(CountryBasis::HeadOffice);
        }
        self.company = Some(CompanyFacts {
            state: dossier.state,
            siren_found: dossier.siren.is_some(),
            official,
        });
        self
    }

    pub fn distinct_hosts(&self) -> usize {
        self.hosts.len()
    }

    pub fn production_hosts(&self) -> usize {
        self.hosts
            .iter()
            .filter(|fact| fact.environment.is_production())
            .count()
    }

    pub fn production_share(&self) -> Option<f64> {
        if self.hosts.is_empty() {
            return None;
        }
        let production = u32::try_from(self.production_hosts()).unwrap_or(u32::MAX);
        let total = u32::try_from(self.hosts.len()).unwrap_or(u32::MAX);
        Some(f64::from(production) / f64::from(total))
    }

    pub fn only_non_production(&self) -> bool {
        !self.hosts.is_empty() && self.production_hosts() == 0
    }

    pub fn render(&self) -> String {
        let mut lines = vec![
            "Facts computed by the system from data already collected (no external lookup):"
                .to_owned(),
        ];
        if self.hosts.is_empty() {
            lines.push("- hosts carrying signals: none, no signal names a host".to_owned());
        } else {
            let hosts: Vec<String> = self
                .hosts
                .iter()
                .map(|fact| format!("{} ({})", fact.host, fact.environment.as_str()))
                .collect();
            lines.push(format!(
                "- hosts carrying signals: {} ({})",
                self.hosts.len(),
                hosts.join(", ")
            ));
            lines.push(format!(
                "- production hosts: {} of {}",
                self.production_hosts(),
                self.hosts.len()
            ));
            if self.only_non_production() {
                lines.push(
                    "- warning: every host carrying a signal is a non-production environment (dev, staging, test, demo...), so the signals say little about the company's real stack"
                        .to_owned(),
                );
            }
        }
        lines.push(format!(
            "- distinct subdomains seen for the domain: {}",
            self.distinct_subdomains
        ));
        if !self.sibling_domains.is_empty() {
            let domains: Vec<&str> = self
                .sibling_domains
                .iter()
                .map(NormalizedDomain::as_str)
                .collect();
            lines.push(format!(
                "- aggregated with the other domains of the same company in this profile: {}",
                domains.join(", ")
            ));
        }
        let sources: Vec<&str> = self.sources.iter().map(|source| source.as_str()).collect();
        lines.push(format!(
            "- signals: {} from {} distinct source(s){}",
            self.signal_count,
            self.sources.len(),
            if sources.is_empty() {
                String::new()
            } else {
                format!(" ({})", sources.join(", "))
            }
        ));
        lines.push(format!(
            "- first seen: {}",
            self.first_seen_at
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        ));
        lines.push(format!(
            "- last seen: {}",
            self.last_seen_at.to_rfc3339_opts(SecondsFormat::Secs, true)
        ));
        match (self.probable_country, self.country_basis) {
            (Some(country), Some(CountryBasis::HeadOffice)) => lines.push(format!(
                "- probable country, from the head office in the official company register: {} ({})",
                country.name, country.code
            )),
            (Some(country), _) => lines.push(format!(
                "- probable country, from the domain extension only: {} ({})",
                country.name, country.code
            )),
            (None, _) => {
                lines.push("- probable country: unknown (generic domain extension)".to_owned());
            }
        }
        if let Some(company) = &self.company {
            lines.push(company.render());
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;
    use crate::domain::{
        host::HostOrigin,
        lead::{LeadId, LeadStatus, Score, SignalId},
        profile::ProfileId,
        values::{Provenance, SignalKind, UnitInterval},
    };

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).unwrap()
    }

    fn lead(domain: &str) -> Lead {
        Lead {
            id: LeadId(uuid::Uuid::nil()),
            profile_id: ProfileId(uuid::Uuid::nil()),
            domain: NormalizedDomain::from_host(domain).unwrap(),
            status: LeadStatus::New,
            score: Score::default(),
            confidence: None,
            verdict: None,
            llm_score: None,
            qualified_at: None,
            first_seen_at: at(1_800_000_000),
            last_seen_at: at(1_800_086_400),
            updated_at: at(1_800_086_400),
        }
    }

    fn signal(kind: &str, source: SourceName, host: Option<&str>, provenance: &str) -> Signal {
        Signal {
            id: SignalId(uuid::Uuid::new_v4()),
            lead_id: LeadId(uuid::Uuid::nil()),
            kind: SignalKind::parse(kind).unwrap(),
            value: Some("auth0".to_owned()),
            source,
            raw: host.map_or_else(|| json!({}), |host| json!({"host": host})),
            provenance: Provenance::parse(provenance).unwrap(),
            strength: UnitInterval::new(0.9).unwrap(),
            observed_at: at(1_800_000_000),
            agent: None,
        }
    }

    fn cname(host: &str) -> Signal {
        signal(
            "saas_provider",
            SourceName::DnsCname,
            Some(host),
            &format!("dns:{host}?type=CNAME"),
        )
    }

    fn known(host: &str) -> CandidateHost {
        CandidateHost::discovered(
            Hostname::parse(host).unwrap(),
            HostOrigin::CertificateTransparency,
            at(1_800_000_000),
        )
        .unwrap()
    }

    #[test]
    fn the_motivating_lead_has_only_a_development_host() {
        let facts = LeadFacts::compute(
            &lead("luzernrisk.com"),
            &[cname("login.dev.luzernrisk.com")],
            &[],
        );
        assert_eq!(facts.hosts.len(), 1);
        assert_eq!(facts.hosts[0].environment, Environment::Development);
        assert_eq!(facts.production_hosts(), 0);
        assert_eq!(facts.production_share(), Some(0.0));
        assert!(facts.only_non_production());
        assert_eq!(facts.signal_count, 1);
        assert_eq!(facts.sources, vec![SourceName::DnsCname]);
        assert_eq!(facts.distinct_hosts(), 1);
        assert_eq!(facts.distinct_subdomains, 1);
        assert_eq!(facts.probable_country, None);
    }

    #[test]
    fn the_production_share_counts_hosts_not_signals() {
        let signals = [
            cname("sso.acme.com"),
            cname("sso.acme.com"),
            cname("sso.acme.com"),
            cname("dev.acme.com"),
        ];
        let facts = LeadFacts::compute(&lead("acme.com"), &signals, &[]);
        assert_eq!(facts.distinct_hosts(), 2);
        assert_eq!(facts.production_hosts(), 1);
        assert_eq!(facts.production_share(), Some(0.5));
        assert!(!facts.only_non_production());
        assert_eq!(facts.signal_count, 4);
        let sso = facts
            .hosts
            .iter()
            .find(|fact| fact.host.as_str() == "sso.acme.com")
            .unwrap();
        assert_eq!(sso.signals, 3);
    }

    #[test]
    fn a_lead_without_hosts_has_no_share_and_no_warning() {
        let tender = signal(
            "public_tender",
            SourceName::Boamp,
            None,
            "https://www.boamp.fr/avis/1",
        );
        let facts = LeadFacts::compute(&lead("acme.fr"), &[tender], &[]);
        assert!(facts.hosts.is_empty());
        assert_eq!(facts.production_share(), None);
        assert!(!facts.only_non_production());
        assert_eq!(facts.probable_country.unwrap().code, "FR");
        assert!(facts.render().contains("no signal names a host"));
    }

    #[test]
    fn a_host_of_another_domain_is_ignored() {
        let foreign = signal(
            "job_offer",
            SourceName::Ats,
            None,
            "https://boards.greenhouse.io/acme/jobs/1",
        );
        let facts = LeadFacts::compute(&lead("acme.com"), &[foreign], &[]);
        assert!(facts.hosts.is_empty());
        let lying = cname("sso.other.com");
        assert!(
            LeadFacts::compute(&lead("acme.com"), &[lying], &[])
                .hosts
                .is_empty()
        );
    }

    #[test]
    fn the_provenance_names_the_host_when_the_raw_data_does_not() {
        let probe = signal(
            "product_detected",
            SourceName::HttpProbe,
            None,
            "https://staging.acme.com/realms/master",
        );
        let facts = LeadFacts::compute(&lead("acme.com"), &[probe], &[]);
        assert_eq!(facts.hosts[0].host.as_str(), "staging.acme.com");
        assert_eq!(facts.hosts[0].environment, Environment::Staging);
    }

    #[test]
    fn subdomains_merge_signal_hosts_and_candidate_hosts_without_the_apex() {
        let known_hosts = [
            known("sso.acme.com"),
            known("dev.acme.com"),
            known("acme.com"),
            known("sso.other.com"),
        ];
        let facts = LeadFacts::compute(&lead("acme.com"), &[cname("sso.acme.com")], &known_hosts);
        assert_eq!(facts.distinct_subdomains, 2);
        assert_eq!(facts.distinct_hosts(), 1);
    }

    #[test]
    fn sources_are_distinct_and_sorted() {
        let signals = [
            cname("a.acme.com"),
            signal(
                "product_detected",
                SourceName::HttpProbe,
                Some("a.acme.com"),
                "https://a.acme.com/",
            ),
            cname("b.acme.com"),
        ];
        let facts = LeadFacts::compute(&lead("acme.com"), &signals, &[]);
        assert_eq!(
            facts.sources,
            vec![SourceName::DnsCname, SourceName::HttpProbe]
        );
        assert_eq!(facts.signal_count, 3);
    }

    #[test]
    fn the_dates_are_the_lead_dates() {
        let facts = LeadFacts::compute(&lead("acme.com"), &[], &[]);
        assert_eq!(facts.first_seen_at, at(1_800_000_000));
        assert_eq!(facts.last_seen_at, at(1_800_086_400));
    }

    #[test]
    fn the_rendering_documents_every_fact() {
        let facts = LeadFacts::compute(
            &lead("luzernrisk.ch"),
            &[cname("login.dev.luzernrisk.ch")],
            &[known("login.dev.luzernrisk.ch"), known("www.luzernrisk.ch")],
        );
        let text = facts.render();
        assert!(text.starts_with("Facts computed by the system"));
        assert!(
            text.contains("- hosts carrying signals: 1 (login.dev.luzernrisk.ch (development))")
        );
        assert!(text.contains("- production hosts: 0 of 1"));
        assert!(text.contains("- warning: every host carrying a signal is a non-production"));
        assert!(text.contains("- distinct subdomains seen for the domain: 2"));
        assert!(text.contains("- signals: 1 from 1 distinct source(s) (dns_cname)"));
        assert!(text.contains("- first seen: 2027-01-15T08:00:00Z"));
        assert!(text.contains("- last seen: 2027-01-16T08:00:00Z"));
        assert!(
            text.contains("- probable country, from the domain extension only: Switzerland (CH)")
        );
    }

    fn dossier(
        state: DossierState,
        official: Option<crate::domain::company::OfficialData>,
        siren_found: bool,
    ) -> CompanyDossier {
        let mut dossier = CompanyDossier::pending(
            NormalizedDomain::from_host("acme.com").unwrap(),
            at(1_800_000_000),
        );
        dossier.state = state;
        dossier.siren =
            siren_found.then(|| crate::domain::company::Siren::parse("552081317").unwrap());
        dossier.official = official;
        dossier
    }

    fn official(code: Option<&str>) -> crate::domain::company::OfficialData {
        crate::domain::company::OfficialData {
            siren: crate::domain::company::Siren::parse("552081317").unwrap(),
            legal_name: "ACME SAS".to_owned(),
            naf_code: Some("62.01Z".to_owned()),
            naf_label: Some("Information et communication".to_owned()),
            headcount_code: Some("12".to_owned()),
            headcount_range: Some("20 à 49 salariés".to_owned()),
            head_office_city: Some("PARIS".to_owned()),
            head_office_postcode: Some("75008".to_owned()),
            head_office_country: Some("France".to_owned()),
            head_office_country_code: code.map(str::to_owned),
            created_on: None,
        }
    }

    #[test]
    fn the_head_office_country_wins_over_the_extension_and_names_its_basis() {
        let plain = LeadFacts::compute(&lead("acme.com"), &[], &[]);
        assert_eq!(plain.probable_country, None);
        assert_eq!(plain.country_basis, None);
        let facts = plain.with_dossier(Some(&dossier(
            DossierState::Complete,
            Some(official(Some("FR"))),
            true,
        )));
        assert_eq!(facts.probable_country.unwrap().code, "FR");
        assert_eq!(facts.country_basis, Some(CountryBasis::HeadOffice));
        let official = facts.company.as_ref().unwrap().official.as_ref().unwrap();
        assert_eq!(official.legal_name, "ACME SAS");
        assert_eq!(official.naf_code.as_deref(), Some("62.01Z"));
        assert_eq!(
            official.headcount_range.as_deref(),
            Some("20 à 49 salariés")
        );
        assert_eq!(official.head_office_country.as_deref(), Some("France"));
        let text = facts.render();
        assert!(text.contains(
            "- probable country, from the head office in the official company register: France (FR)"
        ));
        assert!(!text.contains("from the domain extension only"));
        assert!(text.contains(
            "- official data (French company register, SIREN 552081317): legal name ACME SAS; activity NAF 62.01Z (Information et communication); headcount range 20 à 49 salariés; head office country France"
        ));
    }

    #[test]
    fn the_extension_stays_the_basis_when_the_head_office_country_is_unknown() {
        let facts = LeadFacts::compute(&lead("acme.de"), &[], &[]).with_dossier(Some(&dossier(
            DossierState::Complete,
            Some(official(None)),
            true,
        )));
        assert_eq!(facts.probable_country.unwrap().code, "DE");
        assert_eq!(facts.country_basis, Some(CountryBasis::Tld));
    }

    #[test]
    fn a_dossier_without_official_data_says_why() {
        let base = LeadFacts::compute(&lead("acme.fr"), &[], &[]);
        let not_found =
            base.clone()
                .with_dossier(Some(&dossier(DossierState::Partial, None, false)));
        assert!(
            not_found
                .render()
                .contains("SIREN not found on the company site (SIREN introuvable)")
        );
        assert_eq!(not_found.country_basis, Some(CountryBasis::Tld));
        let failed = base
            .clone()
            .with_dossier(Some(&dossier(DossierState::Partial, None, true)));
        assert!(
            failed
                .render()
                .contains("the official lookup of the SIREN found on the site failed")
        );
        let unreachable =
            base.clone()
                .with_dossier(Some(&dossier(DossierState::Unreachable, None, false)));
        assert!(
            unreachable
                .render()
                .contains("unreachable, the company site could not be read")
        );
        let hosted =
            base.clone()
                .with_dossier(Some(&dossier(DossierState::HostingPlatform, None, false)));
        assert!(
            hosted
                .render()
                .contains("the domain is hosted on a platform, no company site was read")
        );
    }

    #[test]
    fn a_pending_or_missing_dossier_adds_nothing() {
        let base = LeadFacts::compute(&lead("acme.fr"), &[], &[]);
        let pending = base
            .clone()
            .with_dossier(Some(&dossier(DossierState::Pending, None, false)));
        assert_eq!(pending, base);
        assert_eq!(base.clone().with_dossier(None), base);
        assert!(!base.render().contains("company dossier"));
    }

    fn valued(kind: &str, value: &str, source: SourceName, host: &str, seconds: i64) -> Signal {
        Signal {
            value: Some(value.to_owned()),
            observed_at: at(seconds),
            ..signal(kind, source, Some(host), &format!("https://{host}/{value}"))
        }
    }

    #[test]
    fn identical_kind_value_and_source_are_one_independent_signal() {
        let signals = [
            valued(
                "saas_provider",
                "Auth0",
                SourceName::DnsCname,
                "a.acme.com",
                10,
            ),
            valued(
                "saas_provider",
                " auth0 ",
                SourceName::DnsCname,
                "b.acme.com",
                30,
            ),
            valued(
                "saas_provider",
                "auth0",
                SourceName::HttpProbe,
                "a.acme.com",
                20,
            ),
            valued(
                "saas_provider",
                "okta",
                SourceName::DnsCname,
                "a.acme.com",
                20,
            ),
            valued(
                "product_detected",
                "auth0",
                SourceName::DnsCname,
                "a.acme.com",
                20,
            ),
        ];
        let facts = LeadFacts::compute(&lead("acme.com"), &signals, &[]);
        assert_eq!(facts.signal_count, 5);
        assert_eq!(facts.independent_signals(), 4);
        assert_eq!(facts.sources.len(), 2);
        assert_eq!(facts.kinds.len(), 2);
        let merged = facts
            .independent
            .iter()
            .find(|signal| signal.kind.as_str() == "saas_provider" && signal.observed_at == at(30))
            .unwrap();
        assert_eq!(merged.kind.as_str(), "saas_provider");
    }

    #[test]
    fn siblings_add_their_signals_sources_kinds_and_hosts() {
        let own = [valued(
            "saas_provider",
            "auth0",
            SourceName::DnsCname,
            "sso.acme.com",
            0,
        )];
        let other = [
            valued(
                "saas_provider",
                "auth0",
                SourceName::DnsCname,
                "sso.acme.fr",
                0,
            ),
            valued("job_offer", "devops", SourceName::Ats, "jobs.acme.fr", 0),
        ];
        let sibling_domain = NormalizedDomain::from_host("acme.fr").unwrap();
        let facts = LeadFacts::compute_aggregated(
            &lead("acme.com"),
            &own,
            &[known("www.acme.fr"), known("www.elsewhere.com")],
            &[SiblingSignals {
                domain: &sibling_domain,
                signals: &other,
            }],
        );
        assert_eq!(facts.signal_count, 3);
        assert_eq!(facts.independent_signals(), 2);
        assert_eq!(facts.sources, vec![SourceName::Ats, SourceName::DnsCname]);
        assert_eq!(facts.kinds.len(), 2);
        assert_eq!(facts.distinct_hosts(), 3);
        assert_eq!(facts.distinct_subdomains, 4);
        assert_eq!(facts.sibling_domains, vec![sibling_domain]);
        assert!(facts.render().contains(
            "aggregated with the other domains of the same company in this profile: acme.fr"
        ));
    }

    #[test]
    fn a_sibling_signal_naming_a_foreign_host_adds_no_host() {
        let sibling_domain = NormalizedDomain::from_host("acme.fr").unwrap();
        let other = [valued(
            "saas_provider",
            "auth0",
            SourceName::DnsCname,
            "sso.acme.com",
            0,
        )];
        let facts = LeadFacts::compute_aggregated(
            &lead("acme.com"),
            &[],
            &[],
            &[SiblingSignals {
                domain: &sibling_domain,
                signals: &other,
            }],
        );
        assert!(facts.hosts.is_empty());
        assert_eq!(facts.signal_count, 1);
    }

    #[test]
    fn a_production_host_raises_no_warning() {
        let text = LeadFacts::compute(&lead("acme.com"), &[cname("sso.acme.com")], &[]).render();
        assert!(!text.contains("warning"));
        assert!(text.contains("- production hosts: 1 of 1"));
        assert!(text.contains("probable country: unknown"));
    }
}
