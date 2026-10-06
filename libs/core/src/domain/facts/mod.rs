use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, SecondsFormat, Utc};

use crate::domain::{
    NormalizedDomain,
    host::{CandidateHost, Hostname},
    lead::{Lead, Signal},
    values::SourceName,
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

#[derive(Debug, Clone, PartialEq)]
pub struct LeadFacts {
    pub hosts: Vec<HostFact>,
    pub distinct_subdomains: usize,
    pub signal_count: usize,
    pub sources: Vec<SourceName>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub probable_country: Option<Country>,
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

impl LeadFacts {
    pub fn compute(lead: &Lead, signals: &[Signal], known_hosts: &[CandidateHost]) -> Self {
        let mut counts: BTreeMap<Hostname, usize> = BTreeMap::new();
        for signal in signals {
            if let Some(host) = signal_host(signal, &lead.domain) {
                *counts.entry(host).or_default() += 1;
            }
        }
        let mut subdomains: BTreeSet<&Hostname> = counts.keys().collect();
        subdomains.extend(
            known_hosts
                .iter()
                .filter(|known| known.domain == lead.domain)
                .map(|known| &known.host),
        );
        let distinct_subdomains = subdomains
            .iter()
            .filter(|host| host.as_str() != lead.domain.as_str())
            .count();
        let mut sources: Vec<SourceName> = signals.iter().map(|signal| signal.source).collect();
        sources.sort_by_key(|source| source.as_str());
        sources.dedup();
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
            signal_count: signals.len(),
            sources,
            first_seen_at: lead.first_seen_at,
            last_seen_at: lead.last_seen_at,
            probable_country: country::country_of(&lead.domain),
        }
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
        match self.probable_country {
            Some(country) => lines.push(format!(
                "- probable country, from the domain extension only: {} ({})",
                country.name, country.code
            )),
            None => lines.push("- probable country: unknown (generic domain extension)".to_owned()),
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
            score_breakdown: Vec::new(),
            confidence: None,
            verdict: None,
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

    #[test]
    fn a_production_host_raises_no_warning() {
        let text = LeadFacts::compute(&lead("acme.com"), &[cname("sso.acme.com")], &[]).render();
        assert!(!text.contains("warning"));
        assert!(text.contains("- production hosts: 1 of 1"));
        assert!(text.contains("probable country: unknown"));
    }
}
