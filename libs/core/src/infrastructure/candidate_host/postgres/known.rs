use common::CoreError;
use lodestone_macros::repository;

use crate::{
    CandidateHost, NormalizedDomain,
    domain::host::ports::KnownHostRepository,
    infrastructure::{
        candidate_host::postgres::model::CandidateHostRow,
        postgres::{SharedTx, error::map_sqlx_error},
    },
};

#[repository(domain = KnownHost, backend = Postgres)]
pub struct PgKnownHostRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgKnownHostRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> KnownHostRepository for PgKnownHostRepository<'tx> {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "candidate_hosts"), err)]
    async fn list_for_domain(
        &mut self,
        domain: &NormalizedDomain,
    ) -> Result<Vec<CandidateHost>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            CandidateHostRow,
            r#"
            SELECT host, domain, origin, first_seen_at, last_seen_at
            FROM candidate_hosts
            WHERE domain = $1
            ORDER BY host
            "#,
            domain.as_str(),
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter().map(CandidateHost::try_from).collect()
    }
}
