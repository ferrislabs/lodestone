use axum::{
    Extension,
    extract::{Path, State},
};
use lodestone_core::{CompanyDossier, CompanyEntity, OfficialData, SiblingLead};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    auth::AuthenticatedUser,
    errors::ApiError,
    leads::{VerdictDto, lead_id},
    profiles::ProfileRefDto,
    response::Response,
    state::AppState,
};

/// Another lead of the same company, found through the SIREN shared by the dossiers of the two domains.
#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct SiblingLeadDto {
    pub id: String,
    pub domain: String,
    pub profile: ProfileRefDto,
    pub verdict: Option<VerdictDto>,
    pub confidence: Option<f64>,
}

impl From<&SiblingLead> for SiblingLeadDto {
    fn from(sibling: &SiblingLead) -> Self {
        Self {
            id: sibling.id.to_string(),
            domain: sibling.domain.as_str().to_owned(),
            profile: ProfileRefDto {
                id: sibling.profile_id.to_string(),
                name: sibling.profile_name.as_str().to_owned(),
            },
            verdict: sibling.verdict.map(VerdictDto::from),
            confidence: sibling.confidence,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct OfficialDataDto {
    pub siren: String,
    pub legal_name: String,
    /// Activity code of the main activity (NAF), for example `62.01Z`.
    pub naf_code: Option<String>,
    /// Label of the NAF section the code belongs to.
    pub naf_label: Option<String>,
    pub headcount_range: Option<String>,
    pub head_office_city: Option<String>,
    pub head_office_postcode: Option<String>,
    pub head_office_country: Option<String>,
    /// Creation date, `YYYY-MM-DD`.
    pub created_on: Option<String>,
    /// Public page of the company on the Annuaire des entreprises.
    pub annuaire_url: String,
}

impl From<&OfficialData> for OfficialDataDto {
    fn from(official: &OfficialData) -> Self {
        Self {
            siren: official.siren.to_string(),
            legal_name: official.legal_name.clone(),
            naf_code: official.naf_code.clone(),
            naf_label: official.naf_label.clone(),
            headcount_range: official.headcount_range.clone(),
            head_office_city: official.head_office_city.clone(),
            head_office_postcode: official.head_office_postcode.clone(),
            head_office_country: official.head_office_country.clone(),
            created_on: official.created_on.map(|date| date.to_string()),
            annuaire_url: official.public_page(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct CompanyPageDto {
    pub url: String,
}

/// Company dossier of the lead domain: official register data and the pages of the company site, built without any model.
#[derive(Debug, Serialize, PartialEq, ToSchema)]
pub struct CompanyDto {
    /// One of pending, complete, partial, unreachable.
    pub state: String,
    /// SIREN found on the company site, even when the official lookup failed.
    pub siren: Option<String>,
    pub official: Option<OfficialDataDto>,
    /// Legal name of the company entity the domain belongs to; absent until the SIREN is resolved against the official register.
    pub legal_name: Option<String>,
    /// Other leads of the same company, in every profile; empty until the SIREN is resolved.
    pub siblings: Vec<SiblingLeadDto>,
    /// Pages of the company site that were read.
    pub pages: Vec<CompanyPageDto>,
    #[schema(format = DateTime)]
    pub built_at: Option<String>,
    #[schema(format = DateTime)]
    pub next_attempt_at: String,
    /// Reason of the last failure, when the last build failed.
    pub error: Option<String>,
}

impl From<&CompanyDossier> for CompanyDto {
    fn from(dossier: &CompanyDossier) -> Self {
        Self {
            state: dossier.state.as_str().to_owned(),
            siren: dossier.siren.as_ref().map(ToString::to_string),
            official: dossier.official.as_ref().map(OfficialDataDto::from),
            legal_name: None,
            siblings: Vec::new(),
            pages: dossier
                .pages
                .iter()
                .map(|page| CompanyPageDto {
                    url: page.url.clone(),
                })
                .collect(),
            built_at: dossier.built_at.map(|at| at.to_rfc3339()),
            next_attempt_at: dossier.next_attempt_at.to_rfc3339(),
            error: dossier.error.clone(),
        }
    }
}

impl CompanyDto {
    pub fn with_entity(mut self, entity: Option<&CompanyEntity>, siblings: &[SiblingLead]) -> Self {
        if let Some(entity) = entity {
            self.siren = Some(entity.siren.to_string());
            self.legal_name = Some(entity.legal_name.clone());
        }
        self.siblings = siblings.iter().map(SiblingLeadDto::from).collect();
        self
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/leads/{lead_id}/company/refresh",
    tag = "leads",
    security(("bearer_auth" = [])),
    params(("lead_id" = String, Path, description = "Lead identifier (UUID)")),
    responses(
        (status = 202, description = "The dossier of the lead domain is due now; the company research listener builds it within its next loop", body = crate::response::DataEnvelope<CompanyDto>),
        (status = 400, description = "Malformed lead identifier"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown lead, or its domain is suppressed"),
    )
)]
pub async fn refresh_company(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(lead_id_raw): Path<String>,
) -> Result<Response<CompanyDto>, ApiError> {
    if user.is_none() {
        return Err(ApiError::Forbidden);
    }
    let dossier = state
        .usecase
        .request_company_refresh(lead_id(&lead_id_raw)?)
        .await?;
    Ok(Response::Accepted(CompanyDto::from(&dossier)))
}
