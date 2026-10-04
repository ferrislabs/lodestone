use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::transactional;

use crate::{
    application::LodestoneUseCase,
    domain::{
        partner::{PartnerAward, ports::PartnerRepository},
        profile::ProfileId,
    },
};

impl LodestoneUseCase {
    #[transactional(partner)]
    pub async fn record_partner_awards(
        &self,
        profile_id: ProfileId,
        awards: &[PartnerAward],
        at: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut repo = partner_repository;
        let mut inserted = 0;
        for award in awards.iter().filter(|award| award.holder != award.buyer) {
            if repo.record_award(profile_id, award, at).await? {
                inserted += 1;
            }
        }
        Ok(inserted)
    }
}
