mod support;

use lodestone_core::{
    Assessment, AssessmentId, AssessmentRepository, Evidence, GenerationSnapshot, Provenance,
    ProviderId, ReasoningEffort, RevisionNumber, RunProvenance,
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
        quality_warnings: Vec::new(),
        provenance: RunProvenance::legacy("model-x".to_owned(), "v1".to_owned()),
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

#[sqlx::test(migrations = "../../migrations")]
async fn provenance_round_trips(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead_id(&pool, profile, "example.com").await;
    let mut stored = assessment(lead, 0, Vec::new());
    stored.provenance = RunProvenance {
        provider_id: Some(ProviderId(uuid::Uuid::now_v7())),
        provider_name: "cloud".to_owned(),
        model: "big-model".to_owned(),
        prompt_version: "aggregator@abc".to_owned(),
        generation: GenerationSnapshot {
            temperature: Some(0.1),
            max_tokens: Some(4096),
            frequency_penalty: Some(0.2),
            presence_penalty: None,
            repeat_penalty: Some(1.1),
            reasoning_effort: Some(ReasoningEffort::High),
        },
        samples: 3,
    };

    let listed = with_tx(&pool, async |shared| {
        let mut repo = PgAssessmentRepository::new(&shared);
        repo.insert(&stored).await?;
        repo.list_for_lead(lead).await
    })
    .await
    .unwrap();

    assert_eq!(listed, [stored]);
    let (name, samples): (String, i32) =
        sqlx::query_as("SELECT provider_name, samples FROM assessments")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((name.as_str(), samples), ("cloud", 3));
}

#[sqlx::test(migrations = "../../migrations")]
async fn rows_written_without_provenance_read_with_defaults(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead_id(&pool, profile, "example.com").await;
    sqlx::query(
        "INSERT INTO assessments (id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, created_at)
         VALUES ($1, $2, 1, 's', 'a', '[]'::jsonb, 'm', 'aggregator@x', now())",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(lead.0)
    .execute(&pool)
    .await
    .unwrap();

    let listed = with_tx(&pool, async |shared| {
        PgAssessmentRepository::new(&shared)
            .list_for_lead(lead)
            .await
    })
    .await
    .unwrap();

    let provenance = &listed[0].provenance;
    assert_eq!(provenance.provider_id, None);
    assert_eq!(provenance.provider_name, "");
    assert_eq!(provenance.generation, GenerationSnapshot::default());
    assert_eq!(provenance.samples, 1);
}
