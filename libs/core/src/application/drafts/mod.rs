use chrono::Utc;
use common::CoreError;
use lodestone_macros::transactional;

use crate::{
    UserId,
    application::{
        LodestoneUseCase,
        output::schedule_webhook,
        quality::{Decided, record_decisions},
        timeline::{decision_detail, status_changed},
    },
    domain::{
        lead::{
            AssessmentId, ContactEmail, Lead, LeadStatus,
            draft::{
                DraftEdit, DraftEntry, DraftQueue, DraftRecord, DraftReview, DraftRevision,
                DraftStatus, InvalidDraftTransition,
            },
            ports::{AssessmentRepository, ContactRepository, LeadRepository},
            query::VerdictFilter,
        },
        profile::{ProfileId, ports::ProfileRepository},
        quality::{DecisionKind, Rejection},
        suppression::ports::SuppressionRepository,
        timeline::ports::TimelineRepository,
    },
    infrastructure::job::postgres::PgJobOutbox,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Decision {
    Approve,
    Reject(Rejection),
}

impl Decision {
    fn rejection(&self) -> Option<&Rejection> {
        match self {
            Self::Approve => None,
            Self::Reject(rejection) => Some(rejection),
        }
    }

    fn kind(&self) -> DecisionKind {
        match self {
            Self::Approve => DecisionKind::DraftApproved,
            Self::Reject(_) => DecisionKind::DraftRejected,
        }
    }

    fn target(&self) -> LeadStatus {
        match self {
            Self::Approve => LeadStatus::Validated,
            Self::Reject(_) => LeadStatus::Rejected,
        }
    }

    fn apply(
        &self,
        review: &DraftReview,
        by: UserId,
        at: chrono::DateTime<Utc>,
    ) -> Result<DraftReview, InvalidDraftTransition> {
        match self {
            Self::Approve => review.approve(by, at),
            Self::Reject(rejection) => review.reject(by, at, rejection.note().map(str::to_owned)),
        }
    }
}

fn conflict(reason: impl std::fmt::Display) -> CoreError {
    CoreError::Conflict(reason.to_string())
}

async fn visible_draft(
    assessments: &mut impl AssessmentRepository,
    suppression: &mut impl SuppressionRepository,
    id: AssessmentId,
) -> Result<DraftRecord, CoreError> {
    let record = assessments
        .find_draft(id)
        .await?
        .ok_or(CoreError::NotFound)?;
    if suppression
        .is_domain_suppressed(&record.lead.domain)
        .await?
    {
        return Err(CoreError::NotFound);
    }
    Ok(record)
}

fn require_pending(record: &DraftRecord) -> Result<&DraftReview, CoreError> {
    let review = record
        .assessment
        .draft_review
        .as_ref()
        .ok_or(CoreError::NotFound)?;
    if review.status == DraftStatus::Pending {
        Ok(review)
    } else {
        Err(conflict(format!(
            "draft is already {}",
            review.status.as_str()
        )))
    }
}

fn guarded_revision(
    review: &DraftReview,
    expected: Option<DraftRevision>,
) -> Result<DraftRevision, CoreError> {
    let current = review.revision;
    let expected = expected.unwrap_or(current);
    if expected == current {
        Ok(expected)
    } else {
        Err(conflict("draft changed, reload it"))
    }
}

async fn is_latest(
    assessments: &mut impl AssessmentRepository,
    record: &DraftRecord,
) -> Result<bool, CoreError> {
    let latest = assessments
        .list_for_lead(record.lead.id)
        .await?
        .first()
        .map(|assessment| assessment.id);
    Ok(latest == Some(record.assessment.id))
}

async fn best_contact(
    contacts: &mut impl ContactRepository,
    suppression: &mut impl SuppressionRepository,
    lead: &Lead,
) -> Result<Option<ContactEmail>, CoreError> {
    for contact in contacts.list_for_lead(lead.id).await? {
        if !suppression.is_email_suppressed(&contact.address).await? {
            return Ok(Some(contact));
        }
    }
    Ok(None)
}

impl LodestoneUseCase {
    #[transactional(assessment, profile, contact, suppression)]
    pub async fn list_pending_drafts(
        &self,
        profile_id: Option<ProfileId>,
        verdicts: &[VerdictFilter],
        limit: u32,
        offset: u64,
    ) -> Result<DraftQueue, CoreError> {
        let mut assessments = assessment_repository;
        let mut profiles = profile_repository;
        let mut contacts = contact_repository;
        let mut suppression = suppression_repository;

        if let Some(profile_id) = profile_id
            && profiles.find(profile_id).await?.is_none()
        {
            return Err(CoreError::NotFound);
        }
        let page = assessments
            .list_pending_drafts(profile_id, verdicts, limit, offset)
            .await?;
        let mut items = Vec::with_capacity(page.items.len());
        for record in page.items {
            let contact = best_contact(&mut contacts, &mut suppression, &record.lead).await?;
            items.push(DraftEntry { record, contact });
        }
        Ok(DraftQueue {
            items,
            total: page.total,
        })
    }

    #[transactional(assessment, contact, suppression)]
    pub async fn get_draft(&self, id: AssessmentId) -> Result<DraftEntry, CoreError> {
        let mut assessments = assessment_repository;
        let mut contacts = contact_repository;
        let mut suppression = suppression_repository;

        let record = visible_draft(&mut assessments, &mut suppression, id).await?;
        let contact = best_contact(&mut contacts, &mut suppression, &record.lead).await?;
        Ok(DraftEntry { record, contact })
    }

    #[transactional(assessment, contact, suppression)]
    pub async fn edit_draft(
        &self,
        id: AssessmentId,
        edit: DraftEdit,
        expected_revision: Option<DraftRevision>,
    ) -> Result<DraftEntry, CoreError> {
        let mut assessments = assessment_repository;
        let mut contacts = contact_repository;
        let mut suppression = suppression_repository;

        let record = visible_draft(&mut assessments, &mut suppression, id).await?;
        let review = require_pending(&record)?;
        if !is_latest(&mut assessments, &record).await? {
            return Err(conflict("draft was superseded by a newer assessment"));
        }
        let expected = guarded_revision(review, expected_revision)?;
        if !assessments
            .edit_draft(id, &edit, expected, Utc::now())
            .await?
        {
            return Err(conflict("draft changed or no longer pending"));
        }
        let record = visible_draft(&mut assessments, &mut suppression, id).await?;
        let contact = best_contact(&mut contacts, &mut suppression, &record.lead).await?;
        Ok(DraftEntry { record, contact })
    }

    pub async fn approve_draft(
        &self,
        id: AssessmentId,
        actor: UserId,
        expected_revision: Option<DraftRevision>,
    ) -> Result<DraftEntry, CoreError> {
        self.decide_draft(id, actor, Decision::Approve, expected_revision)
            .await
    }

    pub async fn reject_draft(
        &self,
        id: AssessmentId,
        actor: UserId,
        rejection: Rejection,
        expected_revision: Option<DraftRevision>,
    ) -> Result<DraftEntry, CoreError> {
        self.decide_draft(id, actor, Decision::Reject(rejection), expected_revision)
            .await
    }

    #[transactional(
        assessment,
        lead,
        contact,
        suppression,
        qualification,
        scoring_agent,
        profile,
        decision,
        timeline
    )]
    async fn decide_draft(
        &self,
        id: AssessmentId,
        actor: UserId,
        decision: Decision,
        expected_revision: Option<DraftRevision>,
    ) -> Result<DraftEntry, CoreError> {
        let mut assessments = assessment_repository;
        let mut leads = lead_repository;
        let mut contacts = contact_repository;
        let mut suppression = suppression_repository;
        let mut qualification = qualification_repository;
        let mut scoring_agents = scoring_agent_repository;
        let mut profiles = profile_repository;
        let mut decisions = decision_repository;
        let mut timeline = timeline_repository;

        let record = visible_draft(&mut assessments, &mut suppression, id).await?;
        let review = require_pending(&record)?;
        if !is_latest(&mut assessments, &record).await? {
            return Err(CoreError::NotFound);
        }
        let expected = guarded_revision(review, expected_revision)?;
        let from = record.lead.status;
        if from != LeadStatus::Qualified {
            return Err(conflict(format!(
                "lead is {}, only qualified leads can be decided",
                from.as_str()
            )));
        }
        let now = Utc::now();
        let decided = decision.apply(review, actor, now).map_err(conflict)?;
        let to = from.move_to(decision.target()).map_err(conflict)?;

        if !assessments.decide_draft(id, &decided, expected).await? {
            return Err(conflict("draft changed or no longer pending"));
        }
        let updated = leads
            .update_status(record.lead.id, from, to, now)
            .await?
            .ok_or_else(|| conflict("lead status changed concurrently, reload and retry"))?;
        let lead_kind = DecisionKind::for_lead(to);
        let kinds: Vec<DecisionKind> = std::iter::once(decision.kind()).chain(lead_kind).collect();
        timeline
            .record(
                record.lead.id,
                &status_changed(
                    from,
                    to,
                    now,
                    Some(decision_detail(&kinds, actor, decision.rejection())),
                ),
            )
            .await?;
        record_decisions(
            &mut decisions,
            &mut qualification,
            &mut scoring_agents,
            &mut profiles,
            &mut assessments,
            &record.lead,
            Decided {
                kinds: &kinds,
                rejection: decision.rejection(),
                actor,
                at: now,
            },
        )
        .await?;
        if to == LeadStatus::Validated {
            schedule_webhook(&mut PgJobOutbox::new(&tx), self.webhooks.as_ref(), &updated).await?;
        }
        tracing::info!(assessment.id = %id, lead.id = %record.lead.id, actor = %actor.0, draft.status = decided.status.as_str(), lead.status = to.as_str(), "draft decided");

        let record = visible_draft(&mut assessments, &mut suppression, id).await?;
        let contact = best_contact(&mut contacts, &mut suppression, &record.lead).await?;
        Ok(DraftEntry { record, contact })
    }
}
