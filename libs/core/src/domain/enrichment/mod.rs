use chrono::{DateTime, Duration, Utc};

use crate::domain::{
    ecosystem::{EcosystemModule, EcosystemReadiness, ModuleState},
    lead::{Signal, str_enum},
    profile::definition::ProfileDefinition,
    values::SourceName,
};

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

    pub fn enabled_in(self, definition: &ProfileDefinition) -> bool {
        match self {
            Self::HttpProbe => definition.probes_hosts(),
            Self::CompanyJobs => definition.reads_company_jobs(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectionRecord {
    pub created_at: DateTime<Utc>,
    pub enabled: bool,
    pub hosting_platform: Option<&'static str>,
    pub modules: Vec<ModuleState>,
    pub readiness: EcosystemReadiness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedEnrichment {
    pub record: InspectionRecord,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrichmentState {
    pub modules: Vec<ModuleState>,
    pub hosting_platform: Option<&'static str>,
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
    record: &InspectionRecord,
    signals: &[Signal],
    wait: Duration,
    now: DateTime<Utc>,
) -> EnrichmentState {
    listed_enrichment_state(record, only_certificates(signals), wait, now)
}

pub fn listed_enrichment_state(
    record: &InspectionRecord,
    certificates_only: bool,
    wait: Duration,
    now: DateTime<Utc>,
) -> EnrichmentState {
    let modules: Vec<ModuleState> = EcosystemModule::AVAILABLE
        .into_iter()
        .map(|module| {
            record
                .modules
                .iter()
                .find(|state| state.module == module)
                .cloned()
                .unwrap_or_else(|| ModuleState::pending(module))
        })
        .collect();
    let wait_until = record.created_at.checked_add_signed(wait);
    let status = if !record.enabled {
        EnrichmentStatus::Disabled
    } else if modules
        .iter()
        .all(|state| state.status.is_settled() || !record.readiness.is_ready(state.module))
    {
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
        modules,
        hosting_platform: record.hosting_platform,
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
        ecosystem::ModuleStatus,
        lead::{LeadId, SignalId},
        values::{Provenance, SignalKind, UnitInterval},
    };

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 8, 9, minute, 0).unwrap()
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

    fn state(module: EcosystemModule, status: ModuleStatus, minute: u32) -> ModuleState {
        ModuleState {
            module,
            status,
            last_run: Some(at(minute)),
            error: None,
        }
    }

    fn record(enabled: bool, modules: &[ModuleState]) -> InspectionRecord {
        InspectionRecord {
            created_at: at(0),
            enabled,
            hosting_platform: None,
            modules: modules.to_vec(),
            readiness: EcosystemReadiness::default(),
        }
    }

    #[test]
    fn a_lead_waits_for_every_available_module_then_times_out() {
        let signals = [signal("public_tender", SourceName::Boamp)];
        let partial = [state(
            EcosystemModule::OfficialIdentity,
            ModuleStatus::Done,
            3,
        )];
        let pending = enrichment_state(
            &record(true, &partial),
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
        assert_eq!(pending.modules.len(), EcosystemModule::AVAILABLE.len());
        assert_eq!(pending.modules[0].last_run, Some(at(3)));
        assert_eq!(pending.modules[1].status, ModuleStatus::Pending);
        let timed_out = enrichment_state(
            &record(true, &partial),
            &signals,
            Duration::minutes(30),
            at(30),
        );
        assert_eq!(timed_out.status, EnrichmentStatus::TimedOut);
        assert_eq!(timed_out.waiting_reason, None);
        let settled = [
            state(EcosystemModule::OfficialIdentity, ModuleStatus::Done, 3),
            state(EcosystemModule::LegalLife, ModuleStatus::NotApplicable, 3),
            state(EcosystemModule::PublicContracts, ModuleStatus::Done, 3),
            state(
                EcosystemModule::WebsiteStack,
                ModuleStatus::NotApplicable,
                4,
            ),
            state(EcosystemModule::HiringActivity, ModuleStatus::Failed, 5),
        ];
        let complete = enrichment_state(
            &record(true, &settled),
            &signals,
            Duration::minutes(30),
            at(10),
        );
        assert_eq!(complete.status, EnrichmentStatus::Complete);
        let without_bodacc = [
            state(EcosystemModule::OfficialIdentity, ModuleStatus::Done, 3),
            state(EcosystemModule::PublicContracts, ModuleStatus::Done, 3),
            state(EcosystemModule::WebsiteStack, ModuleStatus::Done, 4),
            state(EcosystemModule::HiringActivity, ModuleStatus::Done, 5),
        ];
        let mut waiting_record = record(true, &without_bodacc);
        let still_waiting =
            enrichment_state(&waiting_record, &signals, Duration::minutes(30), at(10));
        assert_eq!(still_waiting.status, EnrichmentStatus::Pending);
        waiting_record.readiness =
            EcosystemReadiness::with_not_ready(vec![EcosystemModule::LegalLife]);
        let not_ready = enrichment_state(&waiting_record, &signals, Duration::minutes(30), at(10));
        assert_eq!(not_ready.status, EnrichmentStatus::Complete);
        assert_eq!(not_ready.waiting_reason, None);
        assert_eq!(
            not_ready
                .modules
                .iter()
                .find(|module| module.module == EcosystemModule::LegalLife)
                .map(|module| module.status),
            Some(ModuleStatus::Pending)
        );
        let disabled =
            enrichment_state(&record(false, &[]), &signals, Duration::minutes(30), at(10));
        assert_eq!(disabled.status, EnrichmentStatus::Disabled);
        assert_eq!(disabled.waiting_reason, None);
    }

    #[test]
    fn a_certificate_only_lead_waits_for_evidence_even_after_its_modules() {
        let certificate = signal(CERTIFICATE_SEEN_KIND, SourceName::CertificateTransparency);
        let guess = signal("role_contact_found", SourceName::RoleGuess);
        let settled: Vec<ModuleState> = EcosystemModule::AVAILABLE
            .into_iter()
            .map(|module| state(module, ModuleStatus::Done, 3))
            .collect();
        let state = enrichment_state(
            &record(true, &settled),
            &[certificate.clone(), guess],
            Duration::minutes(30),
            at(40),
        );
        assert_eq!(state.status, EnrichmentStatus::Complete);
        assert_eq!(
            state.waiting_reason,
            Some(WaitingReason::AwaitingEnrichmentEvidence)
        );
        let listed =
            listed_enrichment_state(&record(true, &settled), true, Duration::minutes(30), at(40));
        assert_eq!(listed, state);
        let product = signal("product_detected", SourceName::HttpProbe);
        assert!(!only_certificates(&[certificate, product]));
        assert!(!only_certificates(&[]));
    }
}
