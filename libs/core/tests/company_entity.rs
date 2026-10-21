mod support;

use lodestone_core::{
    CompanyDossier, CompanyRepository, DossierState, LeadId, LeadQuery, LeadRepository,
    LodestoneUseCase, OfficialData, ProfileId, Provenance, Signal, SignalId, SignalKind,
    SignalRepository, Siren, SourceName, UnitInterval, Verdict, VerdictFilter,
    infrastructure::{
        company::postgres::PgCompanyRepository, lead::postgres::PgLeadRepository,
        postgres::with_tx, signal::postgres::PgSignalRepository,
    },
};
use sqlx::PgPool;
use support::{at, create_profile, domain, lead_id};

const ACME: &str = "552081317";
const OTHER: &str = "123456782";

fn official(siren: &str, legal_name: &str) -> OfficialData {
    OfficialData {
        siren: Siren::parse(siren).unwrap(),
        legal_name: legal_name.to_owned(),
        naf_code: None,
        naf_label: None,
        headcount_code: None,
        headcount_range: None,
        head_office_city: None,
        head_office_postcode: None,
        head_office_country: None,
        head_office_country_code: None,
        created_on: None,
        active: None,
        category: None,
        legal_nature: None,
        head_office_department: None,
    }
}

async fn dossier(pool: &PgPool, host: &str, official: Option<OfficialData>) {
    let mut dossier = CompanyDossier::pending(domain(host), at(0));
    dossier.state = DossierState::Complete;
    dossier.siren = official.as_ref().map(|data| data.siren.clone());
    dossier.official = official;
    with_tx(pool, async |shared| {
        PgCompanyRepository::new(&shared).save(&dossier).await
    })
    .await
    .unwrap();
}

async fn signal(
    pool: &PgPool,
    lead: LeadId,
    kind: &str,
    value: &str,
    source: SourceName,
    host: &str,
) {
    with_tx(pool, async |shared| {
        PgSignalRepository::new(&shared)
            .record(&Signal {
                id: SignalId(common::generate_uuid_v7()),
                lead_id: lead,
                kind: SignalKind::parse(kind).unwrap(),
                value: Some(value.to_owned()),
                source,
                raw: serde_json::json!({"host": host}),
                provenance: Provenance::parse(&format!("https://{host}/{value}")).unwrap(),
                strength: UnitInterval::new(0.5).unwrap(),
                observed_at: at(1),
                agent: None,
            })
            .await
            .map(|_| ())
    })
    .await
    .unwrap();
}

async fn judged(pool: &PgPool, lead: LeadId, confidence: f64, verdict: &str) {
    sqlx::query("UPDATE leads SET confidence = $2, verdict = $3 WHERE id = $1")
        .bind(lead.0)
        .bind(confidence)
        .bind(verdict)
        .execute(pool)
        .await
        .unwrap();
}

struct World {
    alpha: ProfileId,
    beta: ProfileId,
}

async fn world(pool: &PgPool) -> World {
    let alpha = create_profile(pool, "alpha").await;
    let beta = create_profile(pool, "beta").await;
    dossier(pool, "acme.com", Some(official(ACME, "ACME SAS"))).await;
    dossier(pool, "acme.fr", Some(official(ACME, "ACME SAS"))).await;
    dossier(pool, "other.com", Some(official(OTHER, "OTHER SA"))).await;
    dossier(pool, "solo.com", None).await;
    World { alpha, beta }
}

#[sqlx::test(migrations = "../../migrations")]
async fn dossiers_sharing_a_siren_link_their_leads_to_one_company(pool: PgPool) {
    let world = world(&pool).await;
    let first = lead_id(&pool, world.alpha, "acme.com").await;
    let second = lead_id(&pool, world.alpha, "acme.fr").await;
    let other_profile = lead_id(&pool, world.beta, "acme.com").await;
    lead_id(&pool, world.alpha, "other.com").await;
    let solo = lead_id(&pool, world.alpha, "solo.com").await;
    judged(&pool, second, 80.0, "fiable").await;
    judged(&pool, other_profile, 30.0, "ecarte").await;
    let usecase = LodestoneUseCase::new(pool.clone());

    let detail = usecase.lead_detail(first).await.unwrap();

    let entity = detail.entity.unwrap();
    assert_eq!(entity.siren.as_str(), ACME);
    assert_eq!(entity.legal_name, "ACME SAS");
    let siblings: Vec<_> = detail
        .siblings
        .iter()
        .map(|sibling| {
            (
                sibling.id,
                sibling.domain.as_str().to_owned(),
                sibling.profile_name.as_str().to_owned(),
                sibling.verdict,
                sibling.confidence,
            )
        })
        .collect();
    assert_eq!(
        siblings,
        vec![
            (
                second,
                "acme.fr".to_owned(),
                "alpha".to_owned(),
                Some(Verdict::Reliable),
                Some(80.0)
            ),
            (
                other_profile,
                "acme.com".to_owned(),
                "beta".to_owned(),
                Some(Verdict::Discarded),
                Some(30.0)
            ),
        ]
    );
    let standalone = usecase.lead_detail(solo).await.unwrap();
    assert!(standalone.entity.is_none());
    assert!(standalone.siblings.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_company_takes_the_latest_legal_name_of_its_dossiers(pool: PgPool) {
    let world = world(&pool).await;
    let lead = lead_id(&pool, world.alpha, "acme.com").await;
    dossier(&pool, "acme.fr", Some(official(ACME, "ACME HOLDING"))).await;

    let detail = LodestoneUseCase::new(pool.clone())
        .lead_detail(lead)
        .await
        .unwrap();

    assert_eq!(detail.entity.unwrap().legal_name, "ACME HOLDING");
    let companies: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM companies")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(companies, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn facts_aggregate_the_siblings_of_the_same_profile_only(pool: PgPool) {
    let world = world(&pool).await;
    let first = lead_id(&pool, world.alpha, "acme.com").await;
    let second = lead_id(&pool, world.alpha, "acme.fr").await;
    let other_profile = lead_id(&pool, world.beta, "acme.com").await;
    signal(
        &pool,
        first,
        "product_detected",
        "keycloak",
        SourceName::HttpProbe,
        "sso.acme.com",
    )
    .await;
    signal(
        &pool,
        second,
        "product_detected",
        "Keycloak",
        SourceName::HttpProbe,
        "sso.acme.fr",
    )
    .await;
    signal(
        &pool,
        second,
        "saas_provider",
        "auth0",
        SourceName::DnsCname,
        "login.acme.fr",
    )
    .await;
    signal(
        &pool,
        other_profile,
        "job_offer",
        "devops",
        SourceName::Ats,
        "jobs.acme.com",
    )
    .await;
    signal(
        &pool,
        other_profile,
        "public_tender",
        "iam",
        SourceName::Boamp,
        "jobs.acme.com",
    )
    .await;
    let usecase = LodestoneUseCase::new(pool.clone());

    let facts = usecase.lead_detail(first).await.unwrap().facts;

    assert_eq!(facts.signal_count, 3);
    assert_eq!(facts.independent_signals(), 2);
    assert_eq!(
        facts.sources,
        vec![SourceName::DnsCname, SourceName::HttpProbe]
    );
    assert_eq!(facts.kinds.len(), 2);
    let hosts: Vec<&str> = facts.hosts.iter().map(|fact| fact.host.as_str()).collect();
    assert_eq!(hosts, vec!["login.acme.fr", "sso.acme.com", "sso.acme.fr"]);
    let siblings: Vec<&str> = facts.sibling_domains.iter().map(|d| d.as_str()).collect();
    assert_eq!(siblings, vec!["acme.fr"]);
    let in_other_profile = usecase.lead_detail(other_profile).await.unwrap().facts;
    assert_eq!(in_other_profile.signal_count, 2);
    assert!(in_other_profile.sibling_domains.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_without_a_siren_aggregates_nothing(pool: PgPool) {
    let world = world(&pool).await;
    let solo = lead_id(&pool, world.alpha, "solo.com").await;
    lead_id(&pool, world.alpha, "acme.com").await;
    signal(
        &pool,
        solo,
        "product_detected",
        "keycloak",
        SourceName::HttpProbe,
        "sso.solo.com",
    )
    .await;

    let facts = LodestoneUseCase::new(pool.clone())
        .lead_detail(solo)
        .await
        .unwrap()
        .facts;

    assert_eq!(facts.signal_count, 1);
    assert!(facts.sibling_domains.is_empty());
}

fn grouped() -> LeadQuery {
    LeadQuery {
        limit: 50,
        group_by_company: true,
        ..LeadQuery::default()
    }
}

async fn page(
    pool: &PgPool,
    profile: ProfileId,
    query: LeadQuery,
) -> (Vec<String>, u64, Vec<(String, Vec<String>)>) {
    let page = with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .list_page(profile, &query)
            .await
    })
    .await
    .unwrap();
    let items = page
        .items
        .iter()
        .map(|lead| lead.domain.as_str().to_owned())
        .collect();
    let mut members: Vec<(String, Vec<String>)> = page
        .items
        .iter()
        .filter_map(|lead| {
            page.members.get(&lead.id).map(|others| {
                (
                    lead.domain.as_str().to_owned(),
                    others
                        .iter()
                        .map(|other| other.domain.as_str().to_owned())
                        .collect(),
                )
            })
        })
        .collect();
    members.sort();
    (items, page.total, members)
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_grouped_list_has_one_row_per_company_led_by_the_best_confidence(pool: PgPool) {
    let world = world(&pool).await;
    let first = lead_id(&pool, world.alpha, "acme.com").await;
    let second = lead_id(&pool, world.alpha, "acme.fr").await;
    let third = lead_id(&pool, world.alpha, "other.com").await;
    lead_id(&pool, world.alpha, "solo.com").await;
    lead_id(&pool, world.beta, "acme.com").await;
    judged(&pool, first, 40.0, "a_verifier").await;
    judged(&pool, second, 85.0, "fiable").await;
    judged(&pool, third, 10.0, "ecarte").await;

    let (items, total, members) = page(&pool, world.alpha, grouped()).await;

    assert_eq!(total, 3);
    assert_eq!(items, vec!["acme.fr", "other.com", "solo.com"]);
    assert_eq!(
        members,
        vec![("acme.fr".to_owned(), vec!["acme.com".to_owned()])]
    );
    let (flat, flat_total, flat_members) = page(
        &pool,
        world.alpha,
        LeadQuery {
            group_by_company: false,
            ..grouped()
        },
    )
    .await;
    assert_eq!(flat_total, 4);
    assert_eq!(flat.len(), 4);
    assert!(flat_members.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn grouping_applies_after_the_filters_and_keeps_pagination_per_company(pool: PgPool) {
    let world = world(&pool).await;
    let first = lead_id(&pool, world.alpha, "acme.com").await;
    let second = lead_id(&pool, world.alpha, "acme.fr").await;
    let third = lead_id(&pool, world.alpha, "other.com").await;
    judged(&pool, first, 40.0, "a_verifier").await;
    judged(&pool, second, 85.0, "fiable").await;
    judged(&pool, third, 90.0, "fiable").await;

    let reliable = LeadQuery {
        verdicts: vec![VerdictFilter::Reliable],
        ..grouped()
    };
    let (items, total, members) = page(&pool, world.alpha, reliable).await;
    assert_eq!(total, 2);
    assert_eq!(items, vec!["other.com", "acme.fr"]);
    assert!(members.is_empty());

    let (items, total, members) = page(
        &pool,
        world.alpha,
        LeadQuery {
            limit: 1,
            offset: 1,
            ..grouped()
        },
    )
    .await;
    assert_eq!(total, 2);
    assert_eq!(items, vec!["acme.fr"]);
    assert_eq!(
        members,
        vec![("acme.fr".to_owned(), vec!["acme.com".to_owned()])]
    );
}
