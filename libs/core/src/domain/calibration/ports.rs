use common::CoreError;

use crate::domain::{
    calibration::{Calibration, LabelledQualification, LabelledScoring},
    profile::ProfileId,
};

pub trait CalibrationRepository: Send {
    fn list(
        &mut self,
        profile_id: ProfileId,
    ) -> impl Future<Output = Result<Vec<Calibration>, CoreError>> + Send;

    fn labelled_scoring(
        &mut self,
        profile_id: ProfileId,
    ) -> impl Future<Output = Result<Vec<LabelledScoring>, CoreError>> + Send;

    fn labelled_qualification(
        &mut self,
        profile_id: ProfileId,
    ) -> impl Future<Output = Result<Vec<LabelledQualification>, CoreError>> + Send;

    fn replace(
        &mut self,
        profile_id: ProfileId,
        calibrations: &[Calibration],
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn mark_stale(
        &mut self,
        profile_id: ProfileId,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;
}
