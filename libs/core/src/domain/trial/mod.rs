use std::num::NonZeroU32;

use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::domain::{
    NormalizedDomain, UserId,
    lead::{LeadId, str_enum, uuid_id},
    profile::{
        ActiveProfile, Profile, ProfileId, ProfileRevision, RevisionNumber, ValidatedProfile,
    },
    qualification::{Judgement, Verdict},
    scoring_agent::Decision,
};

pub mod ports;

uuid_id!(TrialId);

pub const MIN_SAMPLE_SIZE: u8 = 1;
pub const MAX_SAMPLE_SIZE: u8 = 50;
pub const DEFAULT_SAMPLE_SIZE: u8 = 20;
pub const MIN_BUDGET_CENTS: u32 = 1;
pub const MAX_BUDGET_CENTS: u32 = 2000;
pub const DEFAULT_BUDGET_CENTS: u32 = 200;
pub const TRIAL_LIST_LIMIT: u32 = 20;
pub const NO_DECIDED_LEAD: &str = "no decided lead to try";
pub const TRIAL_ALREADY_LIVE: &str = "a trial is already pending or running for this profile";
pub const NO_LLM_PROVIDER: &str = "no LLM provider is configured";
pub const NO_VALID_ANSWER: &str = "no agent of the draft gave a valid answer";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TrialLimitError {
    #[error("sample size must be between {MIN_SAMPLE_SIZE} and {MAX_SAMPLE_SIZE}")]
    SampleSize,
    #[error("budget must be between {MIN_BUDGET_CENTS} and {MAX_BUDGET_CENTS} cents")]
    Budget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleSize(u8);

impl SampleSize {
    pub const DEFAULT: Self = Self(DEFAULT_SAMPLE_SIZE);

    pub fn new(raw: u32) -> Result<Self, TrialLimitError> {
        u8::try_from(raw)
            .ok()
            .filter(|size| (MIN_SAMPLE_SIZE..=MAX_SAMPLE_SIZE).contains(size))
            .map(Self)
            .ok_or(TrialLimitError::SampleSize)
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrialBudget(NonZeroU32);

impl TrialBudget {
    pub const DEFAULT: Self = match NonZeroU32::new(DEFAULT_BUDGET_CENTS) {
        Some(cents) => Self(cents),
        None => panic!("the default budget is not zero"),
    };

    pub fn new(raw: u32) -> Result<Self, TrialLimitError> {
        if !(MIN_BUDGET_CENTS..=MAX_BUDGET_CENTS).contains(&raw) {
            return Err(TrialLimitError::Budget);
        }
        NonZeroU32::new(raw)
            .map(Self)
            .ok_or(TrialLimitError::Budget)
    }

    pub fn cents(self) -> NonZeroU32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrialStatus {
    Pending,
    Running,
    Done,
    Failed,
    BudgetExhausted,
}

str_enum!(TrialStatus {
    Pending => "pending",
    Running => "running",
    Done => "done",
    Failed => "failed",
    BudgetExhausted => "budget_exhausted",
});

impl TrialStatus {
    pub fn is_final(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::BudgetExhausted)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrialLabel {
    Validated,
    Rejected,
}

str_enum!(TrialLabel {
    Validated => "validated",
    Rejected => "rejected",
});

#[derive(Debug, Clone, PartialEq)]
pub struct NewTrial {
    pub profile_id: ProfileId,
    pub definition: ValidatedProfile,
    pub sample_size: SampleSize,
    pub budget: TrialBudget,
    pub created_by: Option<UserId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileTrial {
    pub id: TrialId,
    pub profile_id: ProfileId,
    pub base_revision: RevisionNumber,
    pub definition: ValidatedProfile,
    pub status: TrialStatus,
    pub sample_size: SampleSize,
    pub budget: TrialBudget,
    pub spent_cents: f64,
    pub leads_tried: u32,
    pub error: Option<String>,
    pub created_by: Option<UserId>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl ProfileTrial {
    pub fn pending(
        id: TrialId,
        base_revision: RevisionNumber,
        new: NewTrial,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            profile_id: new.profile_id,
            base_revision,
            definition: new.definition,
            status: TrialStatus::Pending,
            sample_size: new.sample_size,
            budget: new.budget,
            spent_cents: 0.0,
            leads_tried: 0,
            error: None,
            created_by: new.created_by,
            created_at,
            started_at: None,
            finished_at: None,
        }
    }

    pub fn trial_profile(&self, profile: &Profile, base: &ProfileRevision) -> ActiveProfile {
        ActiveProfile {
            profile: Profile {
                active: true,
                ..profile.clone()
            },
            revision: ProfileRevision {
                definition: self.definition.clone(),
                ..base.clone()
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecidedLead {
    pub lead_id: LeadId,
    pub label: TrialLabel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub verdict: Option<Verdict>,
    pub confidence: Option<f64>,
}

impl Reading {
    pub fn of(decision: &Decision, judgement: &Judgement) -> Self {
        if decision.verdict.is_some() {
            Self {
                verdict: decision.verdict,
                confidence: decision.confidence,
            }
        } else {
            Self {
                verdict: judgement.verdict,
                confidence: judgement.confidence,
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrialLead {
    pub lead_id: LeadId,
    pub domain: NormalizedDomain,
    pub label: TrialLabel,
    pub current_verdict: Option<Verdict>,
    pub current_confidence: Option<f64>,
    pub trial_verdict: Option<Verdict>,
    pub trial_confidence: Option<f64>,
    pub error: Option<String>,
    pub tried_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VersionSummary {
    pub reliable: u64,
    pub reliable_validated: u64,
    pub rejected: u64,
    pub rejected_discarded: u64,
}

impl VersionSummary {
    pub fn precision(&self) -> Option<f64> {
        (self.reliable > 0).then(|| self.reliable_validated as f64 / self.reliable as f64)
    }

    fn count(&mut self, label: TrialLabel, verdict: Option<Verdict>) {
        if verdict == Some(Verdict::Reliable) {
            self.reliable += 1;
            if label == TrialLabel::Validated {
                self.reliable_validated += 1;
            }
        }
        if label == TrialLabel::Rejected {
            self.rejected += 1;
            if verdict == Some(Verdict::Discarded) {
                self.rejected_discarded += 1;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrialSummary {
    pub current: VersionSummary,
    pub trial: VersionSummary,
    pub changed: u64,
    pub errors: u64,
}

impl TrialSummary {
    pub fn of(leads: &[TrialLead]) -> Self {
        let mut summary = Self::default();
        for lead in leads {
            if lead.error.is_some() {
                summary.errors += 1;
                continue;
            }
            summary.current.count(lead.label, lead.current_verdict);
            summary.trial.count(lead.label, lead.trial_verdict);
            if lead.current_verdict != lead.trial_verdict {
                summary.changed += 1;
            }
        }
        summary
    }
}

pub fn select_sample(decided: &[DecidedLead], size: SampleSize) -> Vec<DecidedLead> {
    let wanted = usize::from(size.get());
    let validated = decided
        .iter()
        .filter(|lead| lead.label == TrialLabel::Validated)
        .count();
    let rejected = decided.len() - validated;
    let from_validated = validated.min(wanted.div_ceil(2));
    let from_rejected = rejected.min(wanted - from_validated);
    let from_validated = validated.min(wanted - from_rejected);
    let (mut kept_validated, mut kept_rejected) = (0, 0);
    decided
        .iter()
        .filter(|lead| {
            let (kept, quota) = match lead.label {
                TrialLabel::Validated => (&mut kept_validated, from_validated),
                TrialLabel::Rejected => (&mut kept_rejected, from_rejected),
            };
            let keep = *kept < quota;
            *kept += usize::from(keep);
            keep
        })
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use uuid::Uuid;

    use super::*;
    use crate::domain::{
        profile::fixtures::MINIMAL_PROFILE,
        qualification::{EvidenceComponents, EvidenceStrength},
    };

    fn lead_id(n: u128) -> LeadId {
        LeadId(Uuid::from_u128(n))
    }

    fn decided(n: u128, label: TrialLabel) -> DecidedLead {
        DecidedLead {
            lead_id: lead_id(n),
            label,
        }
    }

    fn size(raw: u32) -> SampleSize {
        SampleSize::new(raw).unwrap()
    }

    fn ids(sample: &[DecidedLead]) -> Vec<u128> {
        sample.iter().map(|lead| lead.lead_id.0.as_u128()).collect()
    }

    fn tried(
        label: TrialLabel,
        current: Option<Verdict>,
        trial: Option<Verdict>,
        error: Option<&str>,
    ) -> TrialLead {
        TrialLead {
            lead_id: lead_id(1),
            domain: NormalizedDomain::from_host("acme.com").unwrap(),
            label,
            current_verdict: current,
            current_confidence: None,
            trial_verdict: trial,
            trial_confidence: None,
            error: error.map(str::to_owned),
            tried_at: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
        }
    }

    fn evidence() -> EvidenceStrength {
        EvidenceStrength {
            score: 0.5,
            components: EvidenceComponents {
                volume: 0.5,
                diversity: 0.5,
                environment: 0.5,
                strength: 0.5,
            },
            signal_count: 1,
            signal_score: 1.0,
            only_non_production: false,
            independent_signals: 1,
            decayed_signals: 1.0,
            source_count: 1,
            kind_count: 1,
        }
    }

    fn judgement(verdict: Option<Verdict>, confidence: Option<f64>) -> Judgement {
        Judgement {
            opinion: confidence,
            evidence: evidence(),
            confidence,
            verdict,
            cap: None,
        }
    }

    fn decision(verdict: Option<Verdict>, confidence: Option<f64>) -> Decision {
        Decision {
            score: None,
            verdict,
            confidence,
            probabilities: None,
            reason: None,
            exclusions: Vec::new(),
            contributors: 0,
        }
    }

    #[test]
    fn the_sample_takes_half_validated_then_fills_with_rejected() {
        let decided: Vec<DecidedLead> = (1..=10)
            .map(|n| decided(n, TrialLabel::Validated))
            .chain((11..=20).map(|n| decided(n, TrialLabel::Rejected)))
            .collect();

        let sample = select_sample(&decided, size(6));

        assert_eq!(ids(&sample), [1, 2, 3, 11, 12, 13]);
    }

    #[test]
    fn an_odd_size_rounds_the_validated_half_up() {
        let decided: Vec<DecidedLead> = (1..=10)
            .map(|n| decided(n, TrialLabel::Validated))
            .chain((11..=20).map(|n| decided(n, TrialLabel::Rejected)))
            .collect();

        let sample = select_sample(&decided, size(5));

        assert_eq!(ids(&sample), [1, 2, 3, 11, 12]);
    }

    #[test]
    fn rejected_leads_fill_the_sample_when_validated_ones_run_short() {
        let decided = [
            decided(1, TrialLabel::Validated),
            decided(2, TrialLabel::Rejected),
            decided(3, TrialLabel::Rejected),
            decided(4, TrialLabel::Rejected),
            decided(5, TrialLabel::Rejected),
        ];

        let sample = select_sample(&decided, size(4));

        assert_eq!(ids(&sample), [1, 2, 3, 4]);
    }

    #[test]
    fn validated_leads_fill_the_sample_when_rejected_ones_run_short() {
        let decided = [
            decided(1, TrialLabel::Validated),
            decided(2, TrialLabel::Validated),
            decided(3, TrialLabel::Validated),
            decided(4, TrialLabel::Validated),
            decided(5, TrialLabel::Rejected),
        ];

        let sample = select_sample(&decided, size(4));

        assert_eq!(ids(&sample), [1, 2, 3, 5]);
    }

    #[test]
    fn the_sample_keeps_the_newest_first_order_and_never_exceeds_the_decided_leads() {
        let decided = [
            decided(9, TrialLabel::Rejected),
            decided(8, TrialLabel::Validated),
        ];

        assert_eq!(ids(&select_sample(&decided, size(50))), [9, 8]);
        assert_eq!(ids(&select_sample(&decided, size(1))), [8]);
        assert!(select_sample(&[], size(20)).is_empty());
    }

    #[test]
    fn the_sample_is_deterministic() {
        let decided: Vec<DecidedLead> = (1..=30)
            .map(|n| {
                decided(
                    n,
                    if n % 3 == 0 {
                        TrialLabel::Rejected
                    } else {
                        TrialLabel::Validated
                    },
                )
            })
            .collect();

        assert_eq!(
            select_sample(&decided, size(12)),
            select_sample(&decided, size(12))
        );
    }

    #[test]
    fn limits_are_checked() {
        assert_eq!(SampleSize::new(0), Err(TrialLimitError::SampleSize));
        assert_eq!(SampleSize::new(51), Err(TrialLimitError::SampleSize));
        assert_eq!(SampleSize::new(50).unwrap().get(), 50);
        assert_eq!(SampleSize::new(1).unwrap().get(), 1);
        assert_eq!(TrialBudget::new(0), Err(TrialLimitError::Budget));
        assert_eq!(TrialBudget::new(2001), Err(TrialLimitError::Budget));
        assert_eq!(TrialBudget::new(2000).unwrap().cents().get(), 2000);
        assert_eq!(TrialBudget::new(1).unwrap().cents().get(), 1);
        assert_eq!(SampleSize::DEFAULT.get(), 20);
        assert_eq!(TrialBudget::DEFAULT.cents().get(), 200);
    }

    #[test]
    fn statuses_round_trip_and_only_three_are_final() {
        for status in [
            TrialStatus::Pending,
            TrialStatus::Running,
            TrialStatus::Done,
            TrialStatus::Failed,
            TrialStatus::BudgetExhausted,
        ] {
            assert_eq!(TrialStatus::parse(status.as_str()), Some(status));
        }
        assert!(TrialStatus::Done.is_final());
        assert!(TrialStatus::Failed.is_final());
        assert!(TrialStatus::BudgetExhausted.is_final());
        assert!(!TrialStatus::Pending.is_final());
        assert!(!TrialStatus::Running.is_final());
        assert_eq!(TrialStatus::parse("other"), None);
    }

    #[test]
    fn the_summary_counts_reliable_leads_and_their_precision_per_version() {
        use TrialLabel::{Rejected, Validated};
        use Verdict::{Discarded, Reliable, ToCheck};
        let leads = [
            tried(Validated, Some(Reliable), Some(Reliable), None),
            tried(Rejected, Some(Reliable), Some(Discarded), None),
            tried(Validated, Some(ToCheck), Some(Reliable), None),
            tried(Rejected, Some(Discarded), Some(ToCheck), None),
        ];

        let summary = TrialSummary::of(&leads);

        assert_eq!(
            summary.current,
            VersionSummary {
                reliable: 2,
                reliable_validated: 1,
                rejected: 2,
                rejected_discarded: 1,
            }
        );
        assert_eq!(
            summary.trial,
            VersionSummary {
                reliable: 2,
                reliable_validated: 2,
                rejected: 2,
                rejected_discarded: 1,
            }
        );
        assert_eq!(summary.current.precision(), Some(0.5));
        assert_eq!(summary.trial.precision(), Some(1.0));
        assert_eq!(summary.changed, 3);
        assert_eq!(summary.errors, 0);
    }

    #[test]
    fn precision_is_absent_without_a_reliable_lead() {
        use TrialLabel::Rejected;
        use Verdict::Discarded;
        let summary = TrialSummary::of(&[tried(Rejected, Some(Discarded), Some(Discarded), None)]);

        assert_eq!(summary.current.precision(), None);
        assert_eq!(summary.trial.precision(), None);
        assert_eq!(summary.changed, 0);
        assert_eq!(TrialSummary::of(&[]), TrialSummary::default());
    }

    #[test]
    fn a_lead_with_an_error_is_counted_apart_and_left_out_of_both_versions() {
        use TrialLabel::Validated;
        use Verdict::Reliable;
        let summary = TrialSummary::of(&[
            tried(Validated, Some(Reliable), Some(Reliable), None),
            tried(Validated, Some(Reliable), None, Some("model call failed")),
        ]);

        assert_eq!(summary.errors, 1);
        assert_eq!(summary.current.reliable, 1);
        assert_eq!(summary.trial.reliable, 1);
        assert_eq!(summary.changed, 0);
    }

    #[test]
    fn a_missing_verdict_on_either_side_is_a_change() {
        use TrialLabel::Validated;
        use Verdict::Reliable;
        let summary = TrialSummary::of(&[
            tried(Validated, None, Some(Reliable), None),
            tried(Validated, Some(Reliable), None, None),
            tried(Validated, None, None, None),
        ]);

        assert_eq!(summary.changed, 2);
    }

    #[test]
    fn the_scoring_decision_wins_when_it_gives_a_verdict() {
        let reading = Reading::of(
            &decision(Some(Verdict::Reliable), Some(80.0)),
            &judgement(Some(Verdict::Discarded), Some(10.0)),
        );

        assert_eq!(reading.verdict, Some(Verdict::Reliable));
        assert_eq!(reading.confidence, Some(80.0));
    }

    #[test]
    fn the_indicators_judgement_is_the_fallback() {
        let reading = Reading::of(
            &decision(None, None),
            &judgement(Some(Verdict::ToCheck), Some(55.0)),
        );

        assert_eq!(reading.verdict, Some(Verdict::ToCheck));
        assert_eq!(reading.confidence, Some(55.0));
        let none = Reading::of(&decision(None, None), &judgement(None, None));
        assert_eq!(none.verdict, None);
        assert_eq!(none.confidence, None);
    }

    #[test]
    fn the_trial_profile_carries_the_draft_under_the_base_revision() {
        let base = ValidatedProfile::parse(serde_json::from_str(MINIMAL_PROFILE).unwrap()).unwrap();
        let mut draft: serde_json::Value = serde_json::from_str(MINIMAL_PROFILE).unwrap();
        draft["identity"]["offer"] = serde_json::json!("Draft offer");
        let draft = ValidatedProfile::parse(draft).unwrap();
        let at = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        let profile = Profile {
            id: ProfileId(Uuid::from_u128(7)),
            name: crate::domain::profile::ProfileName::parse("iam").unwrap(),
            active: false,
            created_at: at,
            updated_at: at,
        };
        let revision = ProfileRevision {
            profile_id: profile.id,
            number: RevisionNumber::new(3).unwrap(),
            definition: base.clone(),
            created_by: None,
            created_at: at,
        };
        let trial = ProfileTrial::pending(
            TrialId(Uuid::from_u128(1)),
            revision.number,
            NewTrial {
                profile_id: profile.id,
                definition: draft.clone(),
                sample_size: SampleSize::DEFAULT,
                budget: TrialBudget::DEFAULT,
                created_by: None,
            },
            at,
        );

        let active = trial.trial_profile(&profile, &revision);

        assert!(active.profile.active);
        assert_eq!(active.profile.id, profile.id);
        assert_eq!(active.revision.number, revision.number);
        assert_eq!(active.revision.definition, draft);
        assert_ne!(active.revision.definition, base);
        assert_eq!(trial.status, TrialStatus::Pending);
        assert_eq!(trial.leads_tried, 0);
    }
}
