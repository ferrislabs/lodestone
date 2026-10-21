use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::domain::{
    UserId,
    lead::{Assessment, ContactEmail, Lead, str_enum},
    profile::{ProfileId, ProfileName},
    prompt::DraftMessage,
};

pub const MAX_SUBJECT_CHARS: usize = 200;
pub const MAX_BODY_CHARS: usize = 10_000;
pub const MAX_REASON_CHARS: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DraftStatus {
    Pending,
    Approved,
    Rejected,
}

str_enum!(DraftStatus {
    Pending => "pending",
    Approved => "approved",
    Rejected => "rejected",
});

#[derive(Debug, Error, PartialEq, Eq)]
#[error("draft cannot move from {from:?} to {to:?}")]
pub struct InvalidDraftTransition {
    pub from: DraftStatus,
    pub to: DraftStatus,
}

impl DraftStatus {
    pub const ALL: [Self; 3] = [Self::Pending, Self::Approved, Self::Rejected];

    pub fn can_move_to(self, to: Self) -> bool {
        matches!(
            (self, to),
            (Self::Pending, Self::Approved) | (Self::Pending, Self::Rejected)
        )
    }

    pub fn move_to(self, to: Self) -> Result<Self, InvalidDraftTransition> {
        if self.can_move_to(to) {
            Ok(to)
        } else {
            Err(InvalidDraftTransition { from: self, to })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftProblem {
    pub field: &'static str,
    pub message: String,
}

impl DraftProblem {
    fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftReview {
    pub status: DraftStatus,
    pub original: Option<DraftMessage>,
    pub edited_at: Option<DateTime<Utc>>,
    pub reviewed_by: Option<UserId>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    pub suggestion: Option<DraftMessage>,
    pub outdated_at: Option<DateTime<Utc>>,
}

impl DraftReview {
    pub fn pending() -> Self {
        Self {
            status: DraftStatus::Pending,
            original: None,
            edited_at: None,
            reviewed_by: None,
            reviewed_at: None,
            rejection_reason: None,
            suggestion: None,
            outdated_at: None,
        }
    }

    pub fn is_human_owned(&self) -> bool {
        self.edited_at.is_some() || self.status != DraftStatus::Pending
    }

    pub fn approve(&self, by: UserId, at: DateTime<Utc>) -> Result<Self, InvalidDraftTransition> {
        self.decide(DraftStatus::Approved, by, at, None)
    }

    pub fn reject(
        &self,
        by: UserId,
        at: DateTime<Utc>,
        reason: Option<String>,
    ) -> Result<Self, InvalidDraftTransition> {
        self.decide(DraftStatus::Rejected, by, at, reason)
    }

    fn decide(
        &self,
        to: DraftStatus,
        by: UserId,
        at: DateTime<Utc>,
        reason: Option<String>,
    ) -> Result<Self, InvalidDraftTransition> {
        Ok(Self {
            status: self.status.move_to(to)?,
            reviewed_by: Some(by),
            reviewed_at: Some(at),
            rejection_reason: reason,
            ..self.clone()
        })
    }
}

pub fn carry_draft(
    previous: Option<&Assessment>,
    mut fresh: Assessment,
    at: DateTime<Utc>,
) -> Assessment {
    let Some((draft, review)) = previous.and_then(|assessment| {
        Some((
            assessment.draft.as_ref()?,
            assessment.draft_review.as_ref()?,
        ))
    }) else {
        return fresh;
    };
    if !review.is_human_owned() {
        return fresh;
    }
    let pending = review.status == DraftStatus::Pending;
    let suggestion = fresh.draft.take();
    fresh.draft = Some(draft.clone());
    fresh.draft_review = Some(if pending {
        DraftReview {
            outdated_at: suggestion.as_ref().map(|_| at),
            suggestion,
            ..review.clone()
        }
    } else {
        review.clone()
    });
    fresh
}

pub fn normalize_reason(reason: Option<&str>) -> Result<Option<String>, DraftProblem> {
    match reason.map(str::trim) {
        None | Some("") => Ok(None),
        Some(text) if text.chars().count() > MAX_REASON_CHARS => Err(DraftProblem::new(
            "reason",
            format!("must be at most {MAX_REASON_CHARS} characters"),
        )),
        Some(text) => Ok(Some(text.to_owned())),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftEdit {
    subject: Option<String>,
    body: Option<String>,
}

impl DraftEdit {
    pub fn new(subject: Option<String>, body: Option<String>) -> Result<Self, Vec<DraftProblem>> {
        let mut problems = Vec::new();
        if subject.is_none() && body.is_none() {
            problems.push(DraftProblem::new(
                "$",
                "at least one of subject or body is required",
            ));
        }
        if let Some(subject) = &subject {
            check_text("subject", subject, MAX_SUBJECT_CHARS, &mut problems);
        }
        if let Some(body) = &body {
            check_text("body", body, MAX_BODY_CHARS, &mut problems);
        }
        if problems.is_empty() {
            Ok(Self { subject, body })
        } else {
            Err(problems)
        }
    }

    pub fn subject(&self) -> Option<&str> {
        self.subject.as_deref()
    }

    pub fn body(&self) -> Option<&str> {
        self.body.as_deref()
    }
}

fn check_text(field: &'static str, text: &str, max: usize, problems: &mut Vec<DraftProblem>) {
    if text.trim().is_empty() {
        problems.push(DraftProblem::new(field, "must not be empty"));
    } else if text.chars().count() > max {
        problems.push(DraftProblem::new(
            field,
            format!("must be at most {max} characters"),
        ));
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DraftRecord {
    pub assessment: Assessment,
    pub lead: Lead,
    pub profile_id: ProfileId,
    pub profile_name: ProfileName,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DraftPage {
    pub items: Vec<DraftRecord>,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DraftEntry {
    pub record: DraftRecord,
    pub contact: Option<ContactEmail>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DraftQueue {
    pub items: Vec<DraftEntry>,
    pub total: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> UserId {
        UserId(uuid::Uuid::nil())
    }

    fn at() -> DateTime<Utc> {
        DateTime::<Utc>::UNIX_EPOCH
    }

    #[test]
    fn only_pending_drafts_can_be_decided() {
        for from in DraftStatus::ALL {
            for to in DraftStatus::ALL {
                let allowed = from == DraftStatus::Pending && to != DraftStatus::Pending;
                assert_eq!(from.can_move_to(to), allowed, "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    fn statuses_round_trip_through_text() {
        for status in DraftStatus::ALL {
            assert_eq!(DraftStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(DraftStatus::parse("sent"), None);
    }

    fn assessment(subject: &str, review: Option<DraftReview>) -> Assessment {
        Assessment {
            id: crate::domain::lead::AssessmentId(uuid::Uuid::nil()),
            lead_id: crate::domain::lead::LeadId(uuid::Uuid::nil()),
            profile_revision: crate::domain::profile::RevisionNumber::new(1).unwrap(),
            summary: subject.to_owned(),
            verdict_narrative: String::new(),
            approach_angle: String::new(),
            evidence: Vec::new(),
            evidence_verified: true,
            model: String::new(),
            prompt_version: String::new(),
            draft: Some(DraftMessage {
                subject: subject.to_owned(),
                body: format!("body of {subject}"),
            }),
            draft_review: review,
            created_at: at(),
        }
    }

    fn later() -> DateTime<Utc> {
        at() + chrono::Duration::hours(1)
    }

    #[test]
    fn an_unedited_pending_draft_is_replaced() {
        let previous = assessment("old", Some(DraftReview::pending()));
        let merged = carry_draft(
            Some(&previous),
            assessment("new", Some(DraftReview::pending())),
            later(),
        );
        assert_eq!(merged.draft.unwrap().subject, "new");
        assert_eq!(merged.draft_review, Some(DraftReview::pending()));
        let first = carry_draft(
            None,
            assessment("new", Some(DraftReview::pending())),
            later(),
        );
        assert_eq!(first.draft.unwrap().subject, "new");
    }

    #[test]
    fn an_edited_pending_draft_is_kept_and_the_new_one_becomes_a_suggestion() {
        let review = DraftReview {
            edited_at: Some(at()),
            original: Some(DraftMessage {
                subject: "model".to_owned(),
                body: "model body".to_owned(),
            }),
            ..DraftReview::pending()
        };
        let previous = assessment("edited", Some(review.clone()));
        let merged = carry_draft(
            Some(&previous),
            assessment("new", Some(DraftReview::pending())),
            later(),
        );
        assert_eq!(merged.summary, "new");
        assert_eq!(merged.draft.unwrap().subject, "edited");
        let kept = merged.draft_review.unwrap();
        assert_eq!(kept.status, DraftStatus::Pending);
        assert_eq!(kept.edited_at, review.edited_at);
        assert_eq!(kept.original, review.original);
        assert_eq!(kept.suggestion.unwrap().subject, "new");
        assert_eq!(kept.outdated_at, Some(later()));
    }

    #[test]
    fn a_decided_draft_is_carried_untouched() {
        let approved = DraftReview::pending().approve(user(), at()).unwrap();
        let previous = assessment("approved", Some(approved.clone()));
        let merged = carry_draft(
            Some(&previous),
            assessment("new", Some(DraftReview::pending())),
            later(),
        );
        assert_eq!(merged.summary, "new");
        assert_eq!(merged.draft.unwrap().subject, "approved");
        assert_eq!(merged.draft_review, Some(approved));
    }

    #[test]
    fn approving_records_the_reviewer() {
        let approved = DraftReview::pending().approve(user(), at()).unwrap();
        assert_eq!(approved.status, DraftStatus::Approved);
        assert_eq!(approved.reviewed_by, Some(user()));
        assert_eq!(approved.reviewed_at, Some(at()));
        assert_eq!(approved.rejection_reason, None);
    }

    #[test]
    fn rejecting_keeps_the_reason() {
        let rejected = DraftReview::pending()
            .reject(user(), at(), Some("off topic".to_owned()))
            .unwrap();
        assert_eq!(rejected.status, DraftStatus::Rejected);
        assert_eq!(rejected.rejection_reason.as_deref(), Some("off topic"));
    }

    #[test]
    fn a_decided_draft_cannot_be_decided_again() {
        let approved = DraftReview::pending().approve(user(), at()).unwrap();
        assert_eq!(
            approved.approve(user(), at()),
            Err(InvalidDraftTransition {
                from: DraftStatus::Approved,
                to: DraftStatus::Approved
            })
        );
        assert!(approved.reject(user(), at(), None).is_err());
    }

    #[test]
    fn an_edit_needs_a_field() {
        let problems = DraftEdit::new(None, None).unwrap_err();
        assert_eq!(problems[0].field, "$");
    }

    #[test]
    fn edit_bounds_are_enforced_in_characters() {
        assert!(DraftEdit::new(Some("é".repeat(MAX_SUBJECT_CHARS)), None).is_ok());
        let problems = DraftEdit::new(
            Some("é".repeat(MAX_SUBJECT_CHARS + 1)),
            Some("  ".to_owned()),
        )
        .unwrap_err();
        let fields: Vec<_> = problems.iter().map(|problem| problem.field).collect();
        assert_eq!(fields, ["subject", "body"]);
        assert!(DraftEdit::new(None, Some("x".repeat(MAX_BODY_CHARS + 1))).is_err());
        assert!(DraftEdit::new(None, Some("x".repeat(MAX_BODY_CHARS))).is_ok());
    }

    #[test]
    fn reasons_are_trimmed_and_bounded() {
        assert_eq!(normalize_reason(None), Ok(None));
        assert_eq!(normalize_reason(Some("   ")), Ok(None));
        assert_eq!(
            normalize_reason(Some(" no fit ")),
            Ok(Some("no fit".to_owned()))
        );
        assert!(normalize_reason(Some(&"x".repeat(MAX_REASON_CHARS + 1))).is_err());
    }
}
