mod support;

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use chrono::{DateTime, Utc};
use lodestone_core::{
    Clock, ContactFinder, ContactMethod, ContactRepository, CreateProfileCommand, EmailAddress,
    FoundAddress, LeadRepository, LodestoneUseCase, NormalizedDomain, ProfileId, ProfileName,
    Provenance, RunRepository, SignalRepository, SourceError, SourceName, UnitInterval,
    ValidatedProfile,
    contacts::ContactRunSettings,
    infrastructure::{
        contact::postgres::PgContactRepository, contact_target::postgres::PgContactTargets,
        lead::postgres::PgLeadRepository, postgres::with_tx, run::postgres::PgRunRepository,
        signal::postgres::PgSignalRepository,
    },
};
use sqlx::PgPool;
use support::{MINIMAL_PROFILE, at, domain, seen_lead};

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

struct LaterClock;

impl Clock for LaterClock {
    fn now(&self) -> DateTime<Utc> {
        at(100 + 7200)
    }
}

struct FakeFinder {
    calls: AtomicUsize,
    rate_limited: bool,
}

impl FakeFinder {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            rate_limited: false,
        }
    }
}

fn found(address: &str, confidence: f64) -> FoundAddress {
    FoundAddress {
        address: EmailAddress::parse(address).unwrap(),
        provenance: Provenance::parse("https://acme.com/mentions-legales").unwrap(),
        method: ContactMethod::PublicPage,
        confidence: UnitInterval::new(confidence).unwrap(),
    }
}

impl ContactFinder for FakeFinder {
    fn name(&self) -> SourceName {
        SourceName::PublicPage
    }

    async fn find(
        &self,
        _domain: &NormalizedDomain,
        _profiles: &[lodestone_core::ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<FoundAddress>, SourceError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.rate_limited {
            return Err(SourceError::RateLimited {
                retry_after: Duration::from_secs(60),
            });
        }
        Ok(vec![
            found("contact@acme.com", 0.8),
            found("security@acme.com", 0.9),
            found("jean.dupont@acme.com", 0.8),
            found("security@agency.example.org", 0.9),
        ])
    }
}

async fn active_profile(pool: &PgPool, name: &str, ranking: &[&str]) -> ProfileId {
    let mut value: serde_json::Value = serde_json::from_str(MINIMAL_PROFILE).unwrap();
    value["contact_ranking"]["preferred_local_parts"] = serde_json::json!(ranking);
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse(name).unwrap(),
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

fn settings() -> ContactRunSettings {
    ContactRunSettings {
        batch_size: 50,
        revisit_interval: Duration::from_secs(3600),
    }
}

async fn contacts_of(pool: &PgPool, profile: ProfileId) -> Vec<String> {
    with_tx(pool, async |shared| {
        let lead = PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain("acme.com"))
            .await?
            .unwrap();
        PgContactRepository::new(&shared)
            .list_for_lead(lead.id)
            .await
    })
    .await
    .unwrap()
    .into_iter()
    .map(|contact| contact.address.to_string())
    .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_crawl_feeds_each_profile_with_its_own_ranking(pool: PgPool) {
    let iam = active_profile(&pool, "iam", &["security", "contact"]).await;
    let shop = active_profile(&pool, "shop", &["contact"]).await;
    seen_lead(&pool, iam, "www.acme.com").await;
    seen_lead(&pool, shop, "acme.com").await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let targets = PgContactTargets::new(pool.clone());
    let finder = FakeFinder::new();

    let report = usecase
        .run_contact_finder(&finder, &targets, settings(), &FixedClock)
        .await
        .unwrap();

    assert_eq!(finder.calls.load(Ordering::SeqCst), 1);
    assert_eq!(report.domains, 1);
    assert_eq!(report.contacts, 3);
    let mut iam_contacts = contacts_of(&pool, iam).await;
    iam_contacts.sort();
    assert_eq!(iam_contacts, ["contact@acme.com", "security@acme.com"]);
    assert_eq!(contacts_of(&pool, shop).await, ["contact@acme.com"]);

    let signals = with_tx(&pool, async |shared| {
        let lead = PgLeadRepository::new(&shared)
            .find_by_domain(shop, &domain("acme.com"))
            .await?
            .unwrap();
        PgSignalRepository::new(&shared)
            .list_for_lead(lead.id)
            .await
    })
    .await
    .unwrap();
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].kind.as_str(), "role_contact_found");
    assert_eq!(signals[0].value.as_deref(), Some("contact"));

    let runs = with_tx(&pool, async |shared| {
        PgRunRepository::new(&shared).list_recent(10).await
    })
    .await
    .unwrap();
    assert_eq!(runs.len(), 2);
    assert!(
        runs.iter()
            .all(|run| run.source == SourceName::PublicPage && run.volume == 1)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn checked_domains_wait_for_the_revisit_interval_and_never_duplicate(pool: PgPool) {
    let iam = active_profile(&pool, "iam", &["security", "contact"]).await;
    seen_lead(&pool, iam, "acme.com").await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let targets = PgContactTargets::new(pool.clone());
    let finder = FakeFinder::new();

    usecase
        .run_contact_finder(&finder, &targets, settings(), &FixedClock)
        .await
        .unwrap();
    let second = usecase
        .run_contact_finder(&finder, &targets, settings(), &FixedClock)
        .await
        .unwrap();
    assert_eq!(second.domains, 0);
    assert_eq!(finder.calls.load(Ordering::SeqCst), 1);

    let third = usecase
        .run_contact_finder(&finder, &targets, settings(), &LaterClock)
        .await
        .unwrap();
    assert_eq!(third.domains, 1);
    assert_eq!(third.contacts, 0);
    assert_eq!(contacts_of(&pool, iam).await.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rate_limited_domains_are_not_marked(pool: PgPool) {
    let iam = active_profile(&pool, "iam", &["security"]).await;
    seen_lead(&pool, iam, "acme.com").await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let targets = PgContactTargets::new(pool.clone());
    let finder = FakeFinder {
        rate_limited: true,
        ..FakeFinder::new()
    };

    let report = usecase
        .run_contact_finder(&finder, &targets, settings(), &FixedClock)
        .await
        .unwrap();
    assert!(report.rate_limited);
    assert_eq!(report.domains, 0);

    let healthy = FakeFinder::new();
    let retry = usecase
        .run_contact_finder(&healthy, &targets, settings(), &FixedClock)
        .await
        .unwrap();
    assert_eq!(retry.domains, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn inactive_profiles_get_nothing(pool: PgPool) {
    support::create_profile(&pool, "dormant").await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let targets = PgContactTargets::new(pool.clone());
    let finder = FakeFinder::new();
    let report = usecase
        .run_contact_finder(&finder, &targets, settings(), &FixedClock)
        .await
        .unwrap();
    assert_eq!(report.domains, 0);
    assert_eq!(finder.calls.load(Ordering::SeqCst), 0);
}
