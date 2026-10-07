use std::num::NonZeroU16;

use chrono::{DateTime, Duration, Utc};
use common::CoreError;
use lodestone_macros::transactional;
use serde_json::json;
use tracing::info;

use crate::{
    application::LodestoneUseCase,
    domain::{
        calibration::{
            CALIBRATION_JOB_INTERVAL_HOURS, CALIBRATION_JOB_KIND, Calibrations, ProfileCalibration,
            effect_changed, fit_profile, ports::CalibrationRepository, profile_report,
        },
        job::{Job, JobKind, NewJob},
        output::ports::JobOutbox,
        profile::{
            ActiveProfile, ProfileId, definition::ProfileDefinition, ports::ProfileRepository,
        },
    },
    infrastructure::job::postgres::PgJobOutbox,
};

const CALIBRATION_JOB_MAX_ATTEMPTS: NonZeroU16 = NonZeroU16::new(3).unwrap();
const RECURRING_DEDUP_KEY: &str = "quality:calibrate";
const MANUAL_DEDUP_KEY: &str = "quality:calibrate:manual";

pub fn calibration_job(run_at: DateTime<Utc>, recurring: bool) -> Result<NewJob, CoreError> {
    let kind = JobKind::parse(CALIBRATION_JOB_KIND)
        .map_err(|_| CoreError::Internal("invalid calibration job kind".to_owned()))?;
    Ok(NewJob {
        kind,
        payload: json!({ "recurring": recurring }),
        run_at,
        max_attempts: CALIBRATION_JOB_MAX_ATTEMPTS,
        dedup_key: Some(
            if recurring {
                RECURRING_DEDUP_KEY
            } else {
                MANUAL_DEDUP_KEY
            }
            .to_owned(),
        ),
    })
}

pub fn is_recurring_calibration(job: &Job) -> bool {
    job.payload
        .get("recurring")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

pub fn next_calibration(now: DateTime<Utc>) -> DateTime<Utc> {
    now + Duration::hours(CALIBRATION_JOB_INTERVAL_HOURS)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CalibrationReport {
    pub profiles: u64,
    pub agents_fitted: u64,
    pub rescored: u64,
}

impl LodestoneUseCase {
    pub async fn calibrate_profiles(
        &self,
        now: DateTime<Utc>,
    ) -> Result<CalibrationReport, CoreError> {
        let mut report = CalibrationReport::default();
        for (profile_id, definition) in self.latest_definitions().await? {
            let (fitted, rescored) = self.calibrate_profile(profile_id, &definition, now).await?;
            report.profiles += 1;
            report.agents_fitted += fitted;
            report.rescored += rescored;
        }
        info!(
            profiles = report.profiles,
            agents_fitted = report.agents_fitted,
            leads_marked_stale = report.rescored,
            "calibrations refitted"
        );
        Ok(report)
    }

    pub async fn enqueue_calibration(&self, now: DateTime<Utc>) -> Result<bool, CoreError> {
        self.enqueue_calibration_job(&calibration_job(now, false)?, now)
            .await
    }

    #[transactional]
    async fn enqueue_calibration_job(
        &self,
        job: &NewJob,
        now: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        Ok(PgJobOutbox::new(&tx).enqueue(job, now).await?.is_some())
    }

    #[transactional(profile)]
    async fn latest_definitions(&self) -> Result<Vec<(ProfileId, ProfileDefinition)>, CoreError> {
        let mut profiles = profile_repository;
        let mut found = Vec::new();
        for profile in profiles.list().await? {
            let revisions = profiles.list_revisions(profile.id).await?;
            if let Some(latest) = revisions.last() {
                found.push((profile.id, latest.definition.definition().clone()));
            }
        }
        Ok(found)
    }

    #[transactional(calibration)]
    async fn calibrate_profile(
        &self,
        profile_id: ProfileId,
        definition: &ProfileDefinition,
        now: DateTime<Utc>,
    ) -> Result<(u64, u64), CoreError> {
        let mut calibrations = calibration_repository;
        let before = calibrations.list(profile_id).await?;
        let scoring = calibrations.labelled_scoring(profile_id).await?;
        let qualification = calibrations.labelled_qualification(profile_id).await?;
        let after = fit_profile(profile_id, definition, &scoring, &qualification, now);
        calibrations.replace(profile_id, &after).await?;
        let rescored = if definition.calibration.learned && effect_changed(&before, &after) {
            calibrations.mark_stale(profile_id).await?
        } else {
            0
        };
        let fitted = after.iter().filter(|item| item.applies()).count();
        Ok((u64::try_from(fitted).unwrap_or(u64::MAX), rescored))
    }

    #[transactional(calibration)]
    pub(crate) async fn profile_calibrations(
        &self,
        active: &ActiveProfile,
    ) -> Result<Calibrations, CoreError> {
        let mut calibrations = calibration_repository;
        let learned = active.revision.definition.definition().calibration.learned;
        if !learned {
            return Ok(Calibrations::none());
        }
        Ok(Calibrations::applied(
            calibrations.list(active.profile.id).await?,
            learned,
        ))
    }

    #[transactional(profile, calibration)]
    pub async fn calibration_report(&self) -> Result<Vec<ProfileCalibration>, CoreError> {
        let mut profiles = profile_repository;
        let mut calibrations = calibration_repository;
        let mut report = Vec::new();
        for profile in profiles.list().await? {
            let revisions = profiles.list_revisions(profile.id).await?;
            let Some(latest) = revisions.last() else {
                continue;
            };
            let stored = calibrations.list(profile.id).await?;
            report.push(profile_report(
                profile.id,
                profile.name.as_str(),
                latest.definition.definition(),
                &stored,
            ));
        }
        Ok(report)
    }
}
