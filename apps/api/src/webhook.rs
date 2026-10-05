use std::{
    fmt::{self, Write as _},
    time::Duration,
};

use aws_lc_rs::{digest, hmac};
use chrono::{DateTime, Utc};
use common::Secret;
use lodestone_core::{
    Job, LeadEvent, LeadRecord, LeadSink, LodestoneUseCase, SinkError, WebhookJobPayload,
    jobs::{JobError, JobOutcome},
    output::RecordLookup,
};
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use serde::Serialize;
use tracing::{info, warn};
use uuid::{Builder, Uuid};

pub const SIGNATURE_HEADER: &str = "Lodestone-Signature";
pub const EVENT_ID_HEADER: &str = "Lodestone-Event-Id";
pub const SIGNATURE_TOLERANCE_SECONDS: i64 = 300;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).ok())
        .collect()
}

fn signed_payload(timestamp: i64, body: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(body.len() + 21);
    payload.extend_from_slice(timestamp.to_string().as_bytes());
    payload.push(b'.');
    payload.extend_from_slice(body);
    payload
}

pub fn signature(secret: &[u8], timestamp: i64, body: &[u8]) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    hex(hmac::sign(&key, &signed_payload(timestamp, body)).as_ref())
}

pub fn signature_header(secret: &[u8], timestamp: i64, body: &[u8]) -> String {
    format!("t={timestamp},v1={}", signature(secret, timestamp, body))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    Malformed,
    Expired,
    Mismatch,
}

pub fn verify(
    secret: &[u8],
    header: &str,
    body: &[u8],
    now: i64,
    tolerance_seconds: i64,
) -> Result<(), VerifyError> {
    let mut timestamp = None;
    let mut candidates = Vec::new();
    for part in header.split(',') {
        match part.trim().split_once('=') {
            Some(("t", value)) => {
                timestamp = Some(value.parse::<i64>().map_err(|_| VerifyError::Malformed)?);
            }
            Some(("v1", value)) => candidates.push(value),
            _ => {}
        }
    }
    let timestamp = timestamp.ok_or(VerifyError::Malformed)?;
    if candidates.is_empty() {
        return Err(VerifyError::Malformed);
    }
    if now.abs_diff(timestamp) > tolerance_seconds.unsigned_abs() {
        return Err(VerifyError::Expired);
    }
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let payload = signed_payload(timestamp, body);
    let matched = candidates.iter().any(|candidate| {
        unhex(candidate).is_some_and(|tag| hmac::verify(&key, &payload, &tag).is_ok())
    });
    if matched {
        Ok(())
    } else {
        Err(VerifyError::Mismatch)
    }
}

pub fn event_id(event: &LeadEvent) -> Uuid {
    let mut context = digest::Context::new(&digest::SHA256);
    context.update(event.kind.as_str().as_bytes());
    context.update(b"|");
    context.update(event.record.lead_id.as_bytes());
    context.update(b"|");
    context.update(&event.occurred_at.timestamp_micros().to_be_bytes());
    let hash = context.finish();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash.as_ref()[..16]);
    Builder::from_custom_bytes(bytes).into_uuid()
}

#[derive(Serialize)]
struct Envelope<'a> {
    id: Uuid,
    #[serde(rename = "type")]
    kind: &'static str,
    occurred_at: DateTime<Utc>,
    profile_id: Uuid,
    data: &'a LeadRecord,
}

pub fn envelope_body(event: &LeadEvent) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&Envelope {
        id: event_id(event),
        kind: event.kind.as_str(),
        occurred_at: event.occurred_at,
        profile_id: event.record.profile.id,
        data: &event.record,
    })
}

pub struct WebhookSink {
    client: Client,
    url: Url,
    secret: Secret,
}

impl fmt::Debug for WebhookSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebhookSink")
            .field("host", &self.url.host_str())
            .field("secret", &self.secret)
            .finish()
    }
}

impl WebhookSink {
    pub fn new(url: Url, secret: Secret, timeout: Duration) -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .timeout(timeout)
            .redirect(Policy::none())
            .build()?;
        Ok(Self {
            client,
            url,
            secret,
        })
    }
}

impl LeadSink for WebhookSink {
    async fn deliver(&self, event: &LeadEvent) -> Result<(), SinkError> {
        let body = envelope_body(event).map_err(|error| SinkError::Failed(error.to_string()))?;
        let header = signature_header(
            self.secret.expose().as_bytes(),
            Utc::now().timestamp(),
            &body,
        );
        let response = self
            .client
            .post(self.url.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(SIGNATURE_HEADER, header)
            .header(EVENT_ID_HEADER, event_id(event).to_string())
            .body(body)
            .send()
            .await
            .map_err(|error| SinkError::Retryable(error.without_url().to_string()))?;
        match response.status() {
            StatusCode::GONE => Err(SinkError::Gone),
            status if status.is_success() => Ok(()),
            status => Err(SinkError::Retryable(format!(
                "the endpoint answered {status}"
            ))),
        }
    }
}

pub async fn run_webhook_job(
    usecase: &LodestoneUseCase,
    sink: Option<&impl LeadSink>,
    job: &Job,
) -> Result<JobOutcome, JobError> {
    let payload = WebhookJobPayload::from_json(&job.payload)
        .map_err(|error| JobError::Permanent(format!("invalid webhook payload: {error}")))?;
    let Some(sink) = sink else {
        info!(lead.id = %payload.lead_id, "webhooks are disabled, the stale delivery completes without sending");
        return Ok(JobOutcome::Done);
    };
    let record = match usecase.lead_record(payload.lead_id()).await {
        Ok(RecordLookup::Found(record)) => *record,
        Ok(RecordLookup::Suppressed) => {
            info!(lead.id = %payload.lead_id, "lead is suppressed, delivery skipped");
            return Ok(JobOutcome::Done);
        }
        Ok(RecordLookup::Missing) => {
            warn!(lead.id = %payload.lead_id, "lead no longer exists, delivery skipped");
            return Ok(JobOutcome::Done);
        }
        Err(error) => return Err(JobError::Retryable(error.to_string())),
    };
    let event = LeadEvent {
        kind: payload.event,
        occurred_at: payload.occurred_at,
        record,
    };
    match sink.deliver(&event).await {
        Ok(()) => {
            info!(lead.id = %payload.lead_id, "webhook delivered");
            Ok(JobOutcome::Done)
        }
        Err(SinkError::Gone) => {
            warn!(lead.id = %payload.lead_id, "webhook endpoint answered 410 Gone, the endpoint is disabled and the delivery dropped");
            Ok(JobOutcome::Done)
        }
        Err(SinkError::Retryable(reason)) => Err(JobError::Retryable(reason)),
        Err(SinkError::Failed(reason)) => Err(JobError::Permanent(reason)),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use lodestone_core::{LeadEventKind, ProfileRecord};

    use super::*;

    const SECRET: &[u8] = b"lodestone-test-secret-0123456789abcdef";
    const BODY: &[u8] = br#"{"id":"evt","type":"lead.validated"}"#;
    const TIMESTAMP: i64 = 1_700_000_000;
    const KNOWN_VECTOR: &str = "55e3574c6573aab0445bd67049ef4801f14bc4d0401fd7616903086e186dccf2";

    fn reference_hmac(key: &[u8], message: &[u8]) -> String {
        const BLOCK: usize = 64;
        let mut padded = [0u8; BLOCK];
        padded[..key.len()].copy_from_slice(key);
        let inner_key: Vec<u8> = padded.iter().map(|byte| byte ^ 0x36).collect();
        let outer_key: Vec<u8> = padded.iter().map(|byte| byte ^ 0x5c).collect();
        let mut inner = digest::Context::new(&digest::SHA256);
        inner.update(&inner_key);
        inner.update(message);
        let inner = inner.finish();
        let mut outer = digest::Context::new(&digest::SHA256);
        outer.update(&outer_key);
        outer.update(inner.as_ref());
        hex(outer.finish().as_ref())
    }

    #[test]
    fn the_signature_matches_a_known_vector() {
        assert_eq!(signature(SECRET, TIMESTAMP, BODY), KNOWN_VECTOR);
    }

    #[test]
    fn the_signature_matches_an_independent_hmac_implementation() {
        let mut message = format!("{TIMESTAMP}.").into_bytes();
        message.extend_from_slice(BODY);
        assert_eq!(
            signature(SECRET, TIMESTAMP, BODY),
            reference_hmac(SECRET, &message)
        );
    }

    #[test]
    fn the_header_carries_the_timestamp_and_the_v1_signature() {
        assert_eq!(
            signature_header(SECRET, TIMESTAMP, BODY),
            format!("t={TIMESTAMP},v1={KNOWN_VECTOR}")
        );
    }

    #[test]
    fn a_signed_body_verifies() {
        let header = signature_header(SECRET, TIMESTAMP, BODY);
        assert_eq!(
            verify(
                SECRET,
                &header,
                BODY,
                TIMESTAMP + 60,
                SIGNATURE_TOLERANCE_SECONDS
            ),
            Ok(())
        );
    }

    #[test]
    fn a_tampered_body_fails() {
        let header = signature_header(SECRET, TIMESTAMP, BODY);
        assert_eq!(
            verify(
                SECRET,
                &header,
                b"{}",
                TIMESTAMP,
                SIGNATURE_TOLERANCE_SECONDS
            ),
            Err(VerifyError::Mismatch)
        );
    }

    #[test]
    fn a_tampered_timestamp_or_wrong_secret_fails() {
        let header = signature_header(SECRET, TIMESTAMP, BODY);
        let moved = header.replace(&TIMESTAMP.to_string(), &(TIMESTAMP + 1).to_string());
        assert_eq!(
            verify(SECRET, &moved, BODY, TIMESTAMP, SIGNATURE_TOLERANCE_SECONDS),
            Err(VerifyError::Mismatch)
        );
        assert_eq!(
            verify(
                b"another-secret-another-secret-another",
                &header,
                BODY,
                TIMESTAMP,
                SIGNATURE_TOLERANCE_SECONDS
            ),
            Err(VerifyError::Mismatch)
        );
    }

    #[test]
    fn a_stale_or_future_timestamp_is_refused() {
        let header = signature_header(SECRET, TIMESTAMP, BODY);
        assert_eq!(
            verify(
                SECRET,
                &header,
                BODY,
                TIMESTAMP + 301,
                SIGNATURE_TOLERANCE_SECONDS
            ),
            Err(VerifyError::Expired)
        );
        assert_eq!(
            verify(
                SECRET,
                &header,
                BODY,
                TIMESTAMP - 301,
                SIGNATURE_TOLERANCE_SECONDS
            ),
            Err(VerifyError::Expired)
        );
    }

    #[test]
    fn malformed_headers_are_refused() {
        for header in [
            "",
            "v1=abcd",
            "t=1700000000",
            "t=x,v1=ab",
            "t=1700000000,v1=zz",
        ] {
            assert!(
                verify(SECRET, header, BODY, TIMESTAMP, SIGNATURE_TOLERANCE_SECONDS).is_err(),
                "{header}"
            );
        }
    }

    #[test]
    fn a_rotated_secret_can_present_several_v1_signatures() {
        let header = format!(
            "t={TIMESTAMP},v1=00,v1={}",
            signature(SECRET, TIMESTAMP, BODY)
        );
        assert_eq!(
            verify(
                SECRET,
                &header,
                BODY,
                TIMESTAMP,
                SIGNATURE_TOLERANCE_SECONDS
            ),
            Ok(())
        );
    }

    fn event() -> LeadEvent {
        let at = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        LeadEvent {
            kind: LeadEventKind::Validated,
            occurred_at: at,
            record: LeadRecord {
                lead_id: Uuid::from_u128(1),
                profile: ProfileRecord {
                    id: Uuid::from_u128(2),
                    name: "alpha".to_owned(),
                    revision: Some(1),
                },
                domain: "acme.com".to_owned(),
                status: "validated".to_owned(),
                score: 1.0,
                score_breakdown: vec![],
                first_seen_at: at,
                last_seen_at: at,
                contact: None,
                assessment: None,
                signals: vec![],
            },
        }
    }

    #[test]
    fn the_event_id_is_stable_for_the_same_event_and_differs_otherwise() {
        let first = event();
        let mut later = event();
        later.occurred_at += chrono::Duration::seconds(1);
        assert_eq!(event_id(&first), event_id(&event()));
        assert_ne!(event_id(&first), event_id(&later));
    }

    #[test]
    fn the_body_is_the_documented_envelope() {
        let value: serde_json::Value =
            serde_json::from_slice(&envelope_body(&event()).unwrap()).unwrap();
        assert_eq!(value["type"], "lead.validated");
        assert_eq!(value["id"], event_id(&event()).to_string());
        assert_eq!(value["profile_id"], "00000000-0000-0000-0000-000000000002");
        assert_eq!(value["data"]["domain"], "acme.com");
        assert!(value["occurred_at"].is_string());
    }

    #[test]
    fn debug_never_prints_the_secret_or_the_url_path() {
        let sink = WebhookSink::new(
            "https://crm.example.com/hook/token-abc".parse().unwrap(),
            Secret::new("super-secret-value-super-secret-value"),
            Duration::from_secs(1),
        )
        .unwrap();
        let printed = format!("{sink:?}");
        assert!(!printed.contains("super-secret-value"));
        assert!(!printed.contains("token-abc"));
    }
}
