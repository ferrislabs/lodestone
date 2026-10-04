use common::CoreError;
use lodestone_macros::transactional;

use crate::{
    application::LodestoneUseCase,
    domain::profile::{
        ActiveProfile, Profile, ProfileId, ProfileRevision,
        commands::{CreateProfileCommand, ReviseProfileCommand},
        service::ProfileService,
    },
};

impl LodestoneUseCase {
    #[transactional(profile)]
    pub async fn create_profile(
        &self,
        command: CreateProfileCommand,
    ) -> Result<ActiveProfile, CoreError> {
        let mut service = ProfileService::new(profile_repository);
        service.create_profile(command).await
    }

    #[transactional(profile)]
    pub async fn revise_profile(
        &self,
        command: ReviseProfileCommand,
    ) -> Result<ProfileRevision, CoreError> {
        let mut service = ProfileService::new(profile_repository);
        service.revise_profile(command).await
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
