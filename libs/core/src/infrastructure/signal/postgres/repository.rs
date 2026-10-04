use common::CoreError;
use lodestone_macros::repository;

use crate::{
    LeadId, Signal,
    domain::lead::ports::SignalRepository,
    infrastructure::{
        postgres::{SharedTx, error::map_sqlx_error},
        signal::postgres::model::SignalRow,
    },
};

#[repository(domain = Signal, backend = Postgres)]
pub struct PgSignalRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgSignalRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> SignalRepository for PgSignalRepository<'tx> {
    #[tracing::instrument(skip(self, signal), fields(db.system = "postgresql", db.operation = "insert", db.table = "signals", signal.id = %signal.id), err)]
    async fn insert(&mut self, signal: &Signal) -> Result<(), CoreError> {
        let mut tx = self.tx.lock().await;
        sqlx::query!(
            r#"
            INSERT INTO signals (id, lead_id, kind, value, source, raw, provenance, strength, observed_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            signal.id.0,
            signal.lead_id.0,
            signal.kind.as_str(),
            signal.value.as_deref(),
            signal.source.as_str(),
            &signal.raw,
            signal.provenance.as_str(),
            signal.strength.get(),
            signal.observed_at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "signals", lead.id = %lead_id), err)]
    async fn list_for_lead(&mut self, lead_id: LeadId) -> Result<Vec<Signal>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            SignalRow,
            r#"
            SELECT id, lead_id, kind, value, source, raw, provenance, strength, observed_at
            FROM signals
            WHERE lead_id = $1
            ORDER BY observed_at ASC, id ASC
            "#,
            lead_id.0,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(Signal::try_from).collect()
    }
}
