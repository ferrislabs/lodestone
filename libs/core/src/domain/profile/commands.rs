use crate::{
    UserId,
    domain::profile::{ProfileId, ProfileName, ValidatedProfile},
};

#[derive(Debug, Clone)]
pub struct CreateProfileCommand {
    pub name: ProfileName,
    pub definition: ValidatedProfile,
    pub created_by: Option<UserId>,
}

#[derive(Debug, Clone)]
pub struct ReviseProfileCommand {
    pub profile_id: ProfileId,
    pub definition: ValidatedProfile,
    pub created_by: Option<UserId>,
}
