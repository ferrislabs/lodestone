use axum::{
    Extension, Json,
    body::Bytes,
    extract::{Path, Query, State},
    response::{IntoResponse, Response as HttpResponse},
};
use common::CoreError;
use http::{
    HeaderName, HeaderValue, StatusCode,
    header::{CONTENT_DISPOSITION, LOCATION},
};
use lodestone_core::{
    ActiveProfile, CreateProfileCommand, Profile, ProfileId, ProfileName, ProfileRevision,
    ProfileSummary, ReviseProfileCommand, RevisionNumber, ValidatedProfile,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

use crate::{
    auth::AuthenticatedUser,
    errors::{ApiError, ErrorBody, ErrorDetail},
    response::{DataEnvelope, Response},
    state::AppState,
};

const DEFINITION_DESCRIPTION: &str = "Detection profile document, validated against docs/schemas/profile.v16.schema.json (see docs/profiles.md).";

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct ProfileRefDto {
    pub id: String,
    pub name: String,
}

impl From<&Profile> for ProfileRefDto {
    fn from(profile: &Profile) -> Self {
        Self {
            id: profile.id.to_string(),
            name: profile.name.as_str().to_owned(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct ProfileSummaryDto {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub revision: Option<u32>,
    pub lead_count: u64,
}

impl From<&ProfileSummary> for ProfileSummaryDto {
    fn from(summary: &ProfileSummary) -> Self {
        Self {
            id: summary.id.to_string(),
            name: summary.name.as_str().to_owned(),
            active: summary.active,
            revision: summary.revision.map(|revision| revision.get()),
            lead_count: summary.lead_count,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/profiles",
    tag = "profiles",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Detection profiles with their current revision and visible lead count", body = DataEnvelope<Vec<ProfileSummaryDto>>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
    )
)]
pub async fn list_profiles(
    State(state): State<AppState>,
) -> Result<Response<Vec<ProfileSummaryDto>>, ApiError> {
    let summaries = state.usecase.list_profiles_summary().await?;
    Ok(Response::OK(
        summaries.iter().map(ProfileSummaryDto::from).collect(),
    ))
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ProfileDto {
    pub id: String,
    pub name: String,
    pub active: bool,
    #[schema(format = DateTime)]
    pub created_at: String,
    #[schema(format = DateTime)]
    pub updated_at: String,
}

impl From<&Profile> for ProfileDto {
    fn from(profile: &Profile) -> Self {
        Self {
            id: profile.id.to_string(),
            name: profile.name.as_str().to_owned(),
            active: profile.active,
            created_at: profile.created_at.to_rfc3339(),
            updated_at: profile.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct ProfileDetailDto {
    #[serde(flatten)]
    pub profile: ProfileDto,
    pub revision: u32,
    /// Detection profile document, see docs/schemas/profile.v16.schema.json.
    #[schema(value_type = Object)]
    pub definition: Value,
}

impl ProfileDetailDto {
    fn new(profile: &Profile, revision: &ProfileRevision) -> Self {
        Self {
            profile: ProfileDto::from(profile),
            revision: revision.number.get(),
            definition: revision.definition.to_json(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct RevisionSummaryDto {
    pub number: u32,
    #[schema(format = DateTime)]
    pub created_at: String,
    pub created_by: Option<String>,
}

impl From<&ProfileRevision> for RevisionSummaryDto {
    fn from(revision: &ProfileRevision) -> Self {
        Self {
            number: revision.number.get(),
            created_at: revision.created_at.to_rfc3339(),
            created_by: revision.created_by.map(|user| user.to_string()),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct RevisionDto {
    #[serde(flatten)]
    pub summary: RevisionSummaryDto,
    /// Detection profile document, see docs/schemas/profile.v16.schema.json.
    #[schema(value_type = Object)]
    pub definition: Value,
}

impl From<&ProfileRevision> for RevisionDto {
    fn from(revision: &ProfileRevision) -> Self {
        Self {
            summary: RevisionSummaryDto::from(revision),
            definition: revision.definition.to_json(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProfileRequest {
    /// Unique profile name. Defaults to `identity.offer` of the definition.
    pub name: Option<String>,
    /// Detection profile document, see docs/schemas/profile.v16.schema.json.
    #[schema(value_type = Object)]
    pub definition: Value,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateProfileRequest {
    /// Detection profile document, see docs/schemas/profile.v16.schema.json.
    #[schema(value_type = Object)]
    pub definition: Value,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ImportParams {
    /// Unique profile name. Defaults to `identity.offer` of the document.
    name: Option<String>,
}

fn reject(path: &str, message: impl Into<String>) -> ApiError {
    ApiError::Rejected(vec![ErrorDetail {
        path: path.to_owned(),
        message: message.into(),
    }])
}

fn profile_id(raw: &str) -> Result<ProfileId, ApiError> {
    raw.parse()
        .map_err(|_| ApiError::BadRequest("profile_id: must be a UUID".to_owned()))
}

fn revision_number(raw: u32) -> Result<RevisionNumber, ApiError> {
    RevisionNumber::new(raw)
        .ok_or_else(|| ApiError::BadRequest("number: must be at least 1".to_owned()))
}

fn actor(user: Option<Extension<AuthenticatedUser>>) -> Result<lodestone_core::UserId, ApiError> {
    match user {
        Some(Extension(AuthenticatedUser(user))) => Ok(user),
        None => Err(ApiError::Forbidden),
    }
}

fn parse_document(bytes: &[u8]) -> Result<Value, ApiError> {
    serde_json::from_slice(bytes).map_err(|error| reject("$", format!("malformed JSON: {error}")))
}

fn validate(document: Value) -> Result<ValidatedProfile, ApiError> {
    ValidatedProfile::parse(document)
        .map_err(|error| reject(&error.path, error.problem.to_string()))
}

fn resolve_name(
    requested: Option<&str>,
    definition: &ValidatedProfile,
) -> Result<ProfileName, ApiError> {
    let raw = requested.unwrap_or(definition.definition().identity.offer.as_str());
    let field = if requested.is_some() {
        "name"
    } else {
        "identity.offer"
    };
    ProfileName::parse(raw).map_err(|error| reject(field, error.to_string()))
}

fn name_conflict(error: CoreError) -> ApiError {
    match error {
        CoreError::Conflict(_) => {
            ApiError::Conflict("a profile with this name already exists".to_owned())
        }
        other => other.into(),
    }
}

fn created(location: String, body: impl Serialize + PartialEq) -> HttpResponse {
    match HeaderValue::from_str(&location) {
        Ok(value) => (
            StatusCode::CREATED,
            [(LOCATION, value)],
            Json(DataEnvelope::new(body)),
        )
            .into_response(),
        Err(_) => ApiError::Internal.into_response(),
    }
}

async fn current(state: &AppState, id: ProfileId) -> Result<ActiveProfile, ApiError> {
    let profile = state.usecase.find_profile(id).await?;
    let revision = state
        .usecase
        .list_profile_revisions(id)
        .await?
        .into_iter()
        .max_by_key(|revision| revision.number)
        .ok_or(ApiError::Internal)?;
    Ok(ActiveProfile { profile, revision })
}

async fn register(
    state: &AppState,
    actor: lodestone_core::UserId,
    name: Option<&str>,
    definition: ValidatedProfile,
) -> Result<HttpResponse, ApiError> {
    let name = resolve_name(name, &definition)?;
    let created_profile = state
        .usecase
        .create_profile(CreateProfileCommand {
            name,
            definition,
            created_by: Some(actor),
        })
        .await
        .map_err(name_conflict)?;
    let location = format!(
        "{}/profiles/{}",
        crate::api::API_PREFIX,
        created_profile.profile.id
    );
    Ok(created(
        location,
        ProfileDetailDto::new(&created_profile.profile, &created_profile.revision),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/profiles/{profile_id}",
    tag = "profiles",
    security(("bearer_auth" = [])),
    params(("profile_id" = String, Path, description = "Profile identifier (UUID)")),
    responses(
        (status = 200, description = "The profile with its current revision definition", body = DataEnvelope<ProfileDetailDto>),
        (status = 400, description = "Malformed profile identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 404, description = "Unknown profile"),
    )
)]
pub async fn get_profile(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Response<ProfileDetailDto>, ApiError> {
    let current = current(&state, profile_id(&raw)?).await?;
    Ok(Response::OK(ProfileDetailDto::new(
        &current.profile,
        &current.revision,
    )))
}

#[utoipa::path(
    get,
    path = "/api/v1/profiles/{profile_id}/revisions",
    tag = "profiles",
    security(("bearer_auth" = [])),
    params(("profile_id" = String, Path, description = "Profile identifier (UUID)")),
    responses(
        (status = 200, description = "Revisions of the profile, oldest first", body = DataEnvelope<Vec<RevisionSummaryDto>>),
        (status = 400, description = "Malformed profile identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 404, description = "Unknown profile"),
    )
)]
pub async fn list_revisions(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Response<Vec<RevisionSummaryDto>>, ApiError> {
    let mut revisions = state
        .usecase
        .list_profile_revisions(profile_id(&raw)?)
        .await?;
    revisions.sort_by_key(|revision| revision.number);
    Ok(Response::OK(
        revisions.iter().map(RevisionSummaryDto::from).collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/profiles/{profile_id}/revisions/{number}",
    tag = "profiles",
    security(("bearer_auth" = [])),
    params(
        ("profile_id" = String, Path, description = "Profile identifier (UUID)"),
        ("number" = u32, Path, description = "Revision number, starting at 1"),
    ),
    responses(
        (status = 200, description = "The revision with its definition", body = DataEnvelope<RevisionDto>),
        (status = 400, description = "Malformed profile identifier or revision number"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 404, description = "Unknown profile or revision"),
    )
)]
pub async fn get_revision(
    State(state): State<AppState>,
    Path((raw, number)): Path<(String, u32)>,
) -> Result<Response<RevisionDto>, ApiError> {
    let number = revision_number(number)?;
    let revisions = state
        .usecase
        .list_profile_revisions(profile_id(&raw)?)
        .await?;
    let revision = revisions
        .iter()
        .find(|revision| revision.number == number)
        .ok_or(ApiError::NotFound)?;
    Ok(Response::OK(RevisionDto::from(revision)))
}

#[utoipa::path(
    post,
    path = "/api/v1/profiles",
    tag = "profiles",
    security(("bearer_auth" = [])),
    request_body = CreateProfileRequest,
    responses(
        (status = 201, description = "The profile, created inactive with revision 1; Location points at it", body = DataEnvelope<ProfileDetailDto>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 409, description = "A profile with this name already exists"),
        (status = 422, description = "The payload is invalid; `details` lists the faulty field paths", body = ErrorBody),
    )
)]
pub async fn create_profile(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    body: Bytes,
) -> Result<HttpResponse, ApiError> {
    let actor = actor(user)?;
    let document = parse_document(&body)?;
    let Value::Object(mut envelope) = document else {
        return Err(reject("$", "must be a JSON object"));
    };
    let name = match envelope.remove("name") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) => Some(name),
        Some(_) => return Err(reject("name", "must be a string")),
    };
    let definition = envelope
        .remove("definition")
        .ok_or_else(|| reject("definition", "is required"))?;
    register(&state, actor, name.as_deref(), validate(definition)?).await
}

#[utoipa::path(
    post,
    path = "/api/v1/profiles/import",
    tag = "profiles",
    security(("bearer_auth" = [])),
    params(ImportParams),
    request_body(content = Object, description = DEFINITION_DESCRIPTION, content_type = "application/json"),
    responses(
        (status = 201, description = "The profile created from the exported document, inactive with revision 1", body = DataEnvelope<ProfileDetailDto>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 409, description = "A profile with this name already exists"),
        (status = 422, description = "The document is invalid; `details` lists the faulty field paths", body = ErrorBody),
    )
)]
pub async fn import_profile(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Query(params): Query<ImportParams>,
    body: Bytes,
) -> Result<HttpResponse, ApiError> {
    let actor = actor(user)?;
    let definition = validate(parse_document(&body)?)?;
    register(&state, actor, params.name.as_deref(), definition).await
}

#[utoipa::path(
    put,
    path = "/api/v1/profiles/{profile_id}",
    tag = "profiles",
    security(("bearer_auth" = [])),
    params(("profile_id" = String, Path, description = "Profile identifier (UUID)")),
    request_body = UpdateProfileRequest,
    responses(
        (status = 200, description = "The definition is identical to the current revision, which is returned unchanged", body = DataEnvelope<RevisionDto>),
        (status = 201, description = "A new revision was appended; past revisions are untouched", body = DataEnvelope<RevisionDto>),
        (status = 400, description = "Malformed profile identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown profile"),
        (status = 422, description = "The payload is invalid; `details` lists the faulty field paths", body = ErrorBody),
    )
)]
pub async fn update_profile(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
    body: Bytes,
) -> Result<HttpResponse, ApiError> {
    let actor = actor(user)?;
    let id = profile_id(&raw)?;
    let Value::Object(mut envelope) = parse_document(&body)? else {
        return Err(reject("$", "must be a JSON object"));
    };
    let definition = envelope
        .remove("definition")
        .ok_or_else(|| reject("definition", "is required"))?;
    let definition = validate(definition)?;
    let revised = state
        .usecase
        .revise_profile(ReviseProfileCommand {
            profile_id: id,
            definition,
            created_by: Some(actor),
        })
        .await?;
    let revision = revised.revision();
    if !revised.is_created() {
        return Ok(Response::OK(RevisionDto::from(revision)).into_response());
    }
    let location = format!(
        "{}/profiles/{}/revisions/{}",
        crate::api::API_PREFIX,
        id,
        revision.number.get()
    );
    Ok(created(location, RevisionDto::from(revision)))
}

async fn set_active(
    state: &AppState,
    user: Option<Extension<AuthenticatedUser>>,
    raw: &str,
    active: bool,
) -> Result<Response<ProfileDto>, ApiError> {
    actor(user)?;
    let profile = state
        .usecase
        .set_profile_active(profile_id(raw)?, active)
        .await?;
    Ok(Response::OK(ProfileDto::from(&profile)))
}

#[utoipa::path(
    post,
    path = "/api/v1/profiles/{profile_id}/activate",
    tag = "profiles",
    security(("bearer_auth" = [])),
    params(("profile_id" = String, Path, description = "Profile identifier (UUID)")),
    responses(
        (status = 200, description = "The profile, now scheduled", body = DataEnvelope<ProfileDto>),
        (status = 400, description = "Malformed profile identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown profile"),
    )
)]
pub async fn activate_profile(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
) -> Result<Response<ProfileDto>, ApiError> {
    set_active(&state, user, &raw, true).await
}

#[utoipa::path(
    post,
    path = "/api/v1/profiles/{profile_id}/deactivate",
    tag = "profiles",
    security(("bearer_auth" = [])),
    params(("profile_id" = String, Path, description = "Profile identifier (UUID)")),
    responses(
        (status = 200, description = "The profile, no longer scheduled; its leads are kept", body = DataEnvelope<ProfileDto>),
        (status = 400, description = "Malformed profile identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown profile"),
    )
)]
pub async fn deactivate_profile(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
) -> Result<Response<ProfileDto>, ApiError> {
    set_active(&state, user, &raw, false).await
}

fn export_filename(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{safe}.json")
}

#[utoipa::path(
    get,
    path = "/api/v1/profiles/{profile_id}/export",
    tag = "profiles",
    security(("bearer_auth" = [])),
    params(("profile_id" = String, Path, description = "Profile identifier (UUID)")),
    responses(
        (status = 200, description = "The current definition as a raw JSON document, downloadable as <name>.json", body = Object, content_type = "application/json"),
        (status = 400, description = "Malformed profile identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 404, description = "Unknown profile"),
    )
)]
pub async fn export_profile(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<HttpResponse, ApiError> {
    let current = current(&state, profile_id(&raw)?).await?;
    let disposition = format!(
        "attachment; filename=\"{}\"",
        export_filename(current.profile.name.as_str())
    );
    let header: (HeaderName, HeaderValue) = (
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).map_err(|_| ApiError::Internal)?,
    );
    Ok(([header], Json(current.revision.definition.to_json())).into_response())
}
