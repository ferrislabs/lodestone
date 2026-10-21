use std::num::NonZeroU16;

use chrono::{DateTime, Utc};
use common::CoreError;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::domain::{
    job::{JobKind, NewJob},
    lead::{Assessment, ContactEmail, Lead, LeadId, Signal},
    profile::ProfileId,
};

pub mod ports;

pub const WEBHOOK_JOB_KIND: &str = "webhook.deliver";

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ProfileRecord {
    pub id: Uuid,
    pub name: String,
    pub revision: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ContactRecord {
    pub address: String,
    pub method: String,
    pub confidence: f64,
    pub provenance: String,
}

impl From<&ContactEmail> for ContactRecord {
    fn from(contact: &ContactEmail) -> Self {
        Self {
            address: contact.address.as_str().to_owned(),
            method: contact.method.as_str().to_owned(),
            confidence: contact.confidence.get(),
            provenance: contact.provenance.as_str().to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct EvidenceRecord {
    pub quote: String,
    pub provenance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct DraftOutput {
    pub subject: String,
    pub body: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AssessmentRecord {
    pub summary: String,
    pub approach_angle: String,
    pub evidence: Vec<EvidenceRecord>,
    pub draft: Option<DraftOutput>,
}

impl From<&Assessment> for AssessmentRecord {
    fn from(assessment: &Assessment) -> Self {
        Self {
            summary: assessment.summary.clone(),
            approach_angle: assessment.approach_angle.clone(),
            evidence: assessment
                .evidence
                .iter()
                .map(|evidence| EvidenceRecord {
                    quote: evidence.quote.clone(),
                    provenance: evidence.provenance.as_str().to_owned(),
                })
                .collect(),
            draft: assessment.draft.as_ref().map(|draft| DraftOutput {
                subject: draft.subject.clone(),
                body: draft.body.clone(),
                status: assessment
                    .draft_review
                    .as_ref()
                    .map_or(crate::domain::lead::draft::DraftStatus::Pending, |review| {
                        review.status
                    })
                    .as_str()
                    .to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SignalRecord {
    pub kind: String,
    pub value: Option<String>,
    pub source: String,
    pub provenance: String,
    pub observed_at: DateTime<Utc>,
}

impl From<&Signal> for SignalRecord {
    fn from(signal: &Signal) -> Self {
        Self {
            kind: signal.kind.as_str().to_owned(),
            value: signal.value.clone(),
            source: signal.source.as_str().to_owned(),
            provenance: signal.provenance.as_str().to_owned(),
            observed_at: signal.observed_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct LeadRecord {
    pub lead_id: Uuid,
    pub profile: ProfileRecord,
    pub domain: String,
    pub status: String,
    /// Signal strength in percent from 0 to 100: mean of the three strongest independent signals, each given by its source and decayed by its age.
    pub score: f64,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub contact: Option<ContactRecord>,
    pub assessment: Option<AssessmentRecord>,
    pub signals: Vec<SignalRecord>,
}

impl LeadRecord {
    pub fn assemble(
        lead: &Lead,
        profile: ProfileRecord,
        contact: Option<&ContactEmail>,
        assessment: Option<&Assessment>,
        signals: &[Signal],
    ) -> Self {
        Self {
            lead_id: lead.id.0,
            profile,
            domain: lead.domain.as_str().to_owned(),
            status: lead.status.as_str().to_owned(),
            score: lead.score.get(),
            first_seen_at: lead.first_seen_at,
            last_seen_at: lead.last_seen_at,
            contact: contact.map(ContactRecord::from),
            assessment: assessment.map(AssessmentRecord::from),
            signals: signals.iter().map(SignalRecord::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeadEventKind {
    #[serde(rename = "lead.validated")]
    Validated,
}

impl LeadEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validated => "lead.validated",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeadEvent {
    pub kind: LeadEventKind,
    pub occurred_at: DateTime<Utc>,
    pub record: LeadRecord,
}

#[derive(Debug, Error)]
pub enum SinkError {
    #[error("the destination rejected the event, it can be retried: {0}")]
    Retryable(String),
    #[error("the destination is permanently gone")]
    Gone,
    #[error("the event could not be delivered: {0}")]
    Failed(String),
}

pub trait LeadSink {
    fn deliver(&self, event: &LeadEvent) -> impl Future<Output = Result<(), SinkError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookPolicy {
    pub profile_ids: Vec<ProfileId>,
    pub max_attempts: NonZeroU16,
}

impl WebhookPolicy {
    pub fn applies_to(&self, profile_id: ProfileId) -> bool {
        self.profile_ids.is_empty() || self.profile_ids.contains(&profile_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookJobPayload {
    pub lead_id: Uuid,
    pub event: LeadEventKind,
    pub occurred_at: DateTime<Utc>,
}

impl WebhookJobPayload {
    pub fn from_json(value: &serde_json::Value) -> Result<Self, serde_json::Error> {
        Self::deserialize(value)
    }

    pub fn lead_id(&self) -> LeadId {
        LeadId(self.lead_id)
    }
}

pub fn webhook_job(
    lead_id: LeadId,
    event: LeadEventKind,
    occurred_at: DateTime<Utc>,
    max_attempts: NonZeroU16,
) -> Result<NewJob, CoreError> {
    let kind = JobKind::parse(WEBHOOK_JOB_KIND)
        .map_err(|_| CoreError::Internal("invalid webhook job kind".to_owned()))?;
    let payload = serde_json::to_value(WebhookJobPayload {
        lead_id: lead_id.0,
        event,
        occurred_at,
    })
    .map_err(|error| CoreError::Internal(error.to_string()))?;
    Ok(NewJob {
        kind,
        payload,
        run_at: occurred_at,
        max_attempts,
        dedup_key: Some(format!(
            "webhook:{}:{}:{}",
            event.as_str(),
            lead_id,
            occurred_at.timestamp_micros()
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(ids: Vec<ProfileId>) -> WebhookPolicy {
        WebhookPolicy {
            profile_ids: ids,
            max_attempts: NonZeroU16::MIN,
        }
    }

    #[test]
    fn an_empty_filter_applies_to_every_profile() {
        assert!(policy(vec![]).applies_to(ProfileId(Uuid::from_u128(1))));
    }

    #[test]
    fn a_filter_applies_only_to_listed_profiles() {
        let listed = ProfileId(Uuid::from_u128(1));
        let other = ProfileId(Uuid::from_u128(2));
        let policy = policy(vec![listed]);
        assert!(policy.applies_to(listed));
        assert!(!policy.applies_to(other));
    }

    #[test]
    fn the_job_payload_round_trips() {
        let at = Utc::now();
        let id = LeadId(Uuid::from_u128(9));
        let job = webhook_job(id, LeadEventKind::Validated, at, NonZeroU16::MIN).unwrap();
        assert_eq!(job.kind.as_str(), "webhook.deliver");
        assert_eq!(job.payload["event"], "lead.validated");
        let payload: WebhookJobPayload = serde_json::from_value(job.payload).unwrap();
        assert_eq!(payload.lead_id(), id);
        assert_eq!(payload.occurred_at, at);
    }
}
