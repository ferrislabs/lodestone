mod support;

use std::time::Duration;

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_core::{
    ActiveProfile, CandidateHost, CandidateHosts, Clock, ContactFinder, ContactMethod,
    ContactRepository, ContactTargets, EmailAddress, FoundAddress, HostOrigin, Hostname,
    LeadRepository, LodestoneUseCase, NormalizedDomain, Observation, ProfileId, Provenance,
    SignalKind, Source, SourceError, SourceName, SuppressionTarget, UnitInterval,
    contacts::ContactRunSettings,
    definition::Agent,
    infrastructure::{
        candidate_host::postgres::PgCandidateHosts, contact::postgres::PgContactRepository,
        contact_target::postgres::PgContactTargets, lead::postgres::PgLeadRepository,
        postgres::with_tx,
    },
};
use serde_json::json;
use sqlx::PgPool;
use support::{at, create_profile, domain, seen_lead};

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

async fn two_active_profiles(pool: &PgPool) -> (ProfileId, ProfileId) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let alpha = create_profile(pool, "alpha").await;
    let beta = create_profile(pool, "beta").await;
    usecase.set_profile_active(alpha, true).await.unwrap();
    usecase.set_profile_active(beta, true).await.unwrap();
    (alpha, beta)
}

async fn suppress(pool: &PgPool, target: SuppressionTarget) {
    LodestoneUseCase::new(pool.clone())
        .add_suppression(target, Some("asked to be left alone".to_owned()), None)
        .await
        .unwrap();
}

async fn lead_domains(pool: &PgPool, profile: ProfileId) -> Vec<String> {
    with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .list_by_profile(profile)
            .await
    })
    .await
    .unwrap()
    .into_iter()
    .map(|lead| lead.domain.to_string())
    .collect()
}

struct HostsSource;

impl Source for HostsSource {
    type Item = &'static str;

    fn name(&self) -> SourceName {
        SourceName::HttpProbe
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Self::Item>, SourceError> {
        Ok(vec!["sso.acme.com", "auth.optout.io"])
    }

    fn evaluate(
        &self,
        item: &Self::Item,
        _profile: &ActiveProfile,
        _agent: &Agent,
    ) -> Vec<Observation> {
        vec![Observation {
            domain: domain(item),
            kind: SignalKind::parse("product_detected").unwrap(),
            value: Some("keycloak".to_owned()),
            raw: json!({ "host": item }),
            provenance: Provenance::parse(&format!("https://{item}/realms/master")).unwrap(),
            strength: UnitInterval::new(0.9).unwrap(),
            observed_at: at(50),
            agent: None,
        }]
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn ingestion_never_creates_a_lead_for_a_suppressed_domain(pool: PgPool) {
    let (alpha, beta) = two_active_profiles(&pool).await;
    suppress(&pool, SuppressionTarget::Domain(domain("optout.io"))).await;

    LodestoneUseCase::new(pool.clone())
        .run_source(&HostsSource, &FixedClock)
        .await
        .unwrap();

    assert_eq!(lead_domains(&pool, alpha).await, ["acme.com"]);
    assert_eq!(lead_domains(&pool, beta).await, ["acme.com"]);
}

fn candidate(host: &str) -> CandidateHost {
    CandidateHost::discovered(Hostname::parse(host).unwrap(), HostOrigin::Import, at(0)).unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn suppressed_domains_are_not_stored_or_probed_as_candidate_hosts(pool: PgPool) {
    suppress(&pool, SuppressionTarget::Domain(domain("optout.io"))).await;
    let hosts = PgCandidateHosts::new(pool.clone());
    hosts
        .upsert(&[candidate("sso.acme.com"), candidate("auth.optout.io")])
        .await
        .unwrap();
    let due = hosts
        .list_due(SourceName::HttpProbe, at(10), 10)
        .await
        .unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].host.as_str(), "sso.acme.com");

    suppress(&pool, SuppressionTarget::Domain(domain("acme.com"))).await;
    assert!(
        hosts
            .list_due(SourceName::HttpProbe, at(10), 10)
            .await
            .unwrap()
            .is_empty()
    );
}

struct PageFinder;

impl ContactFinder for PageFinder {
    fn name(&self) -> SourceName {
        SourceName::PublicPage
    }

    async fn find(
        &self,
        domain: &NormalizedDomain,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<FoundAddress>, SourceError> {
        Ok(["security", "contact"]
            .into_iter()
            .map(|local| FoundAddress {
                address: EmailAddress::parse(&format!("{local}@{}", domain.as_str())).unwrap(),
                provenance: Provenance::parse(&format!("https://{}/contact", domain.as_str()))
                    .unwrap(),
                method: ContactMethod::PublicPage,
                confidence: UnitInterval::new(0.8).unwrap(),
            })
            .collect())
    }
}

async fn contacts_of(pool: &PgPool, profile: ProfileId, raw: &str) -> Vec<String> {
    let raw = raw.to_owned();
    with_tx(pool, async |shared| {
        let lead = PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain(&raw))
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
async fn suppressed_addresses_and_domains_get_no_contact_in_any_profile(pool: PgPool) {
    let (alpha, beta) = two_active_profiles(&pool).await;
    for profile in [alpha, beta] {
        seen_lead(&pool, profile, "acme.com").await;
        seen_lead(&pool, profile, "optout.io").await;
    }
    suppress(
        &pool,
        SuppressionTarget::Email(EmailAddress::parse("security@acme.com").unwrap()),
    )
    .await;
    let targets = PgContactTargets::new(pool.clone());

    LodestoneUseCase::new(pool.clone())
        .run_contact_finder(
            &PageFinder,
            &targets,
            ContactRunSettings {
                batch_size: 10,
                revisit_interval: Duration::from_secs(3600),
            },
            &FixedClock,
        )
        .await
        .unwrap();

    for profile in [alpha, beta] {
        assert_eq!(
            contacts_of(&pool, profile, "acme.com").await,
            ["contact@acme.com"]
        );
    }

    suppress(&pool, SuppressionTarget::Domain(domain("optout.io"))).await;
    let due = targets
        .list_due(SourceName::SecurityTxt, at(1000), 10, None)
        .await
        .unwrap();
    assert_eq!(due, [domain("acme.com")]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn guard_refuses_suppressed_targets_for_later_steps(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    suppress(&pool, SuppressionTarget::Domain(domain("optout.io"))).await;
    suppress(
        &pool,
        SuppressionTarget::Email(EmailAddress::parse("dpo@acme.com").unwrap()),
    )
    .await;
    let email = |raw: &str| EmailAddress::parse(raw).unwrap();

    assert!(matches!(
        usecase
            .ensure_not_suppressed(&domain("optout.io"), None)
            .await,
        Err(CoreError::Forbidden { .. })
    ));
    assert!(matches!(
        usecase
            .ensure_not_suppressed(&domain("acme.com"), Some(&email("dpo@acme.com")))
            .await,
        Err(CoreError::Forbidden { .. })
    ));
    assert!(matches!(
        usecase
            .ensure_not_suppressed(&domain("acme.com"), Some(&email("it@sub.optout.io")))
            .await,
        Err(CoreError::Forbidden { .. })
    ));
    assert!(
        usecase
            .ensure_not_suppressed(&domain("acme.com"), Some(&email("contact@acme.com")))
            .await
            .is_ok()
    );

    let entries = usecase.list_suppressions().await.unwrap();
    assert_eq!(entries.len(), 2);
    usecase.remove_suppression(entries[0].id).await.unwrap();
    assert!(matches!(
        usecase.remove_suppression(entries[0].id).await,
        Err(CoreError::NotFound)
    ));
}
