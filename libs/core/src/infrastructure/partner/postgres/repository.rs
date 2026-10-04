use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::repository;
use uuid::Uuid;

use crate::{
    Partner, PartnerAward, ProfileId,
    domain::partner::ports::PartnerRepository,
    infrastructure::{
        partner::postgres::model::PartnerRow,
        postgres::{SharedTx, error::map_sqlx_error},
    },
};

#[repository(domain = Partner, backend = Postgres)]
pub struct PgPartnerRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgPartnerRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> PartnerRepository for PgPartnerRepository<'tx> {
    #[tracing::instrument(skip(self, award), fields(db.system = "postgresql", db.operation = "upsert", db.table = "partner_awards", partner.siret = %award.holder), err)]
    async fn record_award(
        &mut self,
        profile_id: ProfileId,
        award: &PartnerAward,
        at: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let matched = serde_json::to_value(&award.matched)
            .map_err(|_| CoreError::Internal("partner award keywords not serializable".into()))?;
        let mut tx = self.tx.lock().await;
        let partner_id: Uuid = sqlx::query_scalar!(
            r#"
            INSERT INTO partners (id, profile_id, siret, name, first_seen_at, award_count)
            VALUES ($1, $2, $3, $4, $5, 0)
            ON CONFLICT (profile_id, siret) DO UPDATE
            SET name = COALESCE(EXCLUDED.name, partners.name)
            RETURNING id
            "#,
            generate_uuid_v7(),
            profile_id.0,
            award.holder.as_str(),
            award.holder_name.as_deref(),
            at,
        )
        .fetch_one(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        let inserted = sqlx::query!(
            r#"
            INSERT INTO partner_awards (
                id, partner_id, contract_id, buyer_siret, object, amount,
                notified_on, provenance, matched
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (partner_id, contract_id) DO NOTHING
            "#,
            generate_uuid_v7(),
            partner_id,
            award.contract_id,
            award.buyer.as_str(),
            award.object,
            award.amount,
            award.notified_on,
            award.provenance.as_str(),
            matched,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?
        .rows_affected()
            > 0;

        sqlx::query!(
            r#"
            UPDATE partners
            SET award_count = (
                    SELECT COUNT(*)::int FROM partner_awards WHERE partner_id = $1
                ),
                last_award_at = (
                    SELECT MAX(notified_on) FROM partner_awards WHERE partner_id = $1
                )
            WHERE id = $1
            "#,
            partner_id,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(inserted)
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "partners", profile.id = %profile_id), err)]
    async fn list_for_profile(&mut self, profile_id: ProfileId) -> Result<Vec<Partner>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            PartnerRow,
            r#"
            SELECT id, profile_id, siret, name, first_seen_at, last_award_at, award_count
            FROM partners
            WHERE profile_id = $1
            ORDER BY last_award_at DESC NULLS LAST, siret
            "#,
            profile_id.0,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(Partner::try_from).collect()
    }
}
