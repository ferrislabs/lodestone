use common::CoreError;
use lodestone_macros::repository;

use crate::{
    ContactEmail, LeadId,
    domain::lead::ports::ContactRepository,
    infrastructure::{
        contact::postgres::model::ContactRow,
        postgres::{SharedTx, error::map_sqlx_error},
    },
};

#[repository(domain = Contact, backend = Postgres)]
pub struct PgContactRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgContactRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> ContactRepository for PgContactRepository<'tx> {
    #[tracing::instrument(skip(self, contact), fields(db.system = "postgresql", db.operation = "insert", db.table = "contact_emails", contact.id = %contact.id), err)]
    async fn insert(&mut self, contact: &ContactEmail) -> Result<(), CoreError> {
        let mut tx = self.tx.lock().await;
        sqlx::query!(
            r#"
            INSERT INTO contact_emails (id, lead_id, address, provenance, method, confidence, kind, found_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            contact.id.0,
            contact.lead_id.0,
            contact.address.as_str(),
            contact.provenance.as_str(),
            contact.method.as_str(),
            contact.confidence.get(),
            contact.kind.as_str(),
            contact.found_at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "contact_emails", lead.id = %lead_id), err)]
    async fn list_for_lead(&mut self, lead_id: LeadId) -> Result<Vec<ContactEmail>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            ContactRow,
            r#"
            SELECT id, lead_id, address, provenance, method, confidence, kind, found_at
            FROM contact_emails
            WHERE lead_id = $1
            ORDER BY confidence DESC, address ASC
            "#,
            lead_id.0,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(ContactEmail::try_from).collect()
    }
}
