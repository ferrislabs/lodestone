use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::TimeZone;
use lodestone_core::{
    Profile, ProfileId, ProfileName, ProfileRevision, RevisionNumber, ValidatedProfile,
};
use serde_json::json;
use uuid::Uuid;

use super::*;

struct StubMx {
    answer: Result<bool, DnsError>,
    calls: AtomicUsize,
}

impl MxLookup for StubMx {
    async fn has_mx(&self, _domain: &NormalizedDomain) -> Result<bool, DnsError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.answer.clone()
    }
}

fn stub(answer: Result<bool, DnsError>) -> Arc<StubMx> {
    Arc::new(StubMx {
        answer,
        calls: AtomicUsize::new(0),
    })
}

fn profile(guesses: &[&str]) -> ActiveProfile {
    let ranked: Vec<&str> = ["security", "contact", "info"].into_iter().collect();
    let value = json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": {"dns_cname": {"patterns": [{"provider": "auth0", "target_suffix": "auth0.com"}]}},
        "scoring": {"factors": [{"signal": "saas_provider", "weight": 1.0}]},
        "contact_ranking": {"preferred_local_parts": ranked, "guess_local_parts": guesses},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
    });
    let id = ProfileId(Uuid::new_v4());
    let at = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
    ActiveProfile {
        profile: Profile {
            id,
            name: ProfileName::parse("p").unwrap(),
            active: true,
            created_at: at,
            updated_at: at,
        },
        revision: ProfileRevision {
            profile_id: id,
            number: RevisionNumber::new(1).unwrap(),
            definition: ValidatedProfile::parse(value).unwrap(),
            created_by: None,
            created_at: at,
        },
    }
}

fn domain() -> NormalizedDomain {
    NormalizedDomain::from_host("acme.fr").unwrap()
}

fn now() -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000, 0).unwrap()
}

#[tokio::test]
async fn guesses_the_union_of_profile_parts_when_mx_exists() {
    let finder = RoleGuessFinder::new(stub(Ok(true)));
    let found = finder
        .find(
            &domain(),
            &[profile(&["contact"]), profile(&["info", "contact"])],
            now(),
        )
        .await
        .unwrap();
    let addresses: Vec<String> = found.iter().map(|f| f.address.to_string()).collect();
    assert_eq!(addresses, ["contact@acme.fr", "info@acme.fr"]);
    assert!(found.iter().all(|f| f.method == ContactMethod::RoleGuess));
    assert!(found.iter().all(|f| f.confidence.get() == GUESS_CONFIDENCE));
    assert_eq!(found[0].provenance.as_str(), "dns:acme.fr?type=MX");
}

#[tokio::test]
async fn no_mx_means_no_guess() {
    let finder = RoleGuessFinder::new(stub(Ok(false)));
    assert!(
        finder
            .find(&domain(), &[profile(&["contact"])], now())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn profiles_without_guesses_skip_the_dns_lookup() {
    let mx = stub(Ok(true));
    let finder = RoleGuessFinder::new(Arc::clone(&mx));
    assert!(
        finder
            .find(&domain(), &[profile(&[])], now())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(mx.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn dns_failure_is_an_upstream_error() {
    let finder = RoleGuessFinder::new(stub(Err(DnsError::Timeout)));
    assert!(matches!(
        finder
            .find(&domain(), &[profile(&["contact"])], now())
            .await,
        Err(SourceError::Upstream(_))
    ));
}

#[test]
fn guessing_runs_only_after_the_page_crawl() {
    let finder = RoleGuessFinder::new(stub(Ok(true)));
    assert_eq!(finder.requires(), Some(SourceName::PublicPage));
    assert_eq!(finder.name(), SourceName::RoleGuess);
}
