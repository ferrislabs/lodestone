use std::{collections::BTreeMap, num::NonZeroU16};

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::transactional;
use serde_json::{Value, json};
use tracing::info;

use crate::{
    application::{LodestoneUseCase, jobs::JobError, timeline::exclusion_lifted},
    domain::{
        NormalizedDomain,
        company::{CompanyDossier, OfficialData, ports::CompanyRepository},
        job::{Job, JobKind, NewJob},
        lead::{LeadId, ports::LeadRepository},
        profile::{ActiveProfile, ProfileId, RevisionNumber, definition::TargetCompanies},
        suppression::ports::SuppressionRepository,
        targeting::{Ruling, ruling, target_exclusion},
        timeline::ports::TimelineRepository,
    },
};

pub const TARGETING_SWEEP_JOB_KIND: &str = "targeting.sweep";
const SWEEP_MAX_ATTEMPTS: NonZeroU16 = NonZeroU16::new(3).unwrap();
const SWEEP_BATCH: u32 = 500;

pub fn sweep_job(
    profile_id: ProfileId,
    revision: RevisionNumber,
    run_at: DateTime<Utc>,
) -> Result<NewJob, CoreError> {
    let kind = JobKind::parse(TARGETING_SWEEP_JOB_KIND)
        .map_err(|_| CoreError::Internal("invalid targeting sweep job kind".to_owned()))?;
    Ok(NewJob {
        kind,
        payload: json!({ "profile_id": profile_id.to_string() }),
        run_at,
        max_attempts: SWEEP_MAX_ATTEMPTS,
        dedup_key: Some(format!("targeting:sweep:{profile_id}:{}", revision.get())),
    })
}

pub fn profile_of_job(job: &Job) -> Result<ProfileId, JobError> {
    job.payload
        .get("profile_id")
        .and_then(Value::as_str)
        .ok_or_else(|| JobError::Permanent("targeting sweep job without a profile".to_owned()))?
        .parse()
        .map_err(|error: uuid::Error| JobError::Permanent(error.to_string()))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TargetSweepReport {
    pub excluded: u64,
    pub restored: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SweepStep {
    read: usize,
    last: Option<LeadId>,
    excluded: u64,
    restored: u64,
}

type Groups = BTreeMap<String, Vec<LeadId>>;

async fn exclude_by_reason<S, T>(
    suppression: &mut S,
    timeline: &mut T,
    groups: &Groups,
    now: DateTime<Utc>,
) -> Result<u64, CoreError>
where
    S: SuppressionRepository,
    T: TimelineRepository,
{
    let mut excluded = 0;
    for (reason, lead_ids) in groups {
        let count = suppression.exclude_leads(lead_ids, reason, now).await?;
        if count > 0 {
            timeline.record_exclusions(reason, now).await?;
        }
        excluded += count;
    }
    Ok(excluded)
}

impl LodestoneUseCase {
    pub async fn exclude_off_target(
        &self,
        dossier: &CompanyDossier,
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let Some(official) = &dossier.official else {
            return Ok(0);
        };
        let profiles = self.list_active_profiles().await?;
        let excluded = self
            .exclude_leads_off_target(&dossier.domain, official, &profiles, now)
            .await?;
        if excluded > 0 {
            info!(domain = %dossier.domain, excluded, "leads outside the target companies suppressed");
        }
        Ok(excluded)
    }

    pub async fn sweep_target_companies(
        &self,
        profile_id: ProfileId,
        now: DateTime<Utc>,
    ) -> Result<TargetSweepReport, CoreError> {
        self.sweep_target_companies_by(profile_id, now, SWEEP_BATCH)
            .await
    }

    pub async fn sweep_target_companies_by(
        &self,
        profile_id: ProfileId,
        now: DateTime<Utc>,
        batch: u32,
    ) -> Result<TargetSweepReport, CoreError> {
        let mut report = TargetSweepReport::default();
        let Some(current) = self
            .list_current_profiles()
            .await?
            .into_iter()
            .find(|current| current.profile.id == profile_id)
        else {
            return Ok(report);
        };
        let rules = &current.revision.definition.definition().target_companies;
        let mut after = None;
        loop {
            let step = self
                .sweep_target_batch(profile_id, rules, after, batch, now)
                .await?;
            report.excluded += step.excluded;
            report.restored += step.restored;
            match step.last {
                Some(last) if step.read == usize::try_from(batch).unwrap_or(usize::MAX) => {
                    after = Some(last);
                }
                _ => return Ok(report),
            }
        }
    }

    #[transactional(lead, suppression, timeline)]
    async fn exclude_leads_off_target(
        &self,
        domain: &NormalizedDomain,
        official: &OfficialData,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut leads = lead_repository;
        let mut suppression = suppression_repository;
        let mut timeline = timeline_repository;
        let mut groups = Groups::new();
        for lead in leads.list_by_domain(domain).await? {
            let Some(active) = profiles
                .iter()
                .find(|active| active.profile.id == lead.profile_id)
            else {
                continue;
            };
            let rules = &active.revision.definition.definition().target_companies;
            if let Some(reason) = target_exclusion(rules, official) {
                groups.entry(reason).or_default().push(lead.id);
            }
        }
        exclude_by_reason(&mut suppression, &mut timeline, &groups, now).await
    }

    #[transactional(company, suppression, timeline)]
    async fn sweep_target_batch(
        &self,
        profile_id: ProfileId,
        rules: &TargetCompanies,
        after: Option<LeadId>,
        batch: u32,
        now: DateTime<Utc>,
    ) -> Result<SweepStep, CoreError> {
        let mut company = company_repository;
        let mut suppression = suppression_repository;
        let mut timeline = timeline_repository;
        let found = company.targeted_leads(profile_id, after, batch).await?;
        let mut groups = Groups::new();
        let mut restored = 0;
        for targeted in &found {
            match ruling(rules, targeted.exclusion.as_deref(), &targeted.official) {
                Ruling::Keep => {}
                Ruling::Exclude(reason) => {
                    groups.entry(reason).or_default().push(targeted.lead_id);
                }
                Ruling::Restore(stored) => {
                    if suppression
                        .lift_exclusion(targeted.lead_id, &stored)
                        .await?
                    {
                        timeline
                            .record(targeted.lead_id, &exclusion_lifted(&stored, now))
                            .await?;
                        restored += 1;
                    }
                }
                Ruling::Replace { stored, reason } => {
                    if suppression
                        .lift_exclusion(targeted.lead_id, &stored)
                        .await?
                    {
                        groups.entry(reason).or_default().push(targeted.lead_id);
                    }
                }
            }
        }
        let excluded = exclude_by_reason(&mut suppression, &mut timeline, &groups, now).await?;
        Ok(SweepStep {
            read: found.len(),
            last: found.last().map(|targeted| targeted.lead_id),
            excluded,
            restored,
        })
    }
}
