use std::time::Duration as StdDuration;

use chrono::TimeZone;
use serde_json::Value;
use uuid::Uuid;

use super::*;
use crate::domain::{
    ecosystem::ModuleStatus,
    lead::{LeadId, SignalId},
    values::{Provenance, SignalKind, UnitInterval},
};

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 8, hour, minute, 0).unwrap()
}

fn signal(kind: &str, source: SourceName) -> Signal {
    Signal {
        id: SignalId(Uuid::nil()),
        lead_id: LeadId(Uuid::nil()),
        kind: SignalKind::parse(kind).unwrap(),
        value: None,
        source,
        raw: Value::Null,
        provenance: Provenance::parse("https://example.com").unwrap(),
        strength: UnitInterval::new(0.3).unwrap(),
        observed_at: at(9, 0),
        agent: None,
    }
}

fn state(module: EcosystemModule, status: ModuleStatus, changed: Option<u32>) -> ModuleState {
    ModuleState {
        module,
        status,
        last_run: Some(at(9, 5)),
        changed_at: changed.map(|minute| at(9, minute)),
        error: None,
    }
}

fn listed(modules: &[ModuleState], evaluated: Option<u32>) -> ListedEnrichment {
    ListedEnrichment {
        record: InspectionRecord {
            enabled: true,
            hosting_platform: None,
            modules: modules.to_vec(),
            readiness: EcosystemReadiness::default(),
        },
        only_certificates: false,
        evaluated_at: evaluated.map(|minute| at(9, minute)),
        requalification: None,
    }
}

fn done(count: usize) -> Vec<ModuleState> {
    EcosystemModule::AVAILABLE
        .into_iter()
        .take(count)
        .map(|module| state(module, ModuleStatus::Done, None))
        .collect()
}

#[test]
fn a_lead_is_not_evaluated_until_its_first_qualification() {
    let status = evaluation_status(&listed(&done(3), None), 3);
    assert_eq!(status.stage, EvaluationStage::NotEvaluated);
    assert_eq!(status.modules_done, 3);
    assert_eq!(status.modules_available, 7);
    assert!(status.changed_modules.is_empty());
}

#[test]
fn an_evaluated_lead_shows_the_inspection_progress() {
    let running = evaluation_status(&listed(&done(3), Some(10)), 3);
    assert_eq!(running.stage, EvaluationStage::InspectionRunning);
    assert_eq!((running.modules_done, running.modules_available), (3, 7));
    assert_eq!(running.evaluated_at, Some(at(9, 10)));
    let complete = evaluation_status(&listed(&done(7), Some(10)), 3);
    assert_eq!(complete.stage, EvaluationStage::InspectionComplete);
    let mut not_ready = listed(&done(6), Some(10));
    not_ready.record.readiness =
        EcosystemReadiness::with_not_ready(vec![EcosystemModule::RecentActivity]);
    let without_provider = evaluation_status(&not_ready, 3);
    assert_eq!(without_provider.stage, EvaluationStage::InspectionComplete);
    assert_eq!(without_provider.modules_available, 6);
    let mut disabled = listed(&[], Some(10));
    disabled.record.enabled = false;
    let off = evaluation_status(&disabled, 3);
    assert_eq!(off.stage, EvaluationStage::Evaluated);
    assert_eq!((off.modules_done, off.modules_available), (0, 0));
}

#[test]
fn modules_with_new_findings_after_the_evaluation_are_listed() {
    let modules = [
        state(
            EcosystemModule::OfficialIdentity,
            ModuleStatus::Done,
            Some(5),
        ),
        state(EcosystemModule::DnsEmail, ModuleStatus::Done, Some(20)),
        state(
            EcosystemModule::LegalLife,
            ModuleStatus::NotApplicable,
            None,
        ),
    ];
    let status = evaluation_status(&listed(&modules, Some(10)), 3);
    let changed: Vec<EcosystemModule> = status
        .changed_modules
        .iter()
        .map(|state| state.module)
        .collect();
    assert_eq!(changed, vec![EcosystemModule::DnsEmail]);
}

#[test]
fn a_pending_requalification_is_shown_with_its_time() {
    let mut planned = listed(&done(4), Some(10));
    planned.requalification = Some(Requalification {
        after: Some(at(9, 32)),
        forced: false,
        daily_count: 1,
        counted_on: Some(at(9, 0).date_naive()),
    });
    let status = evaluation_status(&planned, 3);
    assert_eq!(status.stage, EvaluationStage::RequalificationPlanned);
    assert_eq!(status.requalify_at, Some(at(9, 32)));
    assert!(!status.requalify_forced);
    planned.requalification = Some(Requalification {
        after: None,
        forced: false,
        daily_count: 3,
        counted_on: Some(at(9, 0).date_naive()),
    });
    assert_eq!(
        evaluation_status(&planned, 3).stage,
        EvaluationStage::InspectionRunning
    );
}

#[test]
fn the_daily_cap_moves_a_requalification_to_the_next_day_unless_forced() {
    let today = at(9, 0).date_naive();
    let capped = Requalification {
        after: Some(at(14, 0)),
        forced: false,
        daily_count: 3,
        counted_on: Some(today),
    };
    let midnight = Utc.with_ymd_and_hms(2026, 10, 9, 0, 0, 0).unwrap();
    assert_eq!(capped.due_at(3), Some(midnight));
    assert_eq!(capped.due_at(4), Some(at(14, 0)));
    let forced = Requalification {
        forced: true,
        ..capped
    };
    assert_eq!(forced.due_at(3), Some(at(14, 0)));
    let yesterday = Requalification {
        counted_on: today.pred_opt(),
        ..capped
    };
    assert_eq!(yesterday.due_at(3), Some(at(14, 0)));
    let marked_before = Requalification {
        after: Some(Utc.with_ymd_and_hms(2026, 10, 7, 23, 0, 0).unwrap()),
        ..capped
    };
    assert_eq!(marked_before.due_at(3), Some(midnight));
}

#[test]
fn a_certificate_only_lead_waits_for_other_evidence() {
    let certificate = signal(CERTIFICATE_SEEN_KIND, SourceName::CertificateTransparency);
    let guess = signal("role_contact_found", SourceName::RoleGuess);
    assert!(only_certificates(&[certificate.clone(), guess]));
    let product = signal("product_detected", SourceName::HttpProbe);
    assert!(!only_certificates(&[certificate, product]));
    assert!(!only_certificates(&[]));
    let mut waiting = listed(&done(7), None);
    waiting.only_certificates = true;
    assert_eq!(
        evaluation_status(&waiting, 3).stage,
        EvaluationStage::CertificateOnly
    );
}

#[test]
fn the_requalify_policy_debounces_from_now() {
    let policy = RequalifyPolicy::new(Duration::seconds(120), 3);
    assert_eq!(policy.requalify_after(at(9, 0)), at(9, 2));
    assert_eq!(
        RequalifyPolicy::from_std(StdDuration::from_secs(120), 3),
        policy
    );
    assert_eq!(RequalifyPolicy::default().max_per_day, 3);
    assert_eq!(DEFAULT_REQUALIFY_DEBOUNCE_SECONDS, 120);
}

#[test]
fn the_enrichment_block_lists_every_available_module() {
    let record = listed(&done(2), None).record;
    let block = enrichment_state(&record);
    assert_eq!(block.modules.len(), EcosystemModule::AVAILABLE.len());
    assert_eq!(block.modules[2].status, ModuleStatus::Pending);
}
