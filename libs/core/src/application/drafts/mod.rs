use chrono::Utc;
use common::CoreError;
use lodestone_macros::transactional;

use crate::{
    UserId,
    application::{LodestoneUseCase, output::schedule_webhook, quality::record_decisions},
    domain::{
        lead::{
            AssessmentId, ContactEmail, Lead, LeadStatus,
            draft::{
                DraftEdit, DraftEntry, DraftQueue, DraftRecord, DraftReview, DraftStatus,
                InvalidDraftTransition,
            },
            ports::{AssessmentRepository, ContactRepository, LeadRepository},
            query::VerdictFilter,
        },
        profile::{ProfileId, ports::ProfileRepository},
        quality::DecisionKind,
        suppression::ports::SuppressionRepository,
    },
    infrastructure::job::postgres::PgJobOutbox,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Decision {
    Approve,
    Reject(Option<String>),
}

impl Decision {
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
            Self::Reject(reason) => review.reject(by, at, reason.clone()),
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
    ) -> Result<DraftEntry, CoreError> {
        let mut assessments = assessment_repository;
        let mut contacts = contact_repository;
        let mut suppression = suppression_repository;

        let record = visible_draft(&mut assessments, &mut suppression, id).await?;
        require_pending(&record)?;
        if !assessments.edit_draft(id, &edit, Utc::now()).await? {
            return Err(conflict("draft is no longer pending"));
        }
        let record = visible_draft(&mut assessments, &mut suppression, id).await?;
        let contact = best_contact(&mut contacts, &mut suppression, &record.lead).await?;
        Ok(DraftEntry { record, contact })
    }

    pub async fn approve_draft(
        &self,
        id: AssessmentId,
        actor: UserId,
    ) -> Result<DraftEntry, CoreError> {
        self.decide_draft(id, actor, Decision::Approve).await
    }

    pub async fn reject_draft(
        &self,
        id: AssessmentId,
        actor: UserId,
        reason: Option<String>,
    ) -> Result<DraftEntry, CoreError> {
        self.decide_draft(id, actor, Decision::Reject(reason)).await
    }

    #[transactional(
        assessment,
        lead,
        contact,
        suppression,
        qualification,
        profile,
        decision
    )]
    async fn decide_draft(
        &self,
        id: AssessmentId,
        actor: UserId,
        decision: Decision,
    ) -> Result<DraftEntry, CoreError> {
        let mut assessments = assessment_repository;
        let mut leads = lead_repository;
        let mut contacts = contact_repository;
        let mut suppression = suppression_repository;
        let mut qualification = qualification_repository;
        let mut profiles = profile_repository;
        let mut decisions = decision_repository;

        let record = visible_draft(&mut assessments, &mut suppression, id).await?;
        let review = require_pending(&record)?;
        let latest = assessments
            .list_for_lead(record.lead.id)
            .await?
            .first()
            .map(|assessment| assessment.id);
        if latest != Some(id) {
            return Err(CoreError::NotFound);
        }
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

        if !assessments.decide_draft(id, &decided).await? {
            return Err(conflict("draft is no longer pending"));
        }
        let updated = leads
            .update_status(record.lead.id, from, to, now)
            .await?
            .ok_or_else(|| conflict("lead status changed concurrently, reload and retry"))?;
        let lead_kind = DecisionKind::for_lead(to);
        let kinds: Vec<DecisionKind> = std::iter::once(decision.kind()).chain(lead_kind).collect();
        record_decisions(
            &mut decisions,
            &mut qualification,
            &mut profiles,
            &record.lead,
            &kinds,
            actor,
            now,
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
