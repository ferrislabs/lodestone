mod support;

use lodestone_core::{
    Assessment, AssessmentId, AssessmentRepository, Evidence, Provenance, RevisionNumber,
    infrastructure::{assessment::postgres::PgAssessmentRepository, postgres::with_tx},
};
use sqlx::PgPool;
use support::{at, create_profile, lead_id};

fn assessment(lead: lodestone_core::LeadId, created: i64, evidence: Vec<Evidence>) -> Assessment {
    Assessment {
        id: AssessmentId(common::generate_uuid_v7()),
        lead_id: lead,
        profile_revision: RevisionNumber::new(1).unwrap(),
        summary: "Runs Keycloak 21".to_owned(),
        verdict_narrative: "Reliable".to_owned(),
        approach_angle: "Offer a managed migration".to_owned(),
        evidence,
        evidence_verified: true,
        model: "model-x".to_owned(),
        prompt_version: "v1".to_owned(),
        draft: None,
        draft_review: None,
        created_at: at(created),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn evidence_round_trips(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead_id(&pool, profile, "example.com").await;
    let evidence = vec![
        Evidence {
            quote: "Powered by Keycloak".to_owned(),
            provenance: Provenance::parse("https://example.com/about").unwrap(),
        },
        Evidence {
            quote: "version 21.1".to_owned(),
            provenance: Provenance::parse("https://example.com/realms/master").unwrap(),
        },
    ];
    let stored = assessment(lead, 0, evidence);

    let listed = with_tx(&pool, async |shared| {
        let mut repo = PgAssessmentRepository::new(&shared);
        repo.insert(&stored).await?;
        repo.list_for_lead(lead).await
    })
    .await
    .unwrap();

    assert_eq!(listed, [stored]);

    let raw: serde_json::Value = sqlx::query_scalar("SELECT evidence FROM assessments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(raw[0]["quote"], "Powered by Keycloak");
    assert_eq!(raw[1]["provenance"], "https://example.com/realms/master");
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_returns_newest_first(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead_id(&pool, profile, "example.com").await;
    let old = assessment(lead, 0, Vec::new());
    let new = assessment(lead, 60, Vec::new());

    let listed = with_tx(&pool, async |shared| {
        let mut repo = PgAssessmentRepository::new(&shared);
        repo.insert(&old).await?;
        repo.insert(&new).await?;
        repo.list_for_lead(lead).await
    })
    .await
    .unwrap();

    assert_eq!(listed, [new, old]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_stored_draft_comes_back_pending(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead_id(&pool, profile, "example.com").await;
    let mut stored = assessment(lead, 0, Vec::new());
    stored.draft = Some(lodestone_core::DraftMessage {
        subject: "Hello".to_owned(),
        body: "Body".to_owned(),
    });
    stored.draft_review = Some(lodestone_core::DraftReview::pending());

    let listed = with_tx(&pool, async |shared| {
        let mut repo = PgAssessmentRepository::new(&shared);
        repo.insert(&stored).await?;
        repo.list_for_lead(lead).await
    })
    .await
    .unwrap();

    assert_eq!(listed, [stored]);
}
