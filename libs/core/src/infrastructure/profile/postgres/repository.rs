use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;

use crate::{
    Profile, ProfileId, ProfileRevision, RevisionNumber,
    domain::profile::{ActiveProfile, ports::ProfileRepository},
    infrastructure::{
        postgres::{SharedTx, error::map_sqlx_error},
        profile::postgres::model::{ActiveProfileRow, ProfileRow, RevisionRow, revision_number},
    },
};

#[repository(domain = Profile, backend = Postgres)]
pub struct PgProfileRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgProfileRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> ProfileRepository for PgProfileRepository<'tx> {
    #[tracing::instrument(skip(self, profile), fields(db.system = "postgresql", db.operation = "insert", db.table = "profiles", profile.id = %profile.id), err)]
    async fn insert_profile(&mut self, profile: &Profile) -> Result<(), CoreError> {
        let mut tx = self.tx.lock().await;
        sqlx::query!(
            r#"
            INSERT INTO profiles (id, name, active, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            profile.id.0,
            profile.name.as_str(),
            profile.active,
            profile.created_at,
            profile.updated_at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    #[tracing::instrument(skip(self, revision), fields(db.system = "postgresql", db.operation = "insert", db.table = "profile_revisions", profile.id = %revision.profile_id, profile.revision = revision.number.get()), err)]
    async fn insert_revision(&mut self, revision: &ProfileRevision) -> Result<(), CoreError> {
        let number = i32::try_from(revision.number.get())
            .map_err(|_| CoreError::Internal("profile revision number out of range".to_owned()))?;
        let mut tx = self.tx.lock().await;
        sqlx::query!(
            r#"
            INSERT INTO profile_revisions (profile_id, revision, definition, created_by, created_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            revision.profile_id.0,
            number,
            revision.definition.to_json(),
            revision.created_by.map(|id| id.0),
            revision.created_at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "profiles", profile.id = %id), err)]
    async fn find(&mut self, id: ProfileId) -> Result<Option<Profile>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            ProfileRow,
            r#"
            SELECT id, name, active, created_at, updated_at
            FROM profiles
            WHERE id = $1
            "#,
            id.0,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(Profile::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "profile_revisions", profile.id = %id), err)]
    async fn lock_latest_revision_number(
        &mut self,
        id: ProfileId,
    ) -> Result<Option<RevisionNumber>, CoreError> {
        let mut tx = self.tx.lock().await;
        let locked = sqlx::query!(r#"SELECT id FROM profiles WHERE id = $1 FOR UPDATE"#, id.0,)
            .fetch_optional(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;

        if locked.is_none() {
            return Ok(None);
        }

        let latest = sqlx::query_scalar!(
            r#"SELECT MAX(revision) FROM profile_revisions WHERE profile_id = $1"#,
            id.0,
        )
        .fetch_one(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        latest.map(revision_number).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "profiles", profile.id = %id), err)]
    async fn set_active(
        &mut self,
        id: ProfileId,
        active: bool,
        updated_at: DateTime<Utc>,
    ) -> Result<Option<Profile>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as!(
            ProfileRow,
            r#"
            UPDATE profiles
            SET active = $2, updated_at = $3
            WHERE id = $1
            RETURNING id, name, active, created_at, updated_at
            "#,
            id.0,
            active,
            updated_at,
        )
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        row.map(Profile::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "profiles"), err)]
    async fn list(&mut self) -> Result<Vec<Profile>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            ProfileRow,
            r#"
            SELECT id, name, active, created_at, updated_at
            FROM profiles
            ORDER BY name
            "#,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(Profile::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "profile_revisions", profile.id = %id), err)]
    async fn list_revisions(&mut self, id: ProfileId) -> Result<Vec<ProfileRevision>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            RevisionRow,
            r#"
            SELECT profile_id, revision, definition, created_by, created_at
            FROM profile_revisions
            WHERE profile_id = $1
            ORDER BY revision ASC
            "#,
            id.0,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(ProfileRevision::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "profiles"), err)]
    async fn list_active(&mut self) -> Result<Vec<ActiveProfile>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            ActiveProfileRow,
            r#"
            SELECT
                p.id,
                p.name,
                p.active,
                p.created_at,
                p.updated_at,
                r.revision,
                r.definition,
                r.created_by,
                r.created_at AS revision_created_at
            FROM profiles p
            JOIN LATERAL (
                SELECT revision, definition, created_by, created_at
                FROM profile_revisions
                WHERE profile_id = p.id
                ORDER BY revision DESC
                LIMIT 1
            ) r ON true
            WHERE p.active
            ORDER BY p.name
            "#,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(ActiveProfile::try_from).collect()
    }
}
