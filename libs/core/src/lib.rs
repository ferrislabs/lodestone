extern crate self as oxid_core;

pub mod application;
pub(crate) mod domain;
pub mod infrastructure;

pub use application::*;
pub use domain::{
    Member, MemberId, Organization, OrganizationId, Permissions, Role, RoleId, User, UserId,
    organization::commands::{CreateOrganizationCommand, UpdateOrganizationCommand},
    organization::naming::{NamingError, OrganizationName, Slug},
    user::commands::CreateUserCommand,
};
