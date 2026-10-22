use common::CoreError;
use lodestone_macros::transactional;

use crate::{
    UserId,
    application::LodestoneUseCase,
    domain::{
        profile::{
            ActiveProfile, ProfileId, Revised, RevisionNumber, ValidatedProfile,
            commands::ReviseProfileCommand, ports::ProfileRepository, service::ProfileService,
        },
        quality::{EditProblem, SuggestionEdit},
    },
};

fn refused(problem: EditProblem) -> CoreError {
    match problem {
        EditProblem::UnknownAgent => CoreError::NotFound,
        EditProblem::EmptyWord => CoreError::Invalid {
            field: "word".to_owned(),
            message: problem.to_string(),
        },
        EditProblem::KeywordMissing | EditProblem::ExclusionPresent | EditProblem::LastKeyword => {
            CoreError::Conflict(problem.to_string())
        }
    }
}

impl LodestoneUseCase {
    pub async fn apply_keyword_suggestion(
        &self,
        profile_id: ProfileId,
        suggestion: &SuggestionEdit,
        expected_revision: RevisionNumber,
        actor: UserId,
    ) -> Result<ActiveProfile, CoreError> {
        let (previous, revised) = self
            .edit_agent_rules(profile_id, suggestion, expected_revision, actor)
            .await?;
        let current = ActiveProfile {
            profile: previous.profile.clone(),
            revision: revised.revision().clone(),
        };
        self.settle_profile(&previous, Some(&current)).await?;
        Ok(current)
    }

    #[transactional(profile)]
    async fn edit_agent_rules(
        &self,
        profile_id: ProfileId,
        suggestion: &SuggestionEdit,
        expected_revision: RevisionNumber,
        actor: UserId,
    ) -> Result<(ActiveProfile, Revised), CoreError> {
        let mut repo = profile_repository;
        let latest = repo
            .lock_latest_revision_number(profile_id)
            .await?
            .ok_or(CoreError::NotFound)?;
        if latest != expected_revision {
            return Err(CoreError::Conflict(format!(
                "the profile is at revision {} and the suggestion was made for revision {}",
                latest.get(),
                expected_revision.get()
            )));
        }
        let profile = repo.find(profile_id).await?.ok_or(CoreError::NotFound)?;
        let revision = repo
            .list_revisions(profile_id)
            .await?
            .into_iter()
            .find(|revision| revision.number == latest)
            .ok_or(CoreError::NotFound)?;
        let edited = suggestion
            .apply(revision.definition.definition())
            .map_err(refused)?;
        let definition =
            ValidatedProfile::try_from(edited).map_err(|error| CoreError::Invalid {
                field: error.path,
                message: error.problem.to_string(),
            })?;
        let revised = ProfileService::new(repo)
            .revise_profile(ReviseProfileCommand {
                profile_id,
                definition,
                created_by: Some(actor),
            })
            .await?;
        Ok((ActiveProfile { profile, revision }, revised))
    }
}
