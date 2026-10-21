use std::iter::once;

use axum::{
    Extension,
    extract::{Path, State},
};
use lodestone_core::{
    JobStatus, ListenerStatus, ProviderConfiguration, ProviderStatus, QUALIFICATION_LISTENER,
    ScheduledJob, SourceName, SourceRunRecord,
    company::COMPANY_RESEARCH_LISTENER,
    ecosystem::ECOSYSTEM_LISTENER,
    is_continuous, provider_descriptor, provider_key_of_source,
    schedule::{schedule_key, source_of_schedule_key},
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    auth::AuthenticatedUser,
    errors::ApiError,
    response::{DataEnvelope, Response},
    state::AppState,
};

const MAX_SOURCE_NAME_LEN: usize = 64;
const SOURCE_JOB_KIND: &str = "source.run";
const QUALIFICATION_LISTENER_KIND: &str = "listener.qualification";
const COMPANY_LISTENER_KIND: &str = "listener.company_research";
const ECOSYSTEM_LISTENER_KIND: &str = "listener.ecosystem_inspection";

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct SourceRunDto {
    pub profile_id: String,
    pub profile_name: String,
    pub agent_id: Option<String>,
    /// One of `running`, `succeeded`, `failed`, `cancelled`.
    pub status: String,
    pub volume: u64,
    pub signals: u64,
    pub error: Option<String>,
    /// Why a `cancelled` run stopped: `agent_removed`, `agent_disabled`, `profile_deactivated` or `profile_deleted`.
    pub cancel_reason: Option<String>,
    #[schema(format = DateTime)]
    pub started_at: String,
    #[schema(format = DateTime)]
    pub finished_at: Option<String>,
}

impl From<&SourceRunRecord> for SourceRunDto {
    fn from(record: &SourceRunRecord) -> Self {
        Self {
            profile_id: record.profile_id.0.to_string(),
            profile_name: record.profile_name.clone(),
            agent_id: record.agent_id.clone(),
            status: record.status.as_str().to_owned(),
            volume: record.volume,
            signals: record.signals,
            error: record.error.clone(),
            cancel_reason: record
                .cancel_reason
                .map(|reason| reason.as_str().to_owned()),
            started_at: record.started_at.to_rfc3339(),
            finished_at: record.finished_at.map(|at| at.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct ListenerDto {
    /// One of `standby`, `listening`, `idle`, `idle_no_agent`, `error`, `stopped`, `stale`.
    pub state: String,
    /// Instance currently holding the listener, when one does.
    pub holder: Option<String>,
    #[schema(format = DateTime)]
    pub last_activity_at: Option<String>,
    pub last_error: Option<String>,
    pub items_total: u64,
    pub signals_total: u64,
    /// Work waiting: lag in tiles (certificate transparency), due hosts, due contact targets, leads to qualify or leads to summarise.
    pub backlog: Option<u64>,
    /// Last time the holder reported in; a listener silent for too long is `stale`.
    #[schema(format = DateTime)]
    pub updated_at: Option<String>,
}

impl ListenerDto {
    fn from_status(status: Option<&ListenerStatus>) -> Self {
        match status {
            Some(status) => Self {
                state: status.state.as_str().to_owned(),
                holder: status.holder.clone(),
                last_activity_at: status.last_activity_at.map(|at| at.to_rfc3339()),
                last_error: status.last_error.clone(),
                items_total: status.items_total,
                signals_total: status.signals_total,
                backlog: status.backlog,
                updated_at: Some(status.updated_at.to_rfc3339()),
            },
            None => Self {
                state: "stopped".to_owned(),
                holder: None,
                last_activity_at: None,
                last_error: None,
                items_total: 0,
                signals_total: 0,
                backlog: None,
                updated_at: None,
            },
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct SourceProviderRefDto {
    /// Key of the catalog provider the source depends on.
    pub key: String,
    pub name: String,
    /// One of `not_configured`, `untested`, `ready`, `failing`. The source only runs when `ready`.
    pub status: String,
}

impl SourceProviderRefDto {
    fn of(source: &str, configurations: &[ProviderConfiguration]) -> Option<Self> {
        let key = provider_key_of_source(source)?;
        let descriptor = provider_descriptor(key)?;
        let status = configurations
            .iter()
            .find(|configuration| configuration.key == key)
            .map_or(ProviderStatus::NotConfigured, |configuration| {
                configuration.status
            });
        Some(Self {
            key: key.to_owned(),
            name: descriptor.name.to_owned(),
            status: status.as_str().to_owned(),
        })
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, ToSchema)]
pub struct SourceDto {
    pub source: String,
    /// Catalog provider the source depends on and its status; absent for sources without a provider.
    pub provider: Option<SourceProviderRefDto>,
    /// `continuous` for sources that listen around the clock, `scheduled` for recurring jobs.
    pub mode: String,
    /// Listener state of a continuous source; absent for scheduled sources.
    pub listener: Option<ListenerDto>,
    /// Job kind: `source.run`, `listener.qualification`, `listener.company_research` or `listener.ecosystem_inspection`.
    pub kind: String,
    /// Translation key of the source label.
    pub label_key: String,
    /// One of `pending`, `running`, `succeeded`, `failed`, `cancelled`, `unavailable` for scheduled sources;
    /// the listener state for continuous ones.
    pub status: String,
    /// Why the source has no job; set only when `status` is `unavailable`. Always `not_scheduled`.
    pub unavailable_reason: Option<String>,
    #[schema(format = DateTime)]
    pub next_run_at: Option<String>,
    pub attempts: u16,
    #[schema(format = DateTime)]
    pub lease_until: Option<String>,
    #[schema(format = DateTime)]
    pub running_since: Option<String>,
    pub last_error: Option<String>,
    /// Latest runs of the source across profiles, newest first.
    pub last_runs: Vec<SourceRunDto>,
}

impl SourceDto {
    fn with_provider(mut self, configurations: &[ProviderConfiguration]) -> Self {
        self.provider = SourceProviderRefDto::of(&self.source, configurations);
        self
    }
}

fn label_key(source: SourceName) -> String {
    format!("source.{}", source.as_str())
}

impl SourceDto {
    fn scheduled(source: SourceName, job: &ScheduledJob, runs: &[&SourceRunRecord]) -> Self {
        let running = job.status == JobStatus::Running;
        Self {
            source: source.as_str().to_owned(),
            provider: None,
            mode: "scheduled".to_owned(),
            listener: None,
            kind: job.kind.as_str().to_owned(),
            label_key: label_key(source),
            status: job.status.as_str().to_owned(),
            unavailable_reason: None,
            next_run_at: Some(job.run_at.to_rfc3339()),
            attempts: job.attempts,
            lease_until: job.lease_until.map(|at| at.to_rfc3339()),
            running_since: running.then(|| job.updated_at.to_rfc3339()),
            last_error: job.last_error.clone(),
            last_runs: runs.iter().map(|run| SourceRunDto::from(*run)).collect(),
        }
    }

    fn continuous(
        name: &str,
        kind: &str,
        status: Option<&ListenerStatus>,
        runs: &[&SourceRunRecord],
    ) -> Self {
        let listener = ListenerDto::from_status(status);
        Self {
            source: name.to_owned(),
            provider: None,
            mode: "continuous".to_owned(),
            kind: kind.to_owned(),
            label_key: format!("source.{name}"),
            status: listener.state.clone(),
            unavailable_reason: None,
            next_run_at: None,
            attempts: 0,
            lease_until: None,
            running_since: None,
            last_error: listener.last_error.clone(),
            last_runs: runs.iter().map(|run| SourceRunDto::from(*run)).collect(),
            listener: Some(listener),
        }
    }

    fn unavailable(source: SourceName, runs: &[&SourceRunRecord]) -> Self {
        Self {
            source: source.as_str().to_owned(),
            provider: None,
            mode: "scheduled".to_owned(),
            listener: None,
            kind: SOURCE_JOB_KIND.to_owned(),
            label_key: label_key(source),
            status: "unavailable".to_owned(),
            unavailable_reason: Some("not_scheduled".to_owned()),
            next_run_at: None,
            attempts: 0,
            lease_until: None,
            running_since: None,
            last_error: None,
            last_runs: runs.iter().map(|run| SourceRunDto::from(*run)).collect(),
        }
    }
}

fn parse_source(raw: &str) -> Result<SourceName, ApiError> {
    let well_formed = !raw.is_empty()
        && raw.len() <= MAX_SOURCE_NAME_LEN
        && raw
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if !well_formed {
        return Err(ApiError::BadRequest(
            "source: must be a lowercase source name".to_owned(),
        ));
    }
    raw.parse()
        .map_err(|_| ApiError::ResourceNotFound("Source", raw.to_owned()))
}

#[utoipa::path(
    get,
    path = "/api/v1/sources",
    tag = "sources",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Every known source with its job state or listener state and latest runs; a scheduled source without a job is `unavailable`", body = DataEnvelope<Vec<SourceDto>>),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
    )
)]
pub async fn list_sources(
    State(state): State<AppState>,
) -> Result<Response<Vec<SourceDto>>, ApiError> {
    let schedule = state.usecase.list_source_schedule().await?;
    let listeners = state.usecase.list_listener_statuses().await?;
    let configurations = state.usecase.list_provider_configurations().await?;
    let sources = SourceName::ALL
        .into_iter()
        .map(|source| {
            let runs: Vec<&SourceRunRecord> = schedule
                .runs
                .iter()
                .filter(|run| run.source == source)
                .collect();
            if is_continuous(source) {
                let status = listeners
                    .iter()
                    .find(|listener| listener.name == source.as_str());
                return SourceDto::continuous(source.as_str(), SOURCE_JOB_KIND, status, &runs)
                    .with_provider(&configurations);
            }
            match schedule
                .jobs
                .iter()
                .find(|job| source_of_schedule_key(&job.dedup_key) == Some(source))
            {
                Some(job) => SourceDto::scheduled(source, job, &runs),
                None => SourceDto::unavailable(source, &runs),
            }
            .with_provider(&configurations)
        })
        .chain(once(SourceDto::continuous(
            QUALIFICATION_LISTENER,
            QUALIFICATION_LISTENER_KIND,
            listeners
                .iter()
                .find(|listener| listener.name == QUALIFICATION_LISTENER),
            &[],
        )))
        .chain(once(
            SourceDto::continuous(
                COMPANY_RESEARCH_LISTENER,
                COMPANY_LISTENER_KIND,
                listeners
                    .iter()
                    .find(|listener| listener.name == COMPANY_RESEARCH_LISTENER),
                &[],
            )
            .with_provider(&configurations),
        ))
        .chain(once(SourceDto::continuous(
            ECOSYSTEM_LISTENER,
            ECOSYSTEM_LISTENER_KIND,
            listeners
                .iter()
                .find(|listener| listener.name == ECOSYSTEM_LISTENER),
            &[],
        )))
        .collect();
    Ok(Response::OK(sources))
}

#[utoipa::path(
    post,
    path = "/api/v1/sources/{source}/run",
    tag = "sources",
    security(("bearer_auth" = [])),
    params(("source" = String, Path, description = "Source name, for example `boamp` or `decp`")),
    responses(
        (status = 202, description = "The job is due now; the worker picks it up within its poll interval", body = DataEnvelope<SourceDto>),
        (status = 400, description = "Malformed source name"),
        (status = 401, description = "Missing, malformed, invalid or expired bearer token"),
        (status = 403, description = "The caller is not a user"),
        (status = 404, description = "Unknown source, or the source has no scheduled job"),
        (status = 409, description = "The source listens continuously, or its catalog provider is not ready"),
    )
)]
pub async fn run_source(
    State(state): State<AppState>,
    user: Option<Extension<AuthenticatedUser>>,
    Path(raw): Path<String>,
) -> Result<Response<SourceDto>, ApiError> {
    if user.is_none() {
        return Err(ApiError::Forbidden);
    }
    let source = parse_source(&raw)?;
    if is_continuous(source) {
        return Err(ApiError::Conflict("source listens continuously".to_owned()));
    }
    if let Some(key) = provider_key_of_source(source.as_str())
        && !state.usecase.is_provider_usable(key).await?
    {
        let name = provider_descriptor(key).map_or(key, |descriptor| descriptor.name);
        return Err(ApiError::Conflict(format!(
            "Le fournisseur « {name} » n'est pas prêt : configurez-le et testez-le dans le catalogue avant de lancer la source."
        )));
    }
    match state.usecase.run_job_now(&schedule_key(source)).await {
        Ok(job) => Ok(Response::Accepted(SourceDto::scheduled(source, &job, &[]))),
        Err(common::CoreError::NotFound) => Err(ApiError::NotFoundBecause(
            "source has no scheduled job".to_owned(),
        )),
        Err(other) => Err(other.into()),
    }
}
