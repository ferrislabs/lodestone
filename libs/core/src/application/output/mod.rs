use chrono::Utc;
use common::CoreError;
use lodestone_macros::transactional;

use crate::{
    application::LodestoneUseCase,
    domain::{
        lead::{
            Lead, LeadId,
            ports::{AssessmentRepository, ContactRepository, LeadRepository, SignalRepository},
            query::LeadQuery,
        },
        output::{
            LeadEventKind, LeadRecord, ProfileRecord, WebhookPolicy, ports::JobOutbox, webhook_job,
        },
        profile::{ProfileId, ports::ProfileRepository},
        suppression::ports::SuppressionRepository,
    },
};

pub const MAX_EXPORT_ROWS: u32 = 10_000;

#[derive(Debug, Clone, PartialEq)]
pub struct LeadRecordSet {
    pub profile: ProfileRecord,
    pub total: u64,
    pub records: Vec<LeadRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecordLookup {
    Found(Box<LeadRecord>),
    Suppressed,
    Missing,
}

async fn record_for(
    lead: &Lead,
    profile: &ProfileRecord,
    signals: &mut impl SignalRepository,
    contacts: &mut impl ContactRepository,
    assessments: &mut impl AssessmentRepository,
    suppression: &mut impl SuppressionRepository,
) -> Result<Option<LeadRecord>, CoreError> {
    if suppression.is_domain_suppressed(&lead.domain).await? {
        return Ok(None);
    }
    let mut contact = None;
    for candidate in contacts.list_for_lead(lead.id).await? {
        if !suppression.is_email_suppressed(&candidate.address).await? {
            contact = Some(candidate);
            break;
        }
    }
    let assessments = assessments.list_for_lead(lead.id).await?;
    let signals = signals.list_for_lead(lead.id).await?;
    Ok(Some(LeadRecord::assemble(
        lead,
        profile.clone(),
        contact.as_ref(),
        assessments.first(),
        &signals,
    )))
}

async fn profile_record(
    profiles: &mut impl ProfileRepository,
    profile_id: ProfileId,
) -> Result<ProfileRecord, CoreError> {
    let profile = profiles
        .find(profile_id)
        .await?
        .ok_or(CoreError::NotFound)?;
    let revision = profiles
        .list_revisions(profile_id)
        .await?
        .iter()
        .map(|revision| revision.number)
        .max()
        .map(|number| number.get());
    Ok(ProfileRecord {
        id: profile.id.0,
        name: profile.name.as_str().to_owned(),
        revision,
    })
}

impl LodestoneUseCase {
    #[transactional(lead, profile, suppression, signal, contact, assessment)]
    pub async fn lead_records(
        &self,
        profile_id: ProfileId,
        query: LeadQuery,
    ) -> Result<LeadRecordSet, CoreError> {
        let mut leads = lead_repository;
        let mut profiles = profile_repository;
        let mut suppression = suppression_repository;
        let mut signals = signal_repository;
        let mut contacts = contact_repository;
        let mut assessments = assessment_repository;

        let profile = profile_record(&mut profiles, profile_id).await?;
        let query = LeadQuery {
            limit: MAX_EXPORT_ROWS,
            offset: 0,
            ..query
        };
        let page = leads.list_page(profile_id, &query).await?;
        if page.total > u64::from(MAX_EXPORT_ROWS) {
            return Err(CoreError::Invalid {
                field: "filters".to_owned(),
                message: format!(
                    "{} leads match, more than the {MAX_EXPORT_ROWS} an export holds, narrow the filters",
                    page.total
                ),
            });
        }
        let mut records = Vec::with_capacity(page.items.len());
        for lead in &page.items {
            if let Some(record) = record_for(
                lead,
                &profile,
                &mut signals,
                &mut contacts,
                &mut assessments,
                &mut suppression,
            )
            .await?
            {
                records.push(record);
            }
        }
        Ok(LeadRecordSet {
            profile,
            total: page.total,
            records,
        })
    }

    #[transactional(lead, profile, suppression, signal, contact, assessment)]
    pub async fn lead_record(&self, lead_id: LeadId) -> Result<RecordLookup, CoreError> {
        let mut leads = lead_repository;
        let mut profiles = profile_repository;
        let mut suppression = suppression_repository;
        let mut signals = signal_repository;
        let mut contacts = contact_repository;
        let mut assessments = assessment_repository;

        let Some(lead) = leads.find(lead_id).await? else {
            return Ok(RecordLookup::Missing);
        };
        let profile = profile_record(&mut profiles, lead.profile_id).await?;
        let record = record_for(
            &lead,
            &profile,
            &mut signals,
            &mut contacts,
            &mut assessments,
            &mut suppression,
        )
        .await?;
        Ok(record.map_or(RecordLookup::Suppressed, |record| {
            RecordLookup::Found(Box::new(record))
        }))
    }
}

pub(crate) async fn schedule_webhook(
    outbox: &mut impl JobOutbox,
    policy: Option<&WebhookPolicy>,
    lead: &Lead,
) -> Result<(), CoreError> {
    let Some(policy) = policy else {
        return Ok(());
    };
    if !policy.applies_to(lead.profile_id) {
        return Ok(());
    }
    let now = Utc::now();
    let job = webhook_job(lead.id, LeadEventKind::Validated, now, policy.max_attempts)?;
    outbox.enqueue(&job, now).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use chrono::TimeZone;
    use uuid::Uuid;

    use super::*;
    use crate::domain::{
        NormalizedDomain,
        lead::{
            Assessment, AssessmentId, ContactEmail, ContactId, ContactKind, ContactMethod,
            Evidence, LeadStatus, Score, Signal, SignalId,
            ports::{MockAssessmentRepository, MockContactRepository, MockSignalRepository},
        },
        output::ports::MockJobOutbox,
        profile::RevisionNumber,
        prompt::DraftMessage,
        suppression::ports::MockSuppressionRepository,
        values::{EmailAddress, Provenance, SignalKind, SourceName, UnitInterval},
    };

    fn at() -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000, 0).unwrap()
    }

    fn lead() -> Lead {
        Lead {
            id: LeadId(Uuid::from_u128(1)),
            profile_id: ProfileId(Uuid::from_u128(2)),
            domain: NormalizedDomain::from_host("acme.com").unwrap(),
            status: LeadStatus::Validated,
            score: Score::new(42.0).unwrap(),
            score_breakdown: vec![],
            confidence: None,
            verdict: None,
            qualified_at: None,
            first_seen_at: at(),
            last_seen_at: at(),
            updated_at: at(),
        }
    }

    fn profile() -> ProfileRecord {
        ProfileRecord {
            id: Uuid::from_u128(2),
            name: "alpha".to_owned(),
            revision: Some(3),
        }
    }

    fn contact(address: &str) -> ContactEmail {
        ContactEmail {
            id: ContactId(Uuid::from_u128(5)),
            lead_id: LeadId(Uuid::from_u128(1)),
            address: EmailAddress::parse(address).unwrap(),
            provenance: Provenance::parse("https://acme.com/contact").unwrap(),
            method: ContactMethod::PublicPage,
            confidence: UnitInterval::new(0.8).unwrap(),
            kind: ContactKind::Role,
            found_at: at(),
        }
    }

    fn signal() -> Signal {
        Signal {
            id: SignalId(Uuid::from_u128(6)),
            lead_id: LeadId(Uuid::from_u128(1)),
            kind: SignalKind::parse("product_detected").unwrap(),
            value: Some("keycloak".to_owned()),
            source: SourceName::HttpProbe,
            raw: serde_json::Value::Null,
            provenance: Provenance::parse("https://acme.com/realms/master").unwrap(),
            strength: UnitInterval::new(1.0).unwrap(),
            observed_at: at(),
            agent: None,
        }
    }

    fn assessment() -> Assessment {
        Assessment {
            id: AssessmentId(Uuid::from_u128(7)),
            lead_id: LeadId(Uuid::from_u128(1)),
            profile_revision: RevisionNumber::new(3).unwrap(),
            summary: "Runs Keycloak".to_owned(),
            approach_angle: "Managed IAM".to_owned(),
            evidence: vec![Evidence {
                quote: "Powered by Keycloak".to_owned(),
                provenance: Provenance::parse("https://acme.com/about").unwrap(),
            }],
            model: "m".to_owned(),
            prompt_version: "qualification@1".to_owned(),
            draft: Some(DraftMessage {
                subject: "Hello".to_owned(),
                body: "Body".to_owned(),
            }),
            draft_review: None,
            created_at: at(),
        }
    }

    struct Mocks {
        signals: MockSignalRepository,
        contacts: MockContactRepository,
        assessments: MockAssessmentRepository,
        suppression: MockSuppressionRepository,
    }

    fn mocks(
        contacts: Vec<ContactEmail>,
        domain_suppressed: bool,
        suppressed_email: &str,
    ) -> Mocks {
        let mut mocks = Mocks {
            signals: MockSignalRepository::new(),
            contacts: MockContactRepository::new(),
            assessments: MockAssessmentRepository::new(),
            suppression: MockSuppressionRepository::new(),
        };
        mocks
            .signals
            .expect_list_for_lead()
            .returning(|_| Box::pin(async { Ok(vec![signal()]) }));
        mocks.contacts.expect_list_for_lead().returning(move |_| {
            let contacts = contacts.clone();
            Box::pin(async move { Ok(contacts) })
        });
        mocks
            .assessments
            .expect_list_for_lead()
            .returning(|_| Box::pin(async { Ok(vec![assessment()]) }));
        mocks
            .suppression
            .expect_is_domain_suppressed()
            .returning(move |_| Box::pin(async move { Ok(domain_suppressed) }));
        let suppressed_email = suppressed_email.to_owned();
        mocks
            .suppression
            .expect_is_email_suppressed()
            .returning(move |email| {
                let hit = email.as_str() == suppressed_email;
                Box::pin(async move { Ok(hit) })
            });
        mocks
    }

    async fn build(mut m: Mocks) -> Option<LeadRecord> {
        record_for(
            &lead(),
            &profile(),
            &mut m.signals,
            &mut m.contacts,
            &mut m.assessments,
            &mut m.suppression,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn a_suppressed_contact_address_is_dropped_for_the_next_one() {
        let record = build(mocks(
            vec![contact("security@acme.com"), contact("contact@acme.com")],
            false,
            "security@acme.com",
        ))
        .await
        .unwrap();
        assert_eq!(record.contact.unwrap().address, "contact@acme.com");
    }

    #[tokio::test]
    async fn every_contact_suppressed_leaves_the_record_without_contact() {
        let record = build(mocks(
            vec![contact("security@acme.com")],
            false,
            "security@acme.com",
        ))
        .await
        .unwrap();
        assert!(record.contact.is_none());
    }

    #[tokio::test]
    async fn a_suppressed_domain_drops_the_record_even_if_the_sql_filter_missed_it() {
        let record = build(mocks(vec![contact("security@acme.com")], true, "")).await;
        assert!(record.is_none());
    }

    #[tokio::test]
    async fn the_record_carries_the_stable_shape() {
        let record = build(mocks(vec![contact("security@acme.com")], false, ""))
            .await
            .unwrap();
        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(value["lead_id"], "00000000-0000-0000-0000-000000000001");
        assert_eq!(
            value["profile"]["id"],
            "00000000-0000-0000-0000-000000000002"
        );
        assert_eq!(value["profile"]["name"], "alpha");
        assert_eq!(value["profile"]["revision"], 3);
        assert_eq!(value["domain"], "acme.com");
        assert_eq!(value["status"], "validated");
        assert_eq!(value["score"], 42.0);
        assert_eq!(value["contact"]["method"], "public_page");
        assert_eq!(value["assessment"]["summary"], "Runs Keycloak");
        assert_eq!(value["assessment"]["draft"]["status"], "pending");
        assert_eq!(
            value["assessment"]["evidence"][0]["quote"],
            "Powered by Keycloak"
        );
        assert_eq!(value["signals"][0]["kind"], "product_detected");
        assert_eq!(value["signals"][0]["source"], "http_probe");
        assert!(value.get("updated_at").is_none());
    }

    fn policy(ids: Vec<ProfileId>) -> WebhookPolicy {
        WebhookPolicy {
            profile_ids: ids,
            max_attempts: NonZeroU16::new(5).unwrap(),
        }
    }

    #[tokio::test]
    async fn a_webhook_is_scheduled_when_enabled_and_the_profile_matches() {
        let mut outbox = MockJobOutbox::new();
        outbox.expect_enqueue().times(1).returning(|job, _| {
            assert_eq!(job.kind.as_str(), "webhook.deliver");
            assert_eq!(job.max_attempts.get(), 5);
            Box::pin(async { Ok(None) })
        });
        schedule_webhook(&mut outbox, Some(&policy(vec![lead().profile_id])), &lead())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn no_webhook_is_scheduled_when_disabled_or_the_profile_is_filtered_out() {
        let mut outbox = MockJobOutbox::new();
        outbox.expect_enqueue().never();
        schedule_webhook(&mut outbox, None, &lead()).await.unwrap();
        let other = ProfileId(Uuid::from_u128(99));
        schedule_webhook(&mut outbox, Some(&policy(vec![other])), &lead())
            .await
            .unwrap();
    }
}
