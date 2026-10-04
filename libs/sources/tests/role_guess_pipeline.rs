use std::{sync::Arc, time::Duration};

use chrono::{DateTime, TimeZone, Utc};
use lodestone_core::{
    Clock, ContactEmail, ContactId, ContactKind, ContactMethod, ContactRepository, ContactTargets,
    CreateProfileCommand, DnsError, EmailAddress, LeadRepository, LodestoneUseCase, MxLookup,
    NormalizedDomain, ProfileId, ProfileName, Provenance, SourceName, UnitInterval,
    ValidatedProfile,
    contacts::ContactRunSettings,
    infrastructure::{
        contact::postgres::PgContactRepository, contact_target::postgres::PgContactTargets,
        lead::postgres::PgLeadRepository, postgres::with_tx,
    },
};
use serde_json::json;
use sources::role_guess::RoleGuessFinder;
use sqlx::PgPool;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

struct StubMx;

impl MxLookup for StubMx {
    async fn has_mx(&self, domain: &NormalizedDomain) -> Result<bool, DnsError> {
        Ok(domain.as_str() != "nomail.fr")
    }
}

fn domain(raw: &str) -> NormalizedDomain {
    NormalizedDomain::from_host(raw).unwrap()
}

async fn active_profile(pool: &PgPool) -> ProfileId {
    let value = json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": {"dns_cname": {"patterns": [{"provider": "auth0", "target_suffix": "auth0.com"}]}},
        "scoring": {"factors": [{"signal": "saas_provider", "weight": 1.0}]},
        "contact_ranking": {
            "preferred_local_parts": ["security", "contact", "info"],
            "guess_local_parts": ["contact", "info"]
        },
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
    });
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse("iam").unwrap(),
            definition: ValidatedProfile::parse(value).unwrap(),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    usecase.set_profile_active(id, true).await.unwrap();
    id
}

async fn lead(pool: &PgPool, profile: ProfileId, raw: &str) -> lodestone_core::LeadId {
    let raw = raw.to_owned();
    with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .record_seen(profile, &domain(&raw), at(0))
            .await
    })
    .await
    .unwrap()
    .id
}

async fn contacts(pool: &PgPool, lead: lodestone_core::LeadId) -> Vec<(String, ContactMethod)> {
    with_tx(pool, async |shared| {
        PgContactRepository::new(&shared).list_for_lead(lead).await
    })
    .await
    .unwrap()
    .into_iter()
    .map(|contact| (contact.address.to_string(), contact.method))
    .collect()
}

fn settings() -> ContactRunSettings {
    ContactRunSettings {
        batch_size: 50,
        revisit_interval: Duration::from_secs(3600),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn guesses_only_crawled_domains_with_mx_and_no_real_address(pool: PgPool) {
    let profile = active_profile(&pool).await;
    let empty = lead(&pool, profile, "acme.fr").await;
    let known = lead(&pool, profile, "known.fr").await;
    let no_mail = lead(&pool, profile, "nomail.fr").await;
    let not_crawled = lead(&pool, profile, "later.fr").await;
    let targets = PgContactTargets::new(pool.clone());
    targets
        .mark_checked(
            &[domain("acme.fr"), domain("known.fr"), domain("nomail.fr")],
            SourceName::PublicPage,
            at(50),
        )
        .await
        .unwrap();
    with_tx(&pool, async |shared| {
        PgContactRepository::new(&shared)
            .insert(&ContactEmail {
                id: ContactId(common::generate_uuid_v7()),
                lead_id: known,
                address: EmailAddress::parse("security@known.fr").unwrap(),
                provenance: Provenance::parse("https://known.fr/contact").unwrap(),
                method: ContactMethod::PublicPage,
                confidence: UnitInterval::new(0.9).unwrap(),
                kind: ContactKind::Role,
                found_at: at(50),
            })
            .await
    })
    .await
    .unwrap();

    let finder = RoleGuessFinder::new(Arc::new(StubMx));
    let report = LodestoneUseCase::new(pool.clone())
        .run_contact_finder(&finder, &targets, settings(), &FixedClock)
        .await
        .unwrap();

    assert_eq!(report.domains, 3);
    assert_eq!(report.contacts, 2);
    let mut guessed = contacts(&pool, empty).await;
    guessed.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        guessed,
        [
            ("contact@acme.fr".to_owned(), ContactMethod::RoleGuess),
            ("info@acme.fr".to_owned(), ContactMethod::RoleGuess)
        ]
    );
    assert_eq!(
        contacts(&pool, known).await,
        [("security@known.fr".to_owned(), ContactMethod::PublicPage)]
    );
    assert!(contacts(&pool, no_mail).await.is_empty());
    assert!(contacts(&pool, not_crawled).await.is_empty());
}
