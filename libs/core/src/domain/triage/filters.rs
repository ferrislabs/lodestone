use serde_json::Value;

use crate::domain::profile::definition::{ContractFilter, RemoteFilter, SearchFilters};

use super::{
    Candidate, contains_phrase,
    countries::{COUNTRIES, alpha2_of_alpha3, alpha2_of_tld},
};

const JOB_OFFER: &str = "job_offer";

const NEGATED_REMOTE: [&str; 11] = [
    "pas de teletravail",
    "sans teletravail",
    "teletravail impossible",
    "teletravail non",
    "teletravail exclu",
    "no remote",
    "not remote",
    "remote not",
    "no teleworking",
    "non teletravaillable",
    "aucun teletravail",
];

const FULL_REMOTE: [&str; 14] = [
    "full remote",
    "full-remote",
    "fully remote",
    "100% remote",
    "100 % remote",
    "100% teletravail",
    "100 % teletravail",
    "remote only",
    "teletravail complet",
    "teletravail total",
    "teletravail integral",
    "entierement a distance",
    "totalement a distance",
    "100% a distance",
];

const HYBRID: [&str; 12] = [
    "hybride",
    "hybrid",
    "teletravail partiel",
    "teletravail occasionnel",
    "teletravail possible",
    "jours de teletravail",
    "jours par semaine en teletravail",
    "jours en teletravail",
    "days remote",
    "remote days",
    "partial remote",
    "flexible remote",
];

const REMOTE_GENERIC: [&str; 7] = [
    "teletravail",
    "remote",
    "a distance",
    "work from home",
    "work from anywhere",
    "home office",
    "teleworking",
];

const ON_SITE: [&str; 7] = [
    "sur site",
    "on-site",
    "onsite",
    "on site",
    "presentiel",
    "au bureau",
    "in the office",
];

const FREELANCE: [&str; 11] = [
    "freelance",
    "free-lance",
    "independant",
    "independent contractor",
    "contractor",
    "auto-entrepreneur",
    "micro-entrepreneur",
    "tjm",
    "mission freelance",
    "b2b",
    "prestataire",
];

const EMPLOYEE: [&str; 9] = [
    "cdi",
    "cdd",
    "permanent",
    "permanent contract",
    "salarie",
    "employee",
    "fixed-term",
    "contrat de travail",
    "interim",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteMode {
    OnSite,
    Hybrid,
    FullRemote,
    Unknown,
}

fn fold(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for letter in text.chars().flat_map(char::to_lowercase) {
        match letter {
            'à' | 'â' | 'ä' | 'á' => folded.push('a'),
            'é' | 'è' | 'ê' | 'ë' => folded.push('e'),
            'î' | 'ï' | 'í' => folded.push('i'),
            'ô' | 'ö' | 'ó' => folded.push('o'),
            'ù' | 'û' | 'ü' | 'ú' => folded.push('u'),
            'ç' => folded.push('c'),
            'œ' => folded.push_str("oe"),
            '’' => folded.push('\''),
            other => folded.push(other),
        }
    }
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn mentions(haystack: &str, phrases: &[&str]) -> bool {
    phrases
        .iter()
        .any(|phrase| contains_phrase(haystack, phrase))
}

pub fn remote_mode(text: &str) -> RemoteMode {
    let text = fold(text);
    if mentions(&text, &NEGATED_REMOTE) {
        RemoteMode::OnSite
    } else if mentions(&text, &FULL_REMOTE) {
        RemoteMode::FullRemote
    } else if mentions(&text, &HYBRID) || mentions(&text, &REMOTE_GENERIC) {
        RemoteMode::Hybrid
    } else if mentions(&text, &ON_SITE) {
        RemoteMode::OnSite
    } else {
        RemoteMode::Unknown
    }
}

fn content(candidate: &Candidate) -> String {
    format!("{}\n{}", candidate.title, candidate.text)
}

fn is_job_offer(candidate: &Candidate) -> bool {
    candidate.kind.as_str() == JOB_OFFER
}

fn remote_passes(wanted: RemoteFilter, candidate: &Candidate) -> bool {
    if wanted == RemoteFilter::Any || !is_job_offer(candidate) {
        return true;
    }
    matches!(
        (wanted, remote_mode(&content(candidate))),
        (RemoteFilter::FullRemote, RemoteMode::FullRemote)
            | (
                RemoteFilter::RemoteOrHybrid,
                RemoteMode::FullRemote | RemoteMode::Hybrid
            )
    )
}

pub fn contracts_of(candidate: &Candidate) -> Vec<ContractFilter> {
    if !is_job_offer(candidate) {
        return vec![ContractFilter::PublicContract];
    }
    let mut found = Vec::new();
    if let Some(code) = candidate.details.get("contract").and_then(Value::as_str) {
        match code.trim().to_ascii_uppercase().as_str() {
            "LIB" | "FRA" => found.push(ContractFilter::Freelance),
            "CDI" | "CDD" | "MIS" | "SAL" | "SAI" => found.push(ContractFilter::Employee),
            _ => {}
        }
    }
    let text = fold(&content(candidate));
    if mentions(&text, &FREELANCE) && !found.contains(&ContractFilter::Freelance) {
        found.push(ContractFilter::Freelance);
    }
    if mentions(&text, &EMPLOYEE) && !found.contains(&ContractFilter::Employee) {
        found.push(ContractFilter::Employee);
    }
    found
}

fn contract_passes(filters: &SearchFilters, candidate: &Candidate) -> bool {
    if filters.wants_any_contract() {
        return true;
    }
    let found = contracts_of(candidate);
    filters
        .contracts
        .iter()
        .any(|wanted| found.contains(wanted))
}

pub fn countries_of(candidate: &Candidate) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    let mut add = |code: Option<&'static str>| {
        if let Some(code) = code
            && !found.contains(&code)
        {
            found.push(code);
        }
    };
    add(candidate
        .details
        .get("country")
        .and_then(Value::as_str)
        .and_then(alpha2_of_alpha3));
    add(candidate
        .domain
        .as_str()
        .rsplit('.')
        .next()
        .and_then(alpha2_of_tld));
    let text = fold(&content(candidate));
    for country in &COUNTRIES {
        if mentions(&text, country.names) {
            add(Some(country.alpha2));
        }
    }
    found
}

fn country_passes(filters: &SearchFilters, candidate: &Candidate) -> bool {
    if filters.countries.is_empty() {
        return true;
    }
    let found = countries_of(candidate);
    found.is_empty()
        || filters
            .countries
            .iter()
            .any(|wanted| found.contains(&wanted.as_str()))
}

pub fn passes(filters: &SearchFilters, candidate: &Candidate) -> bool {
    remote_passes(filters.remote, candidate)
        && country_passes(filters, candidate)
        && contract_passes(filters, candidate)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;

    use super::*;
    use crate::domain::{
        NormalizedDomain,
        profile::definition::CountryCode,
        values::{Provenance, SignalKind, SourceName},
    };

    fn candidate(host: &str, kind: &str, text: &str, details: Value) -> Candidate {
        Candidate {
            domain: NormalizedDomain::from_host(host).unwrap(),
            source: SourceName::Ats,
            kind: SignalKind::parse(kind).unwrap(),
            title: "Offre".to_owned(),
            text: text.to_owned(),
            provenance: Provenance::parse("https://jobs.test/1").unwrap(),
            observed_at: Utc::now(),
            details,
        }
    }

    fn job(text: &str) -> Candidate {
        candidate("acme.com", "job_offer", text, Value::Null)
    }

    fn remote(remote: RemoteFilter) -> SearchFilters {
        SearchFilters {
            remote,
            ..SearchFilters::default()
        }
    }

    fn contracts(contracts: &[ContractFilter]) -> SearchFilters {
        SearchFilters {
            contracts: contracts.to_vec(),
            ..SearchFilters::default()
        }
    }

    fn countries(codes: &[&str]) -> SearchFilters {
        SearchFilters {
            countries: codes.iter().map(|code| CountryCode::new(*code)).collect(),
            ..SearchFilters::default()
        }
    }

    #[test]
    fn open_filters_keep_everything() {
        assert!(passes(&SearchFilters::default(), &job("Poste sur site")));
        assert!(passes(
            &SearchFilters::default(),
            &candidate("mairie.fr", "public_tender", "Marché", Value::Null)
        ));
    }

    #[test]
    fn remote_modes_are_read_in_french_and_english() {
        let cases = [
            ("Poste en full remote", RemoteMode::FullRemote),
            ("100% télétravail depuis la France", RemoteMode::FullRemote),
            ("Télétravail complet possible", RemoteMode::FullRemote),
            ("Fully remote position", RemoteMode::FullRemote),
            ("Mission entièrement à distance", RemoteMode::FullRemote),
            (
                "Organisation hybride, 2 jours de télétravail",
                RemoteMode::Hybrid,
            ),
            ("Hybrid working, 3 days remote", RemoteMode::Hybrid),
            ("Télétravail partiel", RemoteMode::Hybrid),
            ("Télétravail possible", RemoteMode::Hybrid),
            ("Poste basé sur site à Lyon", RemoteMode::OnSite),
            ("On-site only in Paris", RemoteMode::OnSite),
            ("Présentiel exigé", RemoteMode::OnSite),
            ("Pas de télétravail", RemoteMode::OnSite),
            ("No remote work, sur site", RemoteMode::OnSite),
            ("Ingénieur IAM", RemoteMode::Unknown),
        ];
        for (text, expected) in cases {
            assert_eq!(remote_mode(text), expected, "{text}");
        }
    }

    #[test]
    fn a_full_remote_filter_keeps_only_fully_remote_offers() {
        let filters = remote(RemoteFilter::FullRemote);
        assert!(passes(&filters, &job("Full remote")));
        assert!(!passes(&filters, &job("Hybride, 2 jours de télétravail")));
        assert!(!passes(&filters, &job("Sur site")));
        assert!(!passes(&filters, &job("Ingénieur IAM")));
    }

    #[test]
    fn a_remote_or_hybrid_filter_keeps_remote_and_hybrid_offers() {
        let filters = remote(RemoteFilter::RemoteOrHybrid);
        assert!(passes(&filters, &job("Full remote")));
        assert!(passes(&filters, &job("Hybrid, 2 days remote")));
        assert!(passes(&filters, &job("Télétravail possible")));
        assert!(!passes(&filters, &job("On-site in Lyon")));
        assert!(!passes(&filters, &job("No remote")));
        assert!(!passes(&filters, &job("Ingénieur IAM")));
    }

    #[test]
    fn the_remote_filter_does_not_apply_to_public_notices() {
        let notice = candidate("mairie.fr", "public_tender", "Marché IAM", Value::Null);
        assert!(passes(&remote(RemoteFilter::FullRemote), &notice));
    }

    #[test]
    fn contracts_are_read_from_the_offer_text_and_the_kind() {
        assert_eq!(
            contracts_of(&job("Mission freelance, TJM 600")),
            [ContractFilter::Freelance]
        );
        assert_eq!(
            contracts_of(&job("CDI à Paris")),
            [ContractFilter::Employee]
        );
        assert_eq!(
            contracts_of(&job("Permanent contract in Lyon")),
            [ContractFilter::Employee]
        );
        assert_eq!(contracts_of(&job("Ingénieur IAM")), []);
        assert_eq!(
            contracts_of(&candidate(
                "mairie.fr",
                "public_tender",
                "Marché",
                Value::Null
            )),
            [ContractFilter::PublicContract]
        );
        assert_eq!(
            contracts_of(&candidate(
                "acme.fr",
                "job_offer",
                "Ingénieur",
                json!({"contract": "LIB"})
            )),
            [ContractFilter::Freelance]
        );
        assert_eq!(
            contracts_of(&candidate(
                "acme.fr",
                "job_offer",
                "Ingénieur",
                json!({"contract": "CDD"})
            )),
            [ContractFilter::Employee]
        );
    }

    #[test]
    fn a_contract_filter_keeps_the_listed_types_only() {
        let freelance = contracts(&[ContractFilter::Freelance]);
        assert!(passes(&freelance, &job("Mission freelance")));
        assert!(!passes(&freelance, &job("CDI")));
        assert!(!passes(&freelance, &job("Ingénieur IAM")));
        let both = contracts(&[ContractFilter::Freelance, ContractFilter::Employee]);
        assert!(passes(&both, &job("CDI")));
        assert!(passes(&both, &job("Freelance")));
        let tenders = contracts(&[ContractFilter::PublicContract]);
        assert!(passes(
            &tenders,
            &candidate("mairie.fr", "awarded_contract", "Marché", Value::Null)
        ));
        assert!(!passes(&tenders, &job("CDI")));
        assert!(passes(&contracts(&[ContractFilter::Any]), &job("CDI")));
        assert!(passes(&contracts(&[]), &job("CDI")));
    }

    #[test]
    fn the_country_comes_from_the_provider_detail_the_domain_or_the_text() {
        let from_detail = candidate("mairie.com", "public_tender", "", json!({"country": "FRA"}));
        assert_eq!(countries_of(&from_detail), ["FR"]);
        let from_domain = candidate("acme.de", "job_offer", "Stelle", Value::Null);
        assert_eq!(countries_of(&from_domain), ["DE"]);
        let uk = candidate("acme.co.uk", "job_offer", "Role", Value::Null);
        assert_eq!(countries_of(&uk), ["GB"]);
        let from_text = job("Basé à Bruxelles, Belgique");
        assert_eq!(countries_of(&from_text), ["BE"]);
        let english = job("Based in Germany");
        assert_eq!(countries_of(&english), ["DE"]);
        assert!(countries_of(&job("Ingénieur IAM")).is_empty());
    }

    #[test]
    fn a_country_filter_drops_offers_of_other_countries_and_keeps_unknown_ones() {
        let french = countries(&["FR"]);
        assert!(passes(
            &french,
            &candidate("acme.fr", "job_offer", "Poste", Value::Null)
        ));
        assert!(!passes(
            &french,
            &candidate("acme.de", "job_offer", "Stelle", Value::Null)
        ));
        assert!(passes(&french, &job("Poste en France")));
        assert!(!passes(&french, &job("Role in Germany")));
        assert!(passes(&french, &job("Ingénieur IAM")));
        assert!(passes(&countries(&["FR", "BE"]), &job("Poste en Belgique")));
        assert!(!passes(
            &french,
            &candidate("mairie.com", "public_tender", "", json!({"country": "DEU"}))
        ));
    }

    #[test]
    fn every_filter_must_hold() {
        let filters = SearchFilters {
            remote: RemoteFilter::FullRemote,
            countries: vec![CountryCode::new("FR")],
            contracts: vec![ContractFilter::Freelance],
        };
        let wanted = candidate("acme.fr", "job_offer", "Freelance full remote", Value::Null);
        assert!(passes(&filters, &wanted));
        let on_site = candidate("acme.fr", "job_offer", "Freelance sur site", Value::Null);
        assert!(!passes(&filters, &on_site));
        let abroad = candidate("acme.de", "job_offer", "Freelance full remote", Value::Null);
        assert!(!passes(&filters, &abroad));
    }
}
