use chrono::{DateTime, Utc};
use common::CoreError;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    LeadId, NormalizedDomain, ProfileId, UserId, ValidatedProfile, Verdict,
    domain::trial::{
        DecidedLead, ProfileTrial, SampleSize, TrialBudget, TrialId, TrialLabel, TrialLead,
        TrialStatus,
    },
    infrastructure::profile::postgres::model::revision_number,
};

fn stored(field: &str) -> CoreError {
    CoreError::Internal(format!("stored trial {field} is invalid"))
}

fn verdict(raw: Option<String>) -> Result<Option<Verdict>, CoreError> {
    raw.map(|raw| Verdict::parse(&raw).ok_or_else(|| stored("verdict")))
        .transpose()
}

fn label(raw: &str) -> Result<TrialLabel, CoreError> {
    TrialLabel::parse(raw).ok_or_else(|| stored("label"))
}

#[derive(Debug, sqlx::FromRow)]
pub struct TrialRow {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub base_revision: i32,
    pub definition: Value,
    pub status: String,
    pub sample_size: i32,
    pub budget_cents: i32,
    pub spent_cents: f64,
    pub error: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub leads_tried: i64,
}

impl TryFrom<TrialRow> for ProfileTrial {
    type Error = CoreError;

    fn try_from(row: TrialRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: TrialId(row.id),
            profile_id: ProfileId(row.profile_id),
            base_revision: revision_number(row.base_revision)?,
            definition: ValidatedProfile::parse(row.definition)
                .map_err(|_| stored("definition"))?,
            status: TrialStatus::parse(&row.status).ok_or_else(|| stored("status"))?,
            sample_size: u32::try_from(row.sample_size)
                .ok()
                .and_then(|size| SampleSize::new(size).ok())
                .ok_or_else(|| stored("sample size"))?,
            budget: u32::try_from(row.budget_cents)
                .ok()
                .and_then(|cents| TrialBudget::new(cents).ok())
                .ok_or_else(|| stored("budget"))?,
            spent_cents: row.spent_cents,
            leads_tried: u32::try_from(row.leads_tried).map_err(|_| stored("lead count"))?,
            error: row.error,
            created_by: row.created_by.map(UserId),
            created_at: row.created_at,
            started_at: row.started_at,
            finished_at: row.finished_at,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct TrialLeadRow {
    pub lead_id: Uuid,
    pub domain: String,
    pub label: String,
    pub current_verdict: Option<String>,
    pub current_confidence: Option<f64>,
    pub trial_verdict: Option<String>,
    pub trial_confidence: Option<f64>,
    pub error: Option<String>,
    pub tried_at: DateTime<Utc>,
}

impl TryFrom<TrialLeadRow> for TrialLead {
    type Error = CoreError;

    fn try_from(row: TrialLeadRow) -> Result<Self, Self::Error> {
        Ok(Self {
            lead_id: LeadId(row.lead_id),
            domain: row
                .domain
                .parse::<NormalizedDomain>()
                .map_err(|_| stored("domain"))?,
            label: label(&row.label)?,
            current_verdict: verdict(row.current_verdict)?,
            current_confidence: row.current_confidence,
            trial_verdict: verdict(row.trial_verdict)?,
            trial_confidence: row.trial_confidence,
            error: row.error,
            tried_at: row.tried_at,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct DecidedRow {
    pub lead_id: Uuid,
    pub kind: String,
}

impl TryFrom<DecidedRow> for DecidedLead {
    type Error = CoreError;

    fn try_from(row: DecidedRow) -> Result<Self, Self::Error> {
        let label = match row.kind.as_str() {
            "lead_validated" => TrialLabel::Validated,
            "lead_rejected" => TrialLabel::Rejected,
            _ => return Err(stored("label")),
        };
        Ok(Self {
            lead_id: LeadId(row.lead_id),
            label,
        })
    }
}
