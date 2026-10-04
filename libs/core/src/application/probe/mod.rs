use std::time::Duration;

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::transactional;

use crate::{application::LodestoneUseCase, domain::fetch::ports::HostVisitRepository};

impl LodestoneUseCase {
    #[transactional(host_visit)]
    pub async fn reserve_probe(
        &self,
        host: &str,
        now: DateTime<Utc>,
        revisit_interval: Duration,
    ) -> Result<bool, CoreError> {
        let mut visits = host_visit_repository;
        visits.reserve_probe(host, now, revisit_interval).await
    }
}
