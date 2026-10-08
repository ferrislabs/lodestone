use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, NaiveDate, Utc};

use crate::domain::{
    ecosystem::{EcosystemModule, EcosystemReadiness, ModuleState},
    lead::{Signal, str_enum},
    profile::definition::ProfileDefinition,
    values::SourceName,
};

pub const CERTIFICATE_SEEN_KIND: &str = "certificate_seen";
pub const DEFAULT_REQUALIFY_DEBOUNCE_SECONDS: u64 = 120;
pub const DEFAULT_REQUALIFY_MAX_PER_DAY: u32 = 3;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequalifyPolicy {
    pub debounce: Duration,
    pub max_per_day: u32,
}

impl RequalifyPolicy {
    pub fn new(debounce: Duration, max_per_day: u32) -> Self {
        Self {
            debounce,
            max_per_day,
        }
    }

    pub fn from_std(debounce: StdDuration, max_per_day: u32) -> Self {
        Self {
            debounce: Duration::from_std(debounce).unwrap_or_else(|_| Duration::zero()),
            max_per_day,
        }
    }

    pub fn requalify_after(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        now.checked_add_signed(self.debounce).unwrap_or(now)
    }
}

impl Default for RequalifyPolicy {
    fn default() -> Self {
        Self {
            debounce: Duration::zero(),
            max_per_day: DEFAULT_REQUALIFY_MAX_PER_DAY,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Requalification {
    pub after: Option<DateTime<Utc>>,
    pub forced: bool,
    pub daily_count: u32,
    pub counted_on: Option<NaiveDate>,
}

impl Requalification {
    pub fn due_at(&self, max_per_day: u32) -> Option<DateTime<Utc>> {
        let after = self.after?;
        let capped_until = self
            .counted_on
            .filter(|day| {
                !self.forced && self.daily_count >= max_per_day && after.date_naive() <= *day
            })
            .and_then(|day| day.succ_opt())
            .and_then(|day| day.and_hms_opt(0, 0, 0))
            .map(|midnight| midnight.and_utc());
        Some(capped_until.unwrap_or(after))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectionRecord {
    pub enabled: bool,
    pub hosting_platform: Option<&'static str>,
    pub modules: Vec<ModuleState>,
    pub readiness: EcosystemReadiness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedEnrichment {
    pub record: InspectionRecord,
    pub only_certificates: bool,
    pub evaluated_at: Option<DateTime<Utc>>,
    pub requalification: Option<Requalification>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrichmentState {
    pub modules: Vec<ModuleState>,
    pub hosting_platform: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluationStage {
    NotEvaluated,
    CertificateOnly,
    RequalificationPlanned,
    InspectionRunning,
    InspectionComplete,
    Evaluated,
}

str_enum!(EvaluationStage {
    NotEvaluated => "not_evaluated",
    CertificateOnly => "certificate_only",
    RequalificationPlanned => "requalification_planned",
    InspectionRunning => "inspection_running",
    InspectionComplete => "inspection_complete",
    Evaluated => "evaluated",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationStatus {
    pub stage: EvaluationStage,
    pub evaluated_at: Option<DateTime<Utc>>,
    pub modules_done: u32,
    pub modules_available: u32,
    pub requalify_at: Option<DateTime<Utc>>,
    pub requalify_forced: bool,
    pub changed_modules: Vec<ModuleState>,
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

fn all_modules(record: &InspectionRecord) -> Vec<ModuleState> {
    EcosystemModule::AVAILABLE
        .into_iter()
        .map(|module| {
            record
                .modules
                .iter()
                .find(|state| state.module == module)
                .cloned()
                .unwrap_or_else(|| ModuleState::pending(module))
        })
        .collect()
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

pub fn enrichment_state(record: &InspectionRecord) -> EnrichmentState {
    EnrichmentState {
        modules: all_modules(record),
        hosting_platform: record.hosting_platform,
    }
}

pub fn evaluation_status(listed: &ListedEnrichment, max_per_day: u32) -> EvaluationStatus {
    let record = &listed.record;
    let modules = all_modules(record);
    let ready: Vec<&ModuleState> = modules
        .iter()
        .filter(|state| record.enabled && record.readiness.is_ready(state.module))
        .collect();
    let modules_available = count(ready.len());
    let modules_done = count(
        ready
            .iter()
            .filter(|state| state.status.is_settled())
            .count(),
    );
    let requalify_at = listed
        .requalification
        .as_ref()
        .and_then(|requalification| requalification.due_at(max_per_day));
    let changed_modules = listed
        .evaluated_at
        .map(|evaluated| {
            modules
                .iter()
                .filter(|state| state.changed_at.is_some_and(|changed| changed > evaluated))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let stage = if listed.only_certificates {
        EvaluationStage::CertificateOnly
    } else if listed.evaluated_at.is_none() {
        EvaluationStage::NotEvaluated
    } else if requalify_at.is_some() {
        EvaluationStage::RequalificationPlanned
    } else if !record.enabled {
        EvaluationStage::Evaluated
    } else if modules_done >= modules_available {
        EvaluationStage::InspectionComplete
    } else {
        EvaluationStage::InspectionRunning
    };
    EvaluationStatus {
        stage,
        evaluated_at: listed.evaluated_at,
        modules_done,
        modules_available,
        requalify_at,
        requalify_forced: listed
            .requalification
            .as_ref()
            .is_some_and(|requalification| requalification.forced && requalify_at.is_some()),
        changed_modules,
    }
}

#[cfg(test)]
mod tests;
