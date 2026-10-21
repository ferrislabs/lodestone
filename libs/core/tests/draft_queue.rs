mod support;

use std::fmt::Debug;

use common::CoreError;
use lodestone_core::{
    Assessment, AssessmentId, AssessmentRepository, DraftEdit, DraftEntry, DraftMessage,
    DraftReview, DraftRevision, DraftStatus, LeadId, LeadStatus, LodestoneUseCase, ProfileId,
    Rejection, RejectionReason, RevisionNumber, RunProvenance, UserId, VerdictFilter,
    infrastructure::{assessment::postgres::PgAssessmentRepository, postgres::with_tx},
};
use sqlx::PgPool;
use support::{at, create_profile, seen_lead};
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
        .list_pending_drafts(profile, &[], limit, offset)
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

async fn judged(
    pool: &PgPool,
    profile: ProfileId,
    host: &str,
    score: f64,
    judgement: Option<(f64, &str)>,
) {
    let lead = lead(pool, profile, host, "qualified", score).await;
    sqlx::query("UPDATE leads SET confidence = $2, verdict = $3 WHERE id = $1")
        .bind(lead.0)
        .bind(judgement.map(|(confidence, _)| confidence))
        .bind(judgement.map(|(_, verdict)| verdict))
        .execute(pool)
        .await
        .unwrap();
    assessment(pool, lead, 0, true).await;
}

async fn filtered_queue(pool: &PgPool, verdicts: &[VerdictFilter]) -> Vec<String> {
    LodestoneUseCase::new(pool.clone())
        .list_pending_drafts(None, verdicts, 20, 0)
        .await
        .unwrap()
        .items
        .iter()
        .map(|entry| entry.record.lead.domain.as_str().to_owned())
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_queue_lists_the_most_confident_leads_first_pending_ones_last(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    judged(&pool, profile, "pending.com", 99.0, None).await;
    judged(
        &pool,
        profile,
        "unsure.com",
        1.0,
        Some((55.0, "a_verifier")),
    )
    .await;
    judged(&pool, profile, "sure.com", 1.0, Some((91.0, "fiable"))).await;
    judged(
        &pool,
        profile,
        "tie-low.com",
        1.0,
        Some((55.0, "a_verifier")),
    )
    .await;
    judged(
        &pool,
        profile,
        "tie-high.com",
        7.0,
        Some((55.0, "a_verifier")),
    )
    .await;

    let order = filtered_queue(&pool, &[]).await;

    assert_eq!(
        order,
        [
            "sure.com",
            "tie-high.com",
            "unsure.com",
            "tie-low.com",
            "pending.com"
        ]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_queue_filters_by_verdict_including_pending(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    judged(&pool, profile, "pending.com", 1.0, None).await;
    judged(
        &pool,
        profile,
        "unsure.com",
        1.0,
        Some((55.0, "a_verifier")),
    )
    .await;
    judged(&pool, profile, "sure.com", 1.0, Some((91.0, "fiable"))).await;
    judged(&pool, profile, "out.com", 1.0, Some((5.0, "ecarte"))).await;

    assert_eq!(
        filtered_queue(&pool, &[VerdictFilter::Reliable]).await,
        ["sure.com"]
    );
    assert_eq!(
        filtered_queue(&pool, &[VerdictFilter::Reliable, VerdictFilter::Pending]).await,
        ["sure.com", "pending.com"]
    );
    assert_eq!(
        filtered_queue(
            &pool,
            &[
                VerdictFilter::Reliable,
                VerdictFilter::ToCheck,
                VerdictFilter::Pending
            ]
        )
        .await,
        ["sure.com", "unsure.com", "pending.com"]
    );
    assert_eq!(
        filtered_queue(&pool, &[VerdictFilter::Discarded]).await,
        ["out.com"]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unknown_profile_filter_is_not_found(pool: PgPool) {
    let result = LodestoneUseCase::new(pool)
        .list_pending_drafts(Some(ProfileId(Uuid::now_v7())), &[], 20, 0)
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
        .edit_draft(
            id,
            DraftEdit::new(Some("Edited".to_owned()), None).unwrap(),
            None,
        )
        .await
        .unwrap();
    let second = usecase
        .edit_draft(
            id,
            DraftEdit::new(None, Some("New body".to_owned())).unwrap(),
            None,
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

    let entry = usecase.approve_draft(id, reviewer, None).await.unwrap();

    let review = entry.record.assessment.draft_review.unwrap();
    assert_eq!(review.status, DraftStatus::Approved);
    assert_eq!(review.reviewed_by, Some(reviewer));
    assert!(review.reviewed_at.is_some());
    assert_eq!(entry.record.lead.status, LeadStatus::Validated);

    let again = usecase.approve_draft(id, reviewer, None).await;
    assert!(matches!(again, Err(CoreError::Conflict(_))));
    let edit = usecase
        .edit_draft(
            id,
            DraftEdit::new(Some("late".to_owned()), None).unwrap(),
            None,
        )
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
        .reject_draft(
            id,
            reviewer,
            Rejection::new(RejectionReason::Other, Some("not a fit")).unwrap(),
            None,
        )
        .await
        .unwrap();

    let review = entry.record.assessment.draft_review.unwrap();
    assert_eq!(review.status, DraftStatus::Rejected);
    assert_eq!(review.rejection_reason.as_deref(), Some("not a fit"));
    assert_eq!(entry.record.lead.status, LeadStatus::Rejected);
    assert!(matches!(
        usecase
            .reject_draft(
                id,
                reviewer,
                Rejection::new(RejectionReason::OffTarget, None).unwrap(),
                None
            )
            .await,
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
        usecase.approve_draft(id, reviewer, None).await,
        Err(CoreError::NotFound)
    ));
    assert!(matches!(
        usecase
            .reject_draft(
                id,
                reviewer,
                Rejection::new(RejectionReason::OffTarget, None).unwrap(),
                None
            )
            .await,
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

async fn stored(pool: &PgPool, id: AssessmentId) -> (String, i32, bool, String) {
    sqlx::query_as(
        "SELECT draft_subject, draft_revision, draft_edited_at IS NOT NULL, draft_status FROM assessments WHERE id = $1",
    )
    .bind(id.0)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn lead_status(pool: &PgPool, id: LeadId) -> String {
    sqlx::query_scalar("SELECT status FROM leads WHERE id = $1")
        .bind(id.0)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn subject_edit(subject: &str) -> DraftEdit {
    DraftEdit::new(Some(subject.to_owned()), None).unwrap()
}

fn revision_of(entry: &DraftEntry) -> DraftRevision {
    entry
        .record
        .assessment
        .draft_review
        .as_ref()
        .unwrap()
        .revision
}

fn assert_conflict<T: Debug>(result: Result<T, CoreError>, fragment: &str) {
    match result {
        Err(CoreError::Conflict(message)) => assert!(message.contains(fragment), "{message}"),
        other => panic!("expected a conflict mentioning {fragment:?}, got {other:?}"),
    }
}

fn off_target() -> Rejection {
    Rejection::new(RejectionReason::OffTarget, None).unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_successful_edit_bumps_the_revision_from_zero(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead, 10, true).await;
    let usecase = LodestoneUseCase::new(pool.clone());

    assert_eq!(
        revision_of(&usecase.get_draft(id).await.unwrap()),
        DraftRevision::new(0)
    );
    let first = usecase
        .edit_draft(id, subject_edit("One"), Some(DraftRevision::new(0)))
        .await
        .unwrap();
    let second = usecase
        .edit_draft(id, subject_edit("Two"), Some(DraftRevision::new(1)))
        .await
        .unwrap();
    let third = usecase
        .edit_draft(id, subject_edit("Three"), None)
        .await
        .unwrap();

    assert_eq!(revision_of(&first), DraftRevision::new(1));
    assert_eq!(revision_of(&second), DraftRevision::new(2));
    assert_eq!(revision_of(&third), DraftRevision::new(3));
    assert_eq!(
        revision_of(&usecase.get_draft(id).await.unwrap()),
        DraftRevision::new(3)
    );
    assert_eq!(stored(&pool, id).await.1, 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_edit_with_a_stale_revision_is_refused_and_leaves_the_draft_unchanged(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead, 10, true).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    usecase
        .edit_draft(id, subject_edit("Kept"), None)
        .await
        .unwrap();
    let before = stored(&pool, id).await;

    let stale = usecase
        .edit_draft(id, subject_edit("Lost"), Some(DraftRevision::new(0)))
        .await;
    let ahead = usecase
        .edit_draft(id, subject_edit("Lost"), Some(DraftRevision::new(2)))
        .await;

    assert_conflict(stale, "draft changed");
    assert_conflict(ahead, "draft changed");
    assert_eq!(stored(&pool, id).await, before);
    assert_eq!(before.0, "Kept");
    assert_eq!(before.1, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn two_edits_carrying_the_same_revision_let_the_first_win(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead, 10, true).await;
    let usecase = LodestoneUseCase::new(pool.clone());

    let first = usecase
        .edit_draft(id, subject_edit("First"), Some(DraftRevision::new(0)))
        .await;
    let second = usecase
        .edit_draft(id, subject_edit("Second"), Some(DraftRevision::new(0)))
        .await;

    assert_eq!(revision_of(&first.unwrap()), DraftRevision::new(1));
    assert_conflict(second, "draft changed");
    let (subject, revision, _, _) = stored(&pool, id).await;
    assert_eq!((subject.as_str(), revision), ("First", 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_edits_carrying_the_same_revision_let_exactly_one_win(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead, 10, true).await;
    let usecase = LodestoneUseCase::new(pool.clone());

    let (left, right) = tokio::join!(
        usecase.edit_draft(id, subject_edit("Left"), Some(DraftRevision::new(0))),
        usecase.edit_draft(id, subject_edit("Right"), Some(DraftRevision::new(0)))
    );

    assert_eq!(
        [left.is_ok(), right.is_ok()]
            .iter()
            .filter(|ok| **ok)
            .count(),
        1
    );
    let loser = if left.is_ok() { right } else { left };
    assert_conflict(loser, "draft changed");
    assert_eq!(stored(&pool, id).await.1, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn editing_a_draft_that_is_not_the_latest_assessment_is_a_conflict(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let replaced = lead(&pool, profile, "replaced.com", "qualified", 1.0).await;
    let old = assessment(&pool, replaced, 100, true).await;
    let newer = assessment(&pool, replaced, 10, true).await;
    let bare = lead(&pool, profile, "bare.com", "qualified", 1.0).await;
    let bare_old = assessment(&pool, bare, 100, true).await;
    assessment(&pool, bare, 10, false).await;
    let usecase = LodestoneUseCase::new(pool.clone());

    for stale in [old, bare_old] {
        let before = stored(&pool, stale).await;
        assert_conflict(
            usecase.edit_draft(stale, subject_edit("Late"), None).await,
            "superseded",
        );
        assert_conflict(
            usecase
                .edit_draft(stale, subject_edit("Late"), Some(DraftRevision::new(0)))
                .await,
            "superseded",
        );
        assert_eq!(stored(&pool, stale).await, before);
    }

    let edited = usecase
        .edit_draft(newer, subject_edit("Current"), None)
        .await
        .unwrap();
    assert_eq!(revision_of(&edited), DraftRevision::new(1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_decision_with_a_stale_revision_is_refused_and_changes_nothing(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead_id = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead_id, 10, true).await;
    let reviewer = user(&pool).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    usecase
        .edit_draft(id, subject_edit("Edited"), None)
        .await
        .unwrap();

    assert_conflict(
        usecase
            .approve_draft(id, reviewer, Some(DraftRevision::new(0)))
            .await,
        "draft changed",
    );
    assert_conflict(
        usecase
            .reject_draft(id, reviewer, off_target(), Some(DraftRevision::new(0)))
            .await,
        "draft changed",
    );

    let (subject, revision, _, status) = stored(&pool, id).await;
    assert_eq!(
        (subject.as_str(), revision, status.as_str()),
        ("Edited", 1, "pending")
    );
    assert_eq!(lead_status(&pool, lead_id).await, "qualified");
    let reviewed: Option<Uuid> =
        sqlx::query_scalar("SELECT draft_reviewed_by FROM assessments WHERE id = $1")
            .bind(id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reviewed, None);

    let approved = usecase
        .approve_draft(id, reviewer, Some(DraftRevision::new(1)))
        .await
        .unwrap();
    assert_eq!(
        approved.record.assessment.draft_review.unwrap().status,
        DraftStatus::Approved
    );
    assert_eq!(lead_status(&pool, lead_id).await, "validated");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_rejection_with_the_current_revision_goes_through(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead_id = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead_id, 10, true).await;
    let reviewer = user(&pool).await;
    let usecase = LodestoneUseCase::new(pool.clone());

    let entry = usecase
        .reject_draft(id, reviewer, off_target(), Some(DraftRevision::new(0)))
        .await
        .unwrap();

    assert_eq!(
        entry.record.assessment.draft_review.unwrap().status,
        DraftStatus::Rejected
    );
    assert_eq!(lead_status(&pool, lead_id).await, "rejected");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_decision_on_a_superseded_draft_is_still_not_found(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead_id = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let old = assessment(&pool, lead_id, 100, true).await;
    assessment(&pool, lead_id, 10, true).await;
    let reviewer = user(&pool).await;
    let usecase = LodestoneUseCase::new(pool.clone());

    assert!(matches!(
        usecase.approve_draft(old, reviewer, None).await,
        Err(CoreError::NotFound)
    ));
    assert!(matches!(
        usecase
            .reject_draft(old, reviewer, off_target(), None)
            .await,
        Err(CoreError::NotFound)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_adapter_writes_an_edit_only_for_the_expected_revision(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead, 10, true).await;
    let edit = subject_edit("Edited");

    let outcomes = with_tx(&pool, async |shared| {
        let mut repo = PgAssessmentRepository::new(&shared);
        Ok([
            repo.edit_draft(id, &edit, DraftRevision::new(1), at(0))
                .await?,
            repo.edit_draft(id, &edit, DraftRevision::new(0), at(0))
                .await?,
            repo.edit_draft(id, &edit, DraftRevision::new(0), at(0))
                .await?,
            repo.edit_draft(id, &edit, DraftRevision::new(1), at(0))
                .await?,
        ])
    })
    .await
    .unwrap();

    assert_eq!(outcomes, [false, true, false, true]);
    assert_eq!(stored(&pool, id).await.1, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_adapter_decides_only_for_the_expected_revision(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead, 10, true).await;
    let reviewer = user(&pool).await;
    let edit = subject_edit("Edited");
    let decided = DraftReview::pending().approve(reviewer, at(5)).unwrap();

    let outcomes = with_tx(&pool, async |shared| {
        let mut repo = PgAssessmentRepository::new(&shared);
        repo.edit_draft(id, &edit, DraftRevision::new(0), at(1))
            .await?;
        Ok([
            repo.decide_draft(id, &decided, DraftRevision::new(0))
                .await?,
            repo.decide_draft(id, &decided, DraftRevision::new(1))
                .await?,
            repo.decide_draft(id, &decided, DraftRevision::new(1))
                .await?,
        ])
    })
    .await
    .unwrap();

    assert_eq!(outcomes, [false, true, false]);
    let (_, revision, _, status) = stored(&pool, id).await;
    assert_eq!((revision, status.as_str()), (1, "approved"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_adapter_stores_and_reads_back_the_carried_revision(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let fresh = Assessment {
        id: AssessmentId(Uuid::now_v7()),
        lead_id: lead,
        profile_revision: RevisionNumber::new(1).unwrap(),
        summary: "summary".to_owned(),
        verdict_narrative: String::new(),
        approach_angle: "angle".to_owned(),
        evidence: Vec::new(),
        evidence_verified: true,
        quality_warnings: Vec::new(),
        provenance: RunProvenance::legacy("model-x".to_owned(), "v1".to_owned()),
        draft: Some(DraftMessage {
            subject: "Subject".to_owned(),
            body: "Body".to_owned(),
        }),
        draft_review: Some(DraftReview {
            revision: DraftRevision::new(7),
            ..DraftReview::pending()
        }),
        created_at: at(0),
    };
    let unreviewed = Assessment {
        id: AssessmentId(Uuid::now_v7()),
        draft_review: None,
        created_at: at(1),
        ..fresh.clone()
    };

    let listed = with_tx(&pool, async |shared| {
        let mut repo = PgAssessmentRepository::new(&shared);
        repo.insert(&fresh).await?;
        repo.insert(&unreviewed).await?;
        repo.list_for_lead(lead).await
    })
    .await
    .unwrap();

    assert_eq!(stored(&pool, fresh.id).await.1, 7);
    assert_eq!(stored(&pool, unreviewed.id).await.1, 0);
    let by_id = |id: AssessmentId| {
        listed
            .iter()
            .find(|assessment| assessment.id == id)
            .unwrap()
    };
    assert_eq!(
        by_id(fresh.id).draft_review.as_ref().unwrap().revision,
        DraftRevision::new(7)
    );
    assert_eq!(
        by_id(unreviewed.id).draft_review.as_ref().unwrap().revision,
        DraftRevision::default()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_stored_revision_that_cannot_be_read_is_an_internal_error(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = lead(&pool, profile, "a.com", "qualified", 1.0).await;
    let id = assessment(&pool, lead, 10, true).await;
    sqlx::query("ALTER TABLE assessments DROP CONSTRAINT assessments_draft_revision_check")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE assessments SET draft_revision = -1 WHERE id = $1")
        .bind(id.0)
        .execute(&pool)
        .await
        .unwrap();

    let result = LodestoneUseCase::new(pool).get_draft(id).await;

    assert!(matches!(result, Err(CoreError::Internal(_))), "{result:?}");
}
