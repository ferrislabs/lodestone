use axum::{
    Extension,
    extract::{Path, Query, State},
};
use chrono::Utc;
use lodestone_core::{
    DomainEcosystem, EcosystemFinding, EcosystemModule, EcosystemReadiness, FindingExample,
    ModuleState, ModuleStats, ModuleStatus, PROVIDER_NOT_READY, ecosystem::DryRunError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

use crate::{
    auth::AuthenticatedUser,
    errors::ApiError,
    leads::lead_id,
    response::{DataEnvelope, Response},
    state::AppState,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = EcosystemModule)]
pub enum EcosystemModuleDto {
    OfficialIdentity,
    LegalLife,
    PublicContracts,
    WebsiteStack,
    DnsEmail,
    HiringActivity,
    RecentActivity,
}

impl From<EcosystemModule> for EcosystemModuleDto {
    fn from(module: EcosystemModule) -> Self {
        match module {
            EcosystemModule::OfficialIdentity => Self::OfficialIdentity,
            EcosystemModule::LegalLife => Self::LegalLife,
            EcosystemModule::PublicContracts => Self::PublicContracts,
            EcosystemModule::WebsiteStack => Self::WebsiteStack,
            EcosystemModule::DnsEmail => Self::DnsEmail,
            EcosystemModule::HiringActivity => Self::HiringActivity,
            EcosystemModule::RecentActivity => Self::RecentActivity,
        }
    }
}

impl From<EcosystemModuleDto> for EcosystemModule {
    fn from(module: EcosystemModuleDto) -> Self {
        match module {
            EcosystemModuleDto::OfficialIdentity => Self::OfficialIdentity,
            EcosystemModuleDto::LegalLife => Self::LegalLife,
            EcosystemModuleDto::PublicContracts => Self::PublicContracts,
            EcosystemModuleDto::WebsiteStack => Self::WebsiteStack,
            EcosystemModuleDto::DnsEmail => Self::DnsEmail,
            EcosystemModuleDto::HiringActivity => Self::HiringActivity,
            EcosystemModuleDto::RecentActivity => Self::RecentActivity,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = EcosystemModuleStatus)]
pub enum ModuleStatusDto {
    Pending,
    Done,
    NotApplicable,
    Failed,
}

impl From<ModuleStatus> for ModuleStatusDto {
    fn from(status: ModuleStatus) -> Self {
        match status {
            ModuleStatus::Pending => Self::Pending,
            ModuleStatus::Done => Self::Done,
            ModuleStatus::NotApplicable => Self::NotApplicable,
            ModuleStatus::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, ToSchema)]
#[schema(
    description = "What a module found about a lead domain, with the confirmed page or the API it was read from."
)]
pub struct EcosystemFindingDto {
    pub module: EcosystemModuleDto,
    pub key: String,
    pub kind: String,
    pub label: String,
    pub value: String,
    pub source_url: String,
    #[schema(format = DateTime)]
    pub observed_at: String,
    #[schema(value_type = Object)]
    pub details: Value,
}

impl From<&EcosystemFinding> for EcosystemFindingDto {
    fn from(finding: &EcosystemFinding) -> Self {
        Self {
            module: finding.module.into(),
            key: finding.key.clone(),
            kind: finding.kind.clone(),
            label: finding.label.clone(),
            value: finding.value.clone(),
            source_url: finding.source_url.clone(),
            observed_at: finding.observed_at.to_rfc3339(),
            details: finding.details.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct ModuleStateDto {
    pub module: EcosystemModuleDto,
    pub label: String,
    pub status: ModuleStatusDto,
    #[schema(format = DateTime)]
    pub last_run: Option<String>,
    pub error: Option<String>,
}

impl From<&ModuleState> for ModuleStateDto {
    fn from(state: &ModuleState) -> Self {
        Self {
            module: state.module.into(),
            label: state.module.label().to_owned(),
            status: state.status.into(),
            last_run: state.last_run.map(|at| at.to_rfc3339()),
            error: state.error.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, ToSchema)]
#[schema(
    description = "One module of the inspection of a lead: its status and its findings. A module that reads a catalog provider names it in `provider`; while that provider is not ready the module is skipped (`provider_ready` false, « Fournisseur non prêt »)."
)]
pub struct EcosystemModuleReportDto {
    #[serde(flatten)]
    pub state: ModuleStateDto,
    pub provider: Option<String>,
    pub provider_ready: bool,
    pub findings: Vec<EcosystemFindingDto>,
}

#[derive(Debug, Clone, Serialize, PartialEq, ToSchema)]
#[schema(
    description = "The ecosystem inspection of the lead domain, shared by every profile with a lead on that domain. A domain on a hosting platform has `hosting_platform` set and its site modules do not apply."
)]
pub struct EcosystemDto {
    pub hosting_platform: Option<String>,
    pub modules: Vec<EcosystemModuleReportDto>,
}

impl EcosystemDto {
    pub fn new(
        ecosystem: &DomainEcosystem,
        hosting_platform: Option<&str>,
        readiness: &EcosystemReadiness,
    ) -> Self {
        Self {
            hosting_platform: hosting_platform.map(str::to_owned),
            modules: EcosystemModule::AVAILABLE
                .into_iter()
                .map(|module| EcosystemModuleReportDto {
                    state: ModuleStateDto::from(&ecosystem.state_of(module)),
                    provider: module.provider().map(str::to_owned),
                    provider_ready: readiness.is_ready(module),
                    findings: ecosystem
                        .findings_of(module)
                        .map(EcosystemFindingDto::from)
                        .collect(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct FindingExampleDto {
    pub domain: String,
    pub label: String,
    pub value: String,
    pub source_url: String,
}

impl From<&FindingExample> for FindingExampleDto {
    fn from(example: &FindingExample) -> Self {
        Self {
            domain: example.domain.clone(),
            label: example.label.clone(),
            value: example.value.clone(),
            source_url: example.source_url.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
#[schema(description = "Activity of a module over the last seven days, across every profile.")]
pub struct ModuleStatsDto {
    pub leads_inspected: u64,
    pub findings_count: u64,
    #[schema(format = DateTime)]
    pub last_run: Option<String>,
    pub latest_example: Option<FindingExampleDto>,
}

impl From<&ModuleStats> for ModuleStatsDto {
    fn from(stats: &ModuleStats) -> Self {
        Self {
            leads_inspected: stats.leads_inspected,
            findings_count: stats.findings_count,
            last_run: stats.last_run.map(|at| at.to_rfc3339()),
            latest_example: stats.latest_example.as_ref().map(FindingExampleDto::from),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
#[schema(
    description = "A module of the ecosystem inspection, the same for every profile: the question it answers, what it learns, whether it is available yet and its activity."
)]
pub struct EcosystemModuleInfoDto {
    pub module: EcosystemModuleDto,
    pub label: String,
    pub question: String,
    pub learns: String,
    pub available: bool,
    pub reads_site: bool,
    pub provider: Option<String>,
    pub provider_ready: bool,
    pub refresh_days: i64,
    pub stats: ModuleStatsDto,
}

#[utoipa::path(
    get,
    path = "/api/v1/ecosystem/modules",
    tag = "ecosystem",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Every module of the ecosystem inspection with its activity over the last seven days", body = DataEnvelope<Vec<EcosystemModuleInfoDto>>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
    )
)]
pub async fn list_ecosystem_modules(
    State(state): State<AppState>,
) -> Result<Response<Vec<EcosystemModuleInfoDto>>, ApiError> {
    let stats = state.usecase.ecosystem_stats(Utc::now()).await?;
    let readiness = state.usecase.ecosystem_readiness().await?;
    Ok(Response::OK(
        EcosystemModule::ALL
            .into_iter()
            .map(|module| {
                let module_stats = stats
                    .iter()
                    .find(|found| found.module == module)
                    .cloned()
                    .unwrap_or_else(|| ModuleStats::empty(module));
                EcosystemModuleInfoDto {
                    module: module.into(),
                    label: module.label().to_owned(),
                    question: module.question().to_owned(),
                    learns: module.learns().to_owned(),
                    available: module.is_available(),
                    reads_site: module.reads_site(),
                    provider: module.provider().map(str::to_owned),
                    provider_ready: readiness.is_ready(module),
                    refresh_days: module.refresh_interval().num_days(),
                    stats: ModuleStatsDto::from(&module_stats),
                }
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DryRunQuery {
    pub module: EcosystemModuleDto,
}

#[derive(Debug, Clone, Serialize, PartialEq, ToSchema)]
#[schema(description = "Result of a module run on demand. Nothing is stored.")]
pub struct EcosystemDryRunDto {
    pub module: EcosystemModuleDto,
    pub domain: String,
    pub status: ModuleStatusDto,
    pub hosting_platform: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub findings: Vec<EcosystemFindingDto>,
}

#[utoipa::path(
    post,
    path = "/api/v1/leads/{lead_id}/ecosystem/dry-run",
    tag = "ecosystem",
    security(("bearer_auth" = [])),
    params(("lead_id" = String, Path, description = "Lead identifier (UUID)"), DryRunQuery),
    responses(
        (status = 200, description = "The module ran now on the lead domain; its findings are returned and nothing is stored", body = DataEnvelope<EcosystemDryRunDto>),
        (status = 400, description = "Malformed lead identifier or unknown module"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown lead, or its domain is suppressed"),
        (status = 409, description = "The module is not available yet, or its catalog provider is not ready (« Fournisseur non prêt »)"),
        (status = 429, description = "A host of the module is rate limited, try again later"),
    )
)]
pub async fn dry_run_ecosystem_module(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(lead_id_raw): Path<String>,
    Query(query): Query<DryRunQuery>,
) -> Result<Response<EcosystemDryRunDto>, ApiError> {
    if user.is_none() {
        return Err(ApiError::Forbidden);
    }
    let lead = lead_id(&lead_id_raw)?;
    let runners = state.ecosystem.runners();
    let dry_run = state
        .usecase
        .dry_run_ecosystem_module(&runners, lead, query.module.into(), Utc::now())
        .await
        .map_err(|error| match error {
            DryRunError::Unavailable(module) => {
                ApiError::Conflict(format!("the module `{module}` is not available yet"))
            }
            DryRunError::ProviderNotReady(provider) => {
                ApiError::Conflict(format!("{PROVIDER_NOT_READY} : {provider}"))
            }
            DryRunError::RateLimited { .. } => ApiError::TooManyRequests,
            DryRunError::Core(error) => ApiError::from(error),
        })?;
    Ok(Response::OK(EcosystemDryRunDto {
        module: dry_run.module.into(),
        domain: dry_run.domain.as_str().to_owned(),
        status: dry_run.status.into(),
        hosting_platform: dry_run.hosting_platform.map(str::to_owned),
        error: dry_run.error,
        duration_ms: u64::try_from(dry_run.duration.as_millis()).unwrap_or(u64::MAX),
        findings: dry_run
            .findings
            .iter()
            .map(EcosystemFindingDto::from)
            .collect(),
    }))
}
