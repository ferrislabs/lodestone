mod support;

use common::CoreError;
use lodestone_core::{
    AssessmentId, DraftEdit, DraftStatus, LeadId, LeadStatus, LodestoneUseCase, ProfileId, UserId,
};
use sqlx::PgPool;
use support::{create_profile, seen_lead};
use uuid::Uuid;

async fn user(pool: &PgPool) -> UserId {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, sub, username, display_name) VALUES ($1, $1::text, 'alice', 'Alice')")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    UserId(id)
}

async fn lead(pool: &PgPool, profile: ProfileId, host: &str, status: &str, score: f64) -> LeadId {
    let lead = seen_lead(pool, profile, host).await;
    sqlx::query("UPDATE leads SET status = $2, score = $3 WHERE id = $1")
        .bind(lead.id.0)
        .bind(status)
        .bind(score)
        .execute(pool)
        .await
        .unwrap();
    lead.id
}

async fn assessment(
    pool: &PgPool,
    lead: LeadId,
    age_seconds: i64,
    with_draft: bool,
) -> AssessmentId {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO assessments (id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, created_at, draft_subject, draft_body, draft_status)
         VALUES ($1, $2, 1, 'summary', 'angle', '[]'::jsonb, 'model-x', 'qualification@1', now() - make_interval(secs => $3), $4, $5, CASE WHEN $4::text IS NULL THEN NULL ELSE 'pending' END)",
    )
    .bind(id)
    .bind(lead.0)
    .bind(age_seconds as f64)
    .bind(with_draft.then_some("Subject"))
    .bind(with_draft.then_some("Body"))
    .execute(pool)
    .await
    .unwrap();
    AssessmentId(id)
}

async fn queue(
    pool: &PgPool,
    profile: Option<ProfileId>,
    limit: u32,
    offset: u64,
) -> (Vec<String>, u64) {
    let queue = LodestoneUseCase::new(pool.clone())
        .list_pending_drafts(profile, limit, offset)
        .await
        .unwrap();
    (
        queue
            .items
            .iter()
            .map(|entry| entry.record.lead.domain.as_str().to_owned())
            .collect(),
        queue.total,
    )
}

#[sqlx::test(migrations = "../../migrations")]
async fn inserted_drafts_default_to_pending_and_others_to_no_status(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let with = assessment(&pool, lead, 10, true).await;
    let without = assessment(&pool, lead, 5, false).await;

    let status = |id: AssessmentId| {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT draft_status FROM assessments WHERE id = $1",
        )
        .bind(id.0)
        .fetch_one(&pool)
    };
    assert_eq!(status(with).await.unwrap().as_deref(), Some("pending"));
    assert_eq!(status(without).await.unwrap(), None);

    let orphan = sqlx::query(
        "UPDATE assessments SET draft_status = 'approved', draft_reviewed_at = now() WHERE id = $1",
    )
    .bind(without.0)
    .execute(&pool)
    .await;
    assert!(orphan.is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn queue_holds_the_latest_pending_drafts_of_qualified_visible_leads(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let other = create_profile(&pool, "beta").await;

    let high = lead(&pool, profile, "high.com", "qualified", 9.0).await;
    assessment(&pool, high, 10, true).await;
    let low = lead(&pool, profile, "low.com", "qualified", 2.0).await;
    assessment(&pool, low, 10, true).await;
    let tie_old = lead(&pool, profile, "tie-old.com", "qualified", 5.0).await;
    assessment(&pool, tie_old, 100, true).await;
    let tie_new = lead(&pool, profile, "tie-new.com", "qualified", 5.0).await;
    assessment(&pool, tie_new, 50, true).await;
    let elsewhere = lead(&pool, other, "other.com", "qualified", 7.0).await;
    assessment(&pool, elsewhere, 10, true).await;

    let superseded = lead(&pool, profile, "superseded.com", "qualified", 8.0).await;
    assessment(&pool, superseded, 100, true).await;
    assessment(&pool, superseded, 10, false).await;
    let not_qualified = lead(&pool, profile, "validated.com", "validated", 8.0).await;
    assessment(&pool, not_qualified, 10, true).await;
    let suppressed = lead(&pool, profile, "suppressed.com", "qualified", 8.0).await;
    assessment(&pool, suppressed, 10, true).await;
    sqlx::query("INSERT INTO suppression_list (id, kind, value, created_at) VALUES ($1, 'domain', 'suppressed.com', now())")
        .bind(Uuid::now_v7())
        .execute(&pool)
        .await
        .unwrap();
    let decided = lead(&pool, profile, "decided.com", "qualified", 8.0).await;
    let decided_assessment = assessment(&pool, decided, 10, true).await;
    sqlx::query(
        "UPDATE assessments SET draft_status = 'rejected', draft_reviewed_at = now() WHERE id = $1",
    )
    .bind(decided_assessment.0)
    .execute(&pool)
    .await
    .unwrap();

    let (all, total) = queue(&pool, None, 20, 0).await;
    assert_eq!(
        all,
        [
            "high.com",
            "other.com",
            "tie-old.com",
            "tie-new.com",
            "low.com"
        ]
    );
    assert_eq!(total, 5);

    let (filtered, total) = queue(&pool, Some(profile), 20, 0).await;
    assert_eq!(
        filtered,
        ["high.com", "tie-old.com", "tie-new.com", "low.com"]
    );
    assert_eq!(total, 4);

    let (second_page, total) = queue(&pool, Some(profile), 2, 2).await;
    assert_eq!(second_page, ["tie-new.com", "low.com"]);
    assert_eq!(total, 4);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unknown_profile_filter_is_not_found(pool: PgPool) {
    let result = LodestoneUseCase::new(pool)
        .list_pending_drafts(Some(ProfileId(Uuid::now_v7())), 20, 0)
        .await;
    assert!(matches!(result, Err(CoreError::NotFound)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn editing_keeps_the_first_original_and_stamps_the_edit(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead, 10, true).await;
    let usecase = LodestoneUseCase::new(pool.clone());

    let first = usecase
        .edit_draft(id, DraftEdit::new(Some("Edited".to_owned()), None).unwrap())
        .await
        .unwrap();
    let second = usecase
        .edit_draft(
            id,
            DraftEdit::new(None, Some("New body".to_owned())).unwrap(),
        )
        .await
        .unwrap();

    let draft = second.record.assessment.draft.unwrap();
    assert_eq!(
        (draft.subject.as_str(), draft.body.as_str()),
        ("Edited", "New body")
    );
    let review = second.record.assessment.draft_review.unwrap();
    let original = review.original.unwrap();
    assert_eq!(
        (original.subject.as_str(), original.body.as_str()),
        ("Subject", "Body")
    );
    assert!(review.edited_at.is_some());
    assert!(review.edited_at >= first.record.assessment.draft_review.unwrap().edited_at);
    assert_eq!(review.status, DraftStatus::Pending);
}

#[sqlx::test(migrations = "../../migrations")]
async fn approving_validates_the_lead_and_records_the_reviewer(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead_id = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead_id, 10, true).await;
    let reviewer = user(&pool).await;
    let usecase = LodestoneUseCase::new(pool.clone());

    let entry = usecase.approve_draft(id, reviewer).await.unwrap();

    let review = entry.record.assessment.draft_review.unwrap();
    assert_eq!(review.status, DraftStatus::Approved);
    assert_eq!(review.reviewed_by, Some(reviewer));
    assert!(review.reviewed_at.is_some());
    assert_eq!(entry.record.lead.status, LeadStatus::Validated);

    let again = usecase.approve_draft(id, reviewer).await;
    assert!(matches!(again, Err(CoreError::Conflict(_))));
    let edit = usecase
        .edit_draft(id, DraftEdit::new(Some("late".to_owned()), None).unwrap())
        .await;
    assert!(matches!(edit, Err(CoreError::Conflict(_))));
    assert_eq!(queue(&pool, None, 20, 0).await.1, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rejecting_rejects_the_lead_and_keeps_the_reason(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead_id = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead_id, 10, true).await;
    let reviewer = user(&pool).await;
    let usecase = LodestoneUseCase::new(pool.clone());

    let entry = usecase
        .reject_draft(id, reviewer, Some("not a fit".to_owned()))
        .await
        .unwrap();

    let review = entry.record.assessment.draft_review.unwrap();
    assert_eq!(review.status, DraftStatus::Rejected);
    assert_eq!(review.rejection_reason.as_deref(), Some("not a fit"));
    assert_eq!(entry.record.lead.status, LeadStatus::Rejected);
    assert!(matches!(
        usecase.reject_draft(id, reviewer, None).await,
        Err(CoreError::Conflict(_))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_suppressed_domain_hides_the_draft_and_blocks_decisions(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead_id = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead_id, 10, true).await;
    let reviewer = user(&pool).await;
    sqlx::query("INSERT INTO suppression_list (id, kind, value, created_at) VALUES ($1, 'domain', 'a.com', now())")
        .bind(Uuid::now_v7())
        .execute(&pool)
        .await
        .unwrap();
    let usecase = LodestoneUseCase::new(pool.clone());

    assert!(matches!(
        usecase.get_draft(id).await,
        Err(CoreError::NotFound)
    ));
    assert!(matches!(
        usecase.approve_draft(id, reviewer).await,
        Err(CoreError::NotFound)
    ));
    assert!(matches!(
        usecase.reject_draft(id, reviewer, None).await,
        Err(CoreError::NotFound)
    ));
    let status: String = sqlx::query_scalar("SELECT status FROM leads WHERE id = $1")
        .bind(lead_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "qualified");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_shown_contact_skips_suppressed_addresses(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead_id = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead_id, 10, true).await;
    for (address, confidence) in [("security@a.com", 0.9), ("contact@a.com", 0.5)] {
        sqlx::query(
            "INSERT INTO contact_emails (id, lead_id, address, provenance, method, confidence, kind, found_at)
             VALUES ($1, $2, $3, 'https://a.com/security.txt', 'security_txt', $4, 'role', now())",
        )
        .bind(Uuid::now_v7())
        .bind(lead_id.0)
        .bind(address)
        .bind(confidence)
        .execute(&pool)
        .await
        .unwrap();
    }
    let usecase = LodestoneUseCase::new(pool.clone());
    let shown =
        |entry: lodestone_core::DraftEntry| entry.contact.map(|c| c.address.as_str().to_owned());

    assert_eq!(
        shown(usecase.get_draft(id).await.unwrap()).as_deref(),
        Some("security@a.com")
    );

    sqlx::query("INSERT INTO suppression_list (id, kind, value, created_at) VALUES ($1, 'email', 'security@a.com', now())")
        .bind(Uuid::now_v7())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        shown(usecase.get_draft(id).await.unwrap()).as_deref(),
        Some("contact@a.com")
    );
}
