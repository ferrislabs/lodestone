pub mod recipient;

use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    NormalizedDomain,
    lead::ContactMethod,
    profile::definition::ContactRanking,
    source::SourceError,
    values::{EmailAddress, Provenance, SourceName, UnitInterval},
};

#[derive(Debug, Clone, PartialEq)]
pub struct FoundAddress {
    pub address: EmailAddress,
    pub provenance: Provenance,
    pub method: ContactMethod,
    pub confidence: UnitInterval,
}

pub trait ContactFinder: Send + Sync {
    fn name(&self) -> SourceName;

    fn find(
        &self,
        domain: &NormalizedDomain,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<FoundAddress>, SourceError>> + Send;
}

pub trait ContactTargets: Send + Sync {
    fn list_due(
        &self,
        source: SourceName,
        checked_before: DateTime<Utc>,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<NormalizedDomain>, CoreError>> + Send;

    fn mark_checked(
        &self,
        domains: &[NormalizedDomain],
        source: SourceName,
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;
}

fn belongs_to(address: &EmailAddress, domain: &NormalizedDomain) -> bool {
    let host = address.domain();
    host == domain.as_str()
        || host
            .strip_suffix(domain.as_str())
            .is_some_and(|prefix| prefix.ends_with('.'))
}

pub fn rank_of(address: &EmailAddress, ranking: &ContactRanking) -> Option<usize> {
    let local = address.local_part();
    ranking
        .preferred_local_parts
        .iter()
        .position(|preferred| preferred.eq_ignore_ascii_case(local))
}

pub fn select_for_profile<'a>(
    found: &'a [FoundAddress],
    domain: &NormalizedDomain,
    ranking: &ContactRanking,
) -> Vec<&'a FoundAddress> {
    let mut selected: Vec<&FoundAddress> = Vec::with_capacity(found.len());
    for candidate in found {
        let ranked = rank_of(&candidate.address, ranking).is_some();
        let hint = candidate.method == ContactMethod::SecurityTxt;
        if !belongs_to(&candidate.address, domain) || !(ranked || hint) {
            continue;
        }
        match selected
            .iter_mut()
            .find(|kept| kept.address == candidate.address)
        {
            Some(kept) if kept.confidence < candidate.confidence => *kept = candidate,
            Some(_) => {}
            None => selected.push(candidate),
        }
    }
    selected.sort_by(|a, b| {
        let rank = |found: &FoundAddress| rank_of(&found.address, ranking).unwrap_or(usize::MAX);
        rank(a).cmp(&rank(b)).then_with(|| {
            b.confidence
                .get()
                .partial_cmp(&a.confidence.get())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(address: &str, confidence: f64) -> FoundAddress {
        FoundAddress {
            address: EmailAddress::parse(address).unwrap(),
            provenance: Provenance::parse("https://example.com/contact").unwrap(),
            method: ContactMethod::PublicPage,
            confidence: UnitInterval::new(confidence).unwrap(),
        }
    }

    fn ranking(parts: &[&str]) -> ContactRanking {
        ContactRanking {
            preferred_local_parts: parts.iter().map(|part| (*part).to_owned()).collect(),
            guess_local_parts: Vec::new(),
        }
    }

    fn domain() -> NormalizedDomain {
        NormalizedDomain::from_host("example.com").unwrap()
    }

    fn addresses(selected: &[&FoundAddress]) -> Vec<String> {
        selected.iter().map(|f| f.address.to_string()).collect()
    }

    #[test]
    fn keeps_only_listed_role_addresses_of_the_lead_domain() {
        let found = [
            found("info@example.com", 0.8),
            found("jean.dupont@example.com", 0.8),
            found("security@example.com", 0.8),
            found("contact@agency-partner.fr", 0.8),
            found("it@mail.example.com", 0.8),
            found("security@notexample.com", 0.8),
        ];
        let selected = select_for_profile(&found, &domain(), &ranking(&["security", "it", "info"]));
        assert_eq!(
            addresses(&selected),
            [
                "security@example.com",
                "it@mail.example.com",
                "info@example.com"
            ]
        );
    }

    #[test]
    fn ranking_follows_the_profile_order() {
        let found = [
            found("contact@example.com", 0.9),
            found("security@example.com", 0.5),
        ];
        let iam = select_for_profile(&found, &domain(), &ranking(&["security", "contact"]));
        let shop = select_for_profile(&found, &domain(), &ranking(&["contact", "security"]));
        assert_eq!(
            addresses(&iam),
            ["security@example.com", "contact@example.com"]
        );
        assert_eq!(
            addresses(&shop),
            ["contact@example.com", "security@example.com"]
        );
    }

    #[test]
    fn duplicates_keep_the_highest_confidence() {
        let found = [
            found("contact@example.com", 0.3),
            found("contact@example.com", 0.9),
        ];
        let selected = select_for_profile(&found, &domain(), &ranking(&["contact"]));
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].confidence.get(), 0.9);
    }

    #[test]
    fn security_txt_hints_on_the_lead_domain_are_kept_unranked_and_last() {
        let hint = FoundAddress {
            method: ContactMethod::SecurityTxt,
            ..found("psirt@example.com", 0.7)
        };
        let foreign_hint = FoundAddress {
            method: ContactMethod::SecurityTxt,
            ..found("security@bugbounty-platform.example.org", 0.7)
        };
        let found = [hint, foreign_hint, found("contact@example.com", 0.8)];
        let selected = select_for_profile(&found, &domain(), &ranking(&["contact"]));
        assert_eq!(
            addresses(&selected),
            ["contact@example.com", "psirt@example.com"]
        );
    }

    #[test]
    fn empty_ranking_keeps_nothing() {
        let found = [found("contact@example.com", 0.9)];
        assert!(select_for_profile(&found, &domain(), &ranking(&[])).is_empty());
    }
}
