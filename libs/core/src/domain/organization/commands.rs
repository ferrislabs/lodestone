use authz::Subject;

use crate::{
    UserId,
    domain::organization::{
        OrganizationId,
        naming::{OrganizationName, Slug},
    },
};

#[derive(Debug, Clone)]
pub struct CreateOrganizationCommand {
    pub name: OrganizationName,
    pub slug: Slug,
    pub owner_id: UserId,
}

#[derive(Debug, Clone)]
pub struct UpdateOrganizationCommand {
    /// Authenticated actor performing the update. Built by the handler
    /// from the request `Identity`; carries the AuthZen-shaped subject
    /// the policy engine consumes.
    pub actor: Subject,
    pub id: OrganizationId,
    pub name: OrganizationName,
    pub slug: Slug,
}
