use chrono::Utc;
use common::{CoreError, generate_uuid_v7};

use crate::domain::profile::{
    ActiveProfile, Profile, ProfileId, ProfileRevision, Revised, RevisionNumber,
    commands::{CreateProfileCommand, ReviseProfileCommand},
    ports::ProfileRepository,
};

pub struct ProfileService<P>
where
    P: ProfileRepository,
{
    repo: P,
}

impl<P> ProfileService<P>
where
    P: ProfileRepository,
{
    pub fn new(repo: P) -> Self {
        Self { repo }
    }

    #[tracing::instrument(skip_all, fields(profile.name = %command.name.as_str()), err)]
    pub async fn create_profile(
        &mut self,
        command: CreateProfileCommand,
    ) -> Result<ActiveProfile, CoreError> {
        let now = Utc::now();
        let profile = Profile {
            id: ProfileId(generate_uuid_v7()),
            name: command.name,
            active: false,
            created_at: now,
            updated_at: now,
        };
        let revision = ProfileRevision {
            profile_id: profile.id,
            number: RevisionNumber::FIRST,
            definition: command.definition,
            created_by: command.created_by,
            created_at: now,
        };
        self.repo.insert_profile(&profile).await?;
        self.repo.insert_revision(&revision).await?;
        Ok(ActiveProfile { profile, revision })
    }

    #[tracing::instrument(skip_all, fields(profile.id = %command.profile_id), err)]
    pub async fn revise_profile(
        &mut self,
        command: ReviseProfileCommand,
    ) -> Result<Revised, CoreError> {
        let latest = self
            .repo
            .lock_latest_revision_number(command.profile_id)
            .await?
            .ok_or(CoreError::NotFound)?;
        let current = self
            .repo
            .list_revisions(command.profile_id)
            .await?
            .into_iter()
            .find(|revision| revision.number == latest);
        if let Some(current) = current
            && current.definition == command.definition
        {
            return Ok(Revised::Unchanged(current));
        }
        let number = latest
            .next()
            .ok_or_else(|| CoreError::Conflict("revision number overflow".to_owned()))?;
        let revision = ProfileRevision {
            profile_id: command.profile_id,
            number,
            definition: command.definition,
            created_by: command.created_by,
            created_at: Utc::now(),
        };
        self.repo.insert_revision(&revision).await?;
        Ok(Revised::Created(revision))
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn set_active(&mut self, id: ProfileId, active: bool) -> Result<Profile, CoreError> {
        self.repo
            .set_active(id, active, Utc::now())
            .await?
            .ok_or(CoreError::NotFound)
    }

    pub async fn get(&mut self, id: ProfileId) -> Result<Profile, CoreError> {
        self.repo.find(id).await?.ok_or(CoreError::NotFound)
    }

    pub async fn list(&mut self) -> Result<Vec<Profile>, CoreError> {
        self.repo.list().await
    }

    pub async fn list_revisions(
        &mut self,
        id: ProfileId,
    ) -> Result<Vec<ProfileRevision>, CoreError> {
        if self.repo.find(id).await?.is_none() {
            return Err(CoreError::NotFound);
        }
        self.repo.list_revisions(id).await
    }

    pub async fn list_active(&mut self) -> Result<Vec<ActiveProfile>, CoreError> {
        self.repo.list_active().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile::{ProfileName, ValidatedProfile, ports::MockProfileRepository};

    fn definition() -> ValidatedProfile {
        ValidatedProfile::parse(
            serde_json::from_str(crate::domain::profile::fixtures::MINIMAL_PROFILE).unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn new_profile_starts_inactive_with_first_revision() {
        let mut repo = MockProfileRepository::new();
        repo.expect_insert_profile()
            .withf(|profile| !profile.active)
            .times(1)
            .returning(|_| Box::pin(async { Ok(()) }));
        repo.expect_insert_revision()
            .withf(|revision| revision.number == RevisionNumber::FIRST)
            .times(1)
            .returning(|_| Box::pin(async { Ok(()) }));

        let created = ProfileService::new(repo)
            .create_profile(CreateProfileCommand {
                name: ProfileName::parse("IAM").unwrap(),
                definition: definition(),
                created_by: None,
            })
            .await
            .unwrap();

        assert_eq!(created.revision.profile_id, created.profile.id);
    }

    #[tokio::test]
    async fn revision_number_follows_the_latest() {
        let mut repo = MockProfileRepository::new();
        repo.expect_lock_latest_revision_number()
            .returning(|_| Box::pin(async { Ok(RevisionNumber::new(3)) }));
        repo.expect_list_revisions()
            .returning(|_| Box::pin(async { Ok(Vec::new()) }));
        repo.expect_insert_revision()
            .withf(|revision| revision.number.get() == 4)
            .times(1)
            .returning(|_| Box::pin(async { Ok(()) }));

        let revised = ProfileService::new(repo)
            .revise_profile(ReviseProfileCommand {
                profile_id: ProfileId(uuid::Uuid::nil()),
                definition: definition(),
                created_by: None,
            })
            .await
            .unwrap();

        assert!(revised.is_created());
        assert_eq!(revised.revision().number.get(), 4);
    }

    #[tokio::test]
    async fn an_identical_definition_keeps_the_current_revision() {
        let mut repo = MockProfileRepository::new();
        repo.expect_lock_latest_revision_number()
            .returning(|_| Box::pin(async { Ok(RevisionNumber::new(3)) }));
        repo.expect_list_revisions().returning(|_| {
            Box::pin(async {
                Ok(vec![ProfileRevision {
                    profile_id: ProfileId(uuid::Uuid::nil()),
                    number: RevisionNumber::new(3).unwrap(),
                    definition: definition(),
                    created_by: None,
                    created_at: Utc::now(),
                }])
            })
        });
        repo.expect_insert_revision().never();

        let revised = ProfileService::new(repo)
            .revise_profile(ReviseProfileCommand {
                profile_id: ProfileId(uuid::Uuid::nil()),
                definition: definition(),
                created_by: None,
            })
            .await
            .unwrap();

        assert!(!revised.is_created());
        assert_eq!(revised.revision().number.get(), 3);
    }

    #[tokio::test]
    async fn revising_an_unknown_profile_is_not_found() {
        let mut repo = MockProfileRepository::new();
        repo.expect_lock_latest_revision_number()
            .returning(|_| Box::pin(async { Ok(None) }));
        repo.expect_insert_revision().never();

        let result = ProfileService::new(repo)
            .revise_profile(ReviseProfileCommand {
                profile_id: ProfileId(uuid::Uuid::nil()),
                definition: definition(),
                created_by: None,
            })
            .await;

        assert!(matches!(result, Err(CoreError::NotFound)));
    }

    fn stored_profile() -> Profile {
        let now = Utc::now();
        Profile {
            id: ProfileId(uuid::Uuid::nil()),
            name: crate::domain::profile::ProfileName::parse("IAM").unwrap(),
            active: false,
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn get_returns_the_stored_profile() {
        let mut repo = MockProfileRepository::new();
        repo.expect_find()
            .returning(|_| Box::pin(async { Ok(Some(stored_profile())) }));

        let profile = ProfileService::new(repo)
            .get(ProfileId(uuid::Uuid::nil()))
            .await
            .unwrap();

        assert_eq!(profile.name.as_str(), "IAM");
    }

    #[tokio::test]
    async fn get_of_an_unknown_profile_is_not_found() {
        let mut repo = MockProfileRepository::new();
        repo.expect_find()
            .returning(|_| Box::pin(async { Ok(None) }));

        let result = ProfileService::new(repo)
            .get(ProfileId(uuid::Uuid::nil()))
            .await;

        assert!(matches!(result, Err(CoreError::NotFound)));
    }

    #[tokio::test]
    async fn activating_an_unknown_profile_is_not_found() {
        let mut repo = MockProfileRepository::new();
        repo.expect_set_active()
            .returning(|_, _, _| Box::pin(async { Ok(None) }));

        let result = ProfileService::new(repo)
            .set_active(ProfileId(uuid::Uuid::nil()), true)
            .await;

        assert!(matches!(result, Err(CoreError::NotFound)));
    }
}
