use chrono::Utc;
use common::CoreError;
use lodestone_macros::transactional;

use crate::{
    application::{LodestoneUseCase, targeting::sweep_job},
    domain::{
        output::ports::JobOutbox,
        profile::{
            ActiveProfile, Profile, ProfileId, ProfileRevision, Revised,
            commands::{CreateProfileCommand, ReviseProfileCommand},
            service::ProfileService,
        },
    },
    infrastructure::job::postgres::PgJobOutbox,
};

impl LodestoneUseCase {
    #[transactional(profile)]
    pub async fn create_profile(
        &self,
        command: CreateProfileCommand,
    ) -> Result<ActiveProfile, CoreError> {
        let mut service = ProfileService::new(profile_repository);
        let created = service.create_profile(command).await?;
        let now = Utc::now();
        PgJobOutbox::new(&tx)
            .enqueue(
                &sweep_job(created.profile.id, created.revision.number, now)?,
                now,
            )
            .await?;
        Ok(created)
    }

    #[transactional(profile)]
    pub async fn revise_profile(
        &self,
        command: ReviseProfileCommand,
    ) -> Result<Revised, CoreError> {
        let mut service = ProfileService::new(profile_repository);
        let revised = service.revise_profile(command).await?;
        let now = Utc::now();
        PgJobOutbox::new(&tx)
            .enqueue(
                &sweep_job(
                    revised.revision().profile_id,
                    revised.revision().number,
                    now,
                )?,
                now,
            )
            .await?;
        Ok(revised)
    }

    #[transactional(profile)]
    pub async fn set_profile_active(
        &self,
        id: ProfileId,
        active: bool,
    ) -> Result<Profile, CoreError> {
        let mut service = ProfileService::new(profile_repository);
        service.set_active(id, active).await
    }

    #[transactional(profile)]
    pub async fn find_profile(&self, id: ProfileId) -> Result<Profile, CoreError> {
        let mut service = ProfileService::new(profile_repository);
        service.get(id).await
    }

    #[transactional(profile)]
    pub async fn list_profiles(&self) -> Result<Vec<Profile>, CoreError> {
        let mut service = ProfileService::new(profile_repository);
        service.list().await
    }

    #[transactional(profile)]
    pub async fn list_profile_revisions(
        &self,
        id: ProfileId,
    ) -> Result<Vec<ProfileRevision>, CoreError> {
        let mut service = ProfileService::new(profile_repository);
        service.list_revisions(id).await
    }

    #[transactional(profile)]
    pub async fn list_active_profiles(&self) -> Result<Vec<ActiveProfile>, CoreError> {
        let mut service = ProfileService::new(profile_repository);
        service.list_active().await
    }
}
