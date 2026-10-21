mod support;

use lodestone_core::{
    AtsTargets, CreateProfileCommand, LodestoneUseCase, Provenance, Signal, SignalId, SignalKind,
    SignalRepository, SourceName, SuppressionTarget, UnitInterval, ValidatedProfile,
    infrastructure::{
        contact_target::postgres::PgAtsTargets, postgres::with_tx,
        signal::postgres::PgSignalRepository,
    },
};
use serde_json::json;
use sqlx::PgPool;
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

async fn profile(
    pool: &PgPool,
    name: &str,
    with_ats: bool,
    active: bool,
) -> lodestone_core::ProfileId {
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

async fn signal(pool: &PgPool, lead: lodestone_core::LeadId, source: SourceName, kind: &str) {
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

#[sqlx::test(migrations = "../../migrations")]
async fn only_passively_detected_leads_of_ats_profiles_are_due(pool: PgPool) {
    let with_ats = profile(&pool, "with-ats", true, true).await;
    let without_ats = profile(&pool, "without-ats", false, true).await;
    let inactive = profile(&pool, "inactive", true, false).await;
    for (host, source, kind) in [
        (
            "ct.fr",
            SourceName::CertificateTransparency,
            "saas_provider",
        ),
        ("probe.fr", SourceName::HttpProbe, "product_detected"),
        ("dns.fr", SourceName::DnsCname, "saas_provider"),
        ("tender.fr", SourceName::Boamp, "public_tender"),
    ] {
        let lead = seen_lead(&pool, with_ats, host).await;
        signal(&pool, lead.id, source, kind).await;
    }
    seen_lead(&pool, with_ats, "bare.fr").await;
    let other = seen_lead(&pool, without_ats, "noats.fr").await;
    signal(&pool, other.id, SourceName::HttpProbe, "product_detected").await;
    let sleeping = seen_lead(&pool, inactive, "inactive.fr").await;
    signal(
        &pool,
        sleeping.id,
        SourceName::HttpProbe,
        "product_detected",
    )
    .await;

    let targets = PgAtsTargets::new(pool.clone());
    let due: Vec<String> = targets
        .list_due(at(10), 50)
        .await
        .unwrap()
        .iter()
        .map(|d| d.as_str().to_owned())
        .collect();
    assert_eq!(due, ["ct.fr", "dns.fr", "probe.fr"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn checked_and_suppressed_domains_are_not_due(pool: PgPool) {
    let id = profile(&pool, "with-ats", true, true).await;
    for host in ["a.fr", "b.fr", "optout.io"] {
        let lead = seen_lead(&pool, id, host).await;
        signal(&pool, lead.id, SourceName::HttpProbe, "product_detected").await;
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

    let fresh = targets.list_due(at(4), 50).await.unwrap();
    assert_eq!(fresh, [domain("b.fr")]);
    let stale = targets.list_due(at(6), 50).await.unwrap();
    assert_eq!(stale, [domain("b.fr"), domain("a.fr")]);
    let limited = targets.list_due(at(6), 1).await.unwrap();
    assert_eq!(limited, [domain("b.fr")]);
}
