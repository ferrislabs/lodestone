use chrono::{DateTime, Duration, Utc};

use crate::domain::{
    lead::{Signal, str_enum},
    profile::definition::Enrichment,
    values::SourceName,
};

pub mod ports;

pub const CERTIFICATE_SEEN_KIND: &str = "certificate_seen";

pub const CONTACT_SOURCES: [SourceName; 3] = [
    SourceName::SecurityTxt,
    SourceName::PublicPage,
    SourceName::RoleGuess,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnrichmentStep {
    HttpProbe,
    CompanyJobs,
}

str_enum!(EnrichmentStep {
    HttpProbe => "http_probe",
    CompanyJobs => "company_jobs",
});

impl EnrichmentStep {
    pub const ALL: [Self; 2] = [Self::HttpProbe, Self::CompanyJobs];

    pub fn enabled_in(self, enrichment: &Enrichment) -> bool {
        match self {
            Self::HttpProbe => enrichment.http_probe.enabled,
            Self::CompanyJobs => enrichment.company_jobs.enabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepRun {
    pub step: EnrichmentStep,
    pub ran_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeadEnrichmentRecord {
    pub created_at: DateTime<Utc>,
    pub runs: Vec<StepRun>,
    pub settings: Enrichment,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListedEnrichment {
    pub record: LeadEnrichmentRecord,
    pub only_certificates: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrichmentStatus {
    Disabled,
    Pending,
    Complete,
    TimedOut,
}

str_enum!(EnrichmentStatus {
    Disabled => "disabled",
    Pending => "pending",
    Complete => "complete",
    TimedOut => "timed_out",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitingReason {
    AwaitingEnrichment,
    AwaitingEnrichmentEvidence,
}

str_enum!(WaitingReason {
    AwaitingEnrichment => "awaiting_enrichment",
    AwaitingEnrichmentEvidence => "awaiting_enrichment_evidence",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepState {
    pub step: EnrichmentStep,
    pub enabled: bool,
    pub ran_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrichmentState {
    pub steps: Vec<StepState>,
    pub status: EnrichmentStatus,
    pub waiting_reason: Option<WaitingReason>,
    pub wait_until: Option<DateTime<Utc>>,
}

pub fn is_enrichment_evidence(signal: &Signal) -> bool {
    signal.kind.as_str() != CERTIFICATE_SEEN_KIND && !CONTACT_SOURCES.contains(&signal.source)
}

pub fn only_certificates(signals: &[Signal]) -> bool {
    signals
        .iter()
        .any(|signal| signal.kind.as_str() == CERTIFICATE_SEEN_KIND)
        && !signals.iter().any(is_enrichment_evidence)
}

pub fn enrichment_state(
    record: &LeadEnrichmentRecord,
    signals: &[Signal],
    wait: Duration,
    now: DateTime<Utc>,
) -> EnrichmentState {
    listed_enrichment_state(record, only_certificates(signals), wait, now)
}

pub fn listed_enrichment_state(
    record: &LeadEnrichmentRecord,
    certificates_only: bool,
    wait: Duration,
    now: DateTime<Utc>,
) -> EnrichmentState {
    let enrichment = &record.settings;
    let steps: Vec<StepState> = EnrichmentStep::ALL
        .into_iter()
        .map(|step| StepState {
            step,
            enabled: step.enabled_in(enrichment),
            ran_at: record
                .runs
                .iter()
                .find(|run| run.step == step)
                .map(|run| run.ran_at),
        })
        .collect();
    let enabled = steps.iter().filter(|state| state.enabled);
    let wait_until = record.created_at.checked_add_signed(wait);
    let status = if !enrichment.any_enabled() {
        EnrichmentStatus::Disabled
    } else if enabled.clone().all(|state| state.ran_at.is_some()) {
        EnrichmentStatus::Complete
    } else if wait_until.is_none_or(|until| now >= until) {
        EnrichmentStatus::TimedOut
    } else {
        EnrichmentStatus::Pending
    };
    let waiting_reason = if certificates_only {
        Some(WaitingReason::AwaitingEnrichmentEvidence)
    } else if status == EnrichmentStatus::Pending {
        Some(WaitingReason::AwaitingEnrichment)
    } else {
        None
    };
    EnrichmentState {
        steps,
        status,
        waiting_reason,
        wait_until: (status == EnrichmentStatus::Pending)
            .then_some(wait_until)
            .flatten(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::domain::{
        lead::{LeadId, SignalId},
        profile::definition::{CompanyJobsEnrichment, HttpProbeEnrichment},
        values::{Provenance, SignalKind, UnitInterval},
    };

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 8, 9, minute, 0).unwrap()
    }

    fn enrichment(http_probe: bool, company_jobs: bool) -> Enrichment {
        Enrichment {
            http_probe: HttpProbeEnrichment {
                enabled: http_probe,
                ..HttpProbeEnrichment::default()
            },
            company_jobs: CompanyJobsEnrichment {
                enabled: company_jobs,
                ..CompanyJobsEnrichment::default()
            },
        }
    }

    fn signal(kind: &str, source: SourceName) -> Signal {
        Signal {
            id: SignalId(uuid::Uuid::nil()),
            lead_id: LeadId(uuid::Uuid::nil()),
            kind: SignalKind::parse(kind).unwrap(),
            value: None,
            source,
            raw: serde_json::Value::Null,
            provenance: Provenance::parse("https://example.com").unwrap(),
            strength: UnitInterval::new(0.3).unwrap(),
            observed_at: at(0),
            agent: None,
        }
    }

    fn record(settings: Enrichment, runs: &[(EnrichmentStep, u32)]) -> LeadEnrichmentRecord {
        LeadEnrichmentRecord {
            settings,
            created_at: at(0),
            runs: runs
                .iter()
                .map(|(step, minute)| StepRun {
                    step: *step,
                    ran_at: at(*minute),
                })
                .collect(),
        }
    }

    #[test]
    fn a_lead_waits_for_its_enabled_steps_then_times_out() {
        let signals = [signal("public_tender", SourceName::Boamp)];
        let pending = enrichment_state(
            &record(enrichment(true, true), &[(EnrichmentStep::HttpProbe, 3)]),
            &signals,
            Duration::minutes(30),
            at(10),
        );
        assert_eq!(pending.status, EnrichmentStatus::Pending);
        assert_eq!(
            pending.waiting_reason,
            Some(WaitingReason::AwaitingEnrichment)
        );
        assert_eq!(pending.wait_until, Some(at(30)));
        assert_eq!(pending.steps[0].ran_at, Some(at(3)));
        assert_eq!(pending.steps[1].ran_at, None);
        let timed_out = enrichment_state(
            &record(enrichment(true, true), &[(EnrichmentStep::HttpProbe, 3)]),
            &signals,
            Duration::minutes(30),
            at(30),
        );
        assert_eq!(timed_out.status, EnrichmentStatus::TimedOut);
        assert_eq!(timed_out.waiting_reason, None);
        let complete = enrichment_state(
            &record(enrichment(true, false), &[(EnrichmentStep::HttpProbe, 3)]),
            &signals,
            Duration::minutes(30),
            at(10),
        );
        assert_eq!(complete.status, EnrichmentStatus::Complete);
        let disabled = enrichment_state(
            &record(enrichment(false, false), &[]),
            &signals,
            Duration::minutes(30),
            at(10),
        );
        assert_eq!(disabled.status, EnrichmentStatus::Disabled);
        assert_eq!(disabled.waiting_reason, None);
    }

    #[test]
    fn a_certificate_only_lead_waits_for_evidence_even_after_its_steps() {
        let certificate = signal(CERTIFICATE_SEEN_KIND, SourceName::CertificateTransparency);
        let guess = signal("role_contact_found", SourceName::RoleGuess);
        let state = enrichment_state(
            &record(enrichment(true, false), &[(EnrichmentStep::HttpProbe, 3)]),
            &[certificate.clone(), guess],
            Duration::minutes(30),
            at(40),
        );
        assert_eq!(state.status, EnrichmentStatus::Complete);
        assert_eq!(
            state.waiting_reason,
            Some(WaitingReason::AwaitingEnrichmentEvidence)
        );
        let listed = listed_enrichment_state(
            &record(enrichment(true, false), &[(EnrichmentStep::HttpProbe, 3)]),
            true,
            Duration::minutes(30),
            at(40),
        );
        assert_eq!(listed, state);
        let product = signal("product_detected", SourceName::HttpProbe);
        assert!(!only_certificates(&[certificate, product]));
        assert!(!only_certificates(&[]));
    }
}
