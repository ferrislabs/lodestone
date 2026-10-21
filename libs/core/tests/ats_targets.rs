mod support;

use chrono::{DateTime, Utc};
use lodestone_core::{
    AtsTargets, CreateProfileCommand, LeadId, LodestoneUseCase, ProfileId, Provenance, Signal,
    SignalId, SignalKind, SignalRepository, SourceName, SuppressionTarget, UnitInterval,
    ValidatedProfile,
    infrastructure::{
        contact_target::postgres::PgAtsTargets, postgres::with_tx,
        signal::postgres::PgSignalRepository,
    },
};
use serde_json::json;
use sqlx::{PgPool, query_scalar};
use support::{at, domain, seen_lead};

fn definition(with_ats: bool) -> serde_json::Value {
    let mut value = json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": {"boamp": {"keywords": ["sso"]}},
        "scoring": {"factors": [{"signal": "job_offer", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
    });
    if with_ats {
        value["sources"]["ats"] = json!({"keywords": ["keycloak"]});
    }
    value
}

async fn profile(pool: &PgPool, name: &str, with_ats: bool, active: bool) -> ProfileId {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: lodestone_core::ProfileName::parse(name).unwrap(),
            definition: ValidatedProfile::parse(definition(with_ats)).unwrap(),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    usecase.set_profile_active(id, active).await.unwrap();
    id
}

async fn signal(pool: &PgPool, lead: LeadId, source: SourceName, kind: &str) {
    with_tx(pool, async |shared| {
        PgSignalRepository::new(&shared)
            .record(&Signal {
                id: SignalId(common::generate_uuid_v7()),
                lead_id: lead,
                kind: SignalKind::parse(kind).unwrap(),
                value: Some("x".to_owned()),
                source,
                raw: json!({}),
                provenance: Provenance::parse("https://example.com/x").unwrap(),
                strength: UnitInterval::new(0.5).unwrap(),
                observed_at: at(1),
                agent: None,
            })
            .await
    })
    .await
    .unwrap();
}

async fn due(targets: &PgAtsTargets, profiles: &[ProfileId], at_seconds: i64) -> Vec<String> {
    targets
        .list_due(profiles, at(at_seconds), 50)
        .await
        .unwrap()
        .iter()
        .map(|d| d.as_str().to_owned())
        .collect()
}

async fn company_jobs_ran_at(pool: &PgPool, lead: LeadId) -> Option<DateTime<Utc>> {
    query_scalar::<_, Option<DateTime<Utc>>>(
        "SELECT ran_at FROM lead_enrichments WHERE lead_id = $1 AND step = 'company_jobs'",
    )
    .bind(lead.0)
    .fetch_optional(pool)
    .await
    .unwrap()
    .flatten()
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_lead_of_the_enriched_profiles_is_due_whatever_its_source(pool: PgPool) {
    let with_ats = profile(&pool, "with-ats", true, true).await;
    let without_ats = profile(&pool, "without-ats", false, true).await;
    for (host, source, kind) in [
        (
            "ct.fr",
            SourceName::CertificateTransparency,
            "certificate_seen",
        ),
        ("probe.fr", SourceName::HttpProbe, "product_detected"),
        ("dns.fr", SourceName::DnsCname, "saas_provider"),
        ("tender.fr", SourceName::Boamp, "public_tender"),
        ("offer.fr", SourceName::HackerNewsHiring, "job_offer"),
    ] {
        let lead = seen_lead(&pool, with_ats, host).await;
        signal(&pool, lead.id, source, kind).await;
    }
    let other = seen_lead(&pool, without_ats, "noats.fr").await;
    signal(&pool, other.id, SourceName::Boamp, "public_tender").await;

    let targets = PgAtsTargets::new(pool.clone());
    assert_eq!(
        due(&targets, &[with_ats], 10).await,
        ["ct.fr", "dns.fr", "offer.fr", "probe.fr", "tender.fr"]
    );
    assert!(due(&targets, &[], 10).await.is_empty());
    assert_eq!(due(&targets, &[with_ats, without_ats], 10).await.len(), 6);
}

#[sqlx::test(migrations = "../../migrations")]
async fn checked_and_suppressed_domains_are_not_due(pool: PgPool) {
    let id = profile(&pool, "with-ats", true, true).await;
    for host in ["a.fr", "b.fr", "optout.io"] {
        let lead = seen_lead(&pool, id, host).await;
        signal(&pool, lead.id, SourceName::Boamp, "public_tender").await;
    }
    LodestoneUseCase::new(pool.clone())
        .add_suppression(SuppressionTarget::Domain(domain("optout.io")), None, None)
        .await
        .unwrap();
    let targets = PgAtsTargets::new(pool.clone());
    targets
        .mark_checked(&[domain("a.fr")], at(5))
        .await
        .unwrap();

    let fresh = targets.list_due(&[id], at(4), 50).await.unwrap();
    assert_eq!(fresh, [domain("b.fr")]);
    let stale = targets.list_due(&[id], at(6), 50).await.unwrap();
    assert_eq!(stale, [domain("b.fr"), domain("a.fr")]);
    let limited = targets.list_due(&[id], at(6), 1).await.unwrap();
    assert_eq!(limited, [domain("b.fr")]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_scan_completes_the_company_jobs_step_of_every_lead_of_the_domain(pool: PgPool) {
    let first = profile(&pool, "first", true, true).await;
    let second = profile(&pool, "second", true, true).await;
    let early = seen_lead(&pool, first, "acme.fr").await;
    signal(&pool, early.id, SourceName::Boamp, "public_tender").await;
    let targets = PgAtsTargets::new(pool.clone());
    assert_eq!(company_jobs_ran_at(&pool, early.id).await, None);

    targets
        .mark_checked(&[domain("acme.fr")], at(5))
        .await
        .unwrap();
    assert_eq!(company_jobs_ran_at(&pool, early.id).await, Some(at(5)));
    assert!(due(&targets, &[first, second], 4).await.is_empty());

    let late = seen_lead(&pool, second, "acme.fr").await;
    signal(&pool, late.id, SourceName::Boamp, "public_tender").await;
    assert!(due(&targets, &[first], 4).await.is_empty());
    assert_eq!(due(&targets, &[first, second], 4).await, ["acme.fr"]);
    targets
        .mark_checked(&[domain("acme.fr")], at(7))
        .await
        .unwrap();
    assert_eq!(company_jobs_ran_at(&pool, late.id).await, Some(at(7)));
    assert_eq!(company_jobs_ran_at(&pool, early.id).await, Some(at(7)));
}
