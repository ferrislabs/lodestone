use common::CoreError;
use oxid_macros::repository;

use crate::{
    User,
    domain::user::ports::UserRepository,
    infrastructure::{
        postgres::{SharedTx, error::map_sqlx_error},
        user::postgres::model::UserRow,
    },
};

#[repository(domain = User, backend = Postgres)]
pub struct PgUserRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgUserRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> UserRepository for PgUserRepository<'tx> {
    #[tracing::instrument(skip(self, user), fields(db.system = "postgresql", db.operation = "upsert", db.table = "users", user.sub = %user.sub), err)]
    async fn upsert_by_sub(&mut self, user: &User) -> Result<User, CoreError> {
        let mut tx = self.tx.lock().await;
        // Conflicting on `sub` keeps one row per identity-provider subject.
        // Email and username are refreshed from the token because they are
        // attributes of that identity, never the thing that identifies it.
        let row = sqlx::query_as!(
            UserRow,
            r#"
            INSERT INTO users (id, email, username, display_name, sub, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (sub) DO UPDATE SET
                email        = EXCLUDED.email,
                username     = EXCLUDED.username,
                display_name = EXCLUDED.display_name,
                updated_at   = EXCLUDED.updated_at
            RETURNING id, email, username, display_name, sub, created_at, updated_at
            "#,
            user.id.0,
            user.email,
            user.username,
            user.name,
            user.sub,
            user.created_at,
            user.updated_at,
        )
        .fetch_one(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.into())
    }
}
