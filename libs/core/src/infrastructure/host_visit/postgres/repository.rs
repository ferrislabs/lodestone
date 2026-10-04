use std::time::Duration;

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;

use crate::{
    domain::fetch::ports::HostVisitRepository,
    infrastructure::postgres::{SharedTx, error::map_sqlx_error},
};

#[repository(domain = HostVisit, backend = Postgres)]
pub struct PgHostVisitRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgHostVisitRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> HostVisitRepository for PgHostVisitRepository<'tx> {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "upsert", db.table = "host_visits"), err)]
    async fn reserve_probe(
        &mut self,
        host: &str,
        now: DateTime<Utc>,
        revisit_interval: Duration,
    ) -> Result<bool, CoreError> {
        let interval = chrono::Duration::from_std(revisit_interval)
            .map_err(|_| CoreError::Internal("revisit interval out of range".to_owned()))?;
        let due_before = now
            .checked_sub_signed(interval)
            .ok_or_else(|| CoreError::Internal("revisit interval out of range".to_owned()))?;
        let host = host.to_ascii_lowercase();
        let mut tx = self.tx.lock().await;
        let reserved = sqlx::query_scalar!(
            r#"
            INSERT INTO host_visits (host, last_probed_at)
            VALUES ($1, $2)
            ON CONFLICT (host) DO UPDATE SET last_probed_at = EXCLUDED.last_probed_at
            WHERE host_visits.last_probed_at <= $3
            RETURNING host
            "#,
            host,
            now,
            due_before,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(reserved.is_some())
    }
}
