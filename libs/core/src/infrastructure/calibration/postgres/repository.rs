use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    ProfileId,
    domain::{
        calibration::{
            Calibration, CalibrationKind, IsotonicMap, LabelledQualification, LabelledScoring,
            Point, ReliabilityBin, ports::CalibrationRepository,
        },
        profile::definition::AgentId,
    },
    infrastructure::{
        lead::postgres::repository::NOT_SUPPRESSED,
        postgres::{SharedTx, error::map_sqlx_error},
    },
};

#[repository(domain = Calibration, backend = Postgres)]
pub struct PgCalibrationRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgCalibrationRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct CalibrationRow {
    profile_id: Uuid,
    kind: String,
    agent_id: String,
    points: Value,
    n: i32,
    positives: i32,
    brier_before: Option<f64>,
    brier_after: Option<f64>,
    reliability: Value,
    fitted_at: DateTime<Utc>,
    enabled: bool,
}

fn stored(message: &str) -> CoreError {
    CoreError::Internal(format!("stored calibration {message} is invalid"))
}

impl TryFrom<CalibrationRow> for Calibration {
    type Error = CoreError;

    fn try_from(row: CalibrationRow) -> Result<Self, Self::Error> {
        let points: Vec<Point> =
            serde_json::from_value(row.points).map_err(|_| stored("points"))?;
        let reliability: Vec<ReliabilityBin> =
            serde_json::from_value(row.reliability).map_err(|_| stored("curve"))?;
        Ok(Self {
            profile_id: ProfileId(row.profile_id),
            kind: CalibrationKind::parse(&row.kind).ok_or_else(|| stored("kind"))?,
            agent_id: AgentId::new(row.agent_id),
            map: IsotonicMap::from_points(points),
            n: u64::try_from(row.n).map_err(|_| stored("count"))?,
            positives: u64::try_from(row.positives).map_err(|_| stored("positives"))?,
            brier_before: row.brier_before,
            brier_after: row.brier_after,
            reliability,
            fitted_at: row.fitted_at,
            enabled: row.enabled,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ScoringRow {
    agent_id: String,
    validated: bool,
    verdict: Value,
}

#[derive(Debug, sqlx::FromRow)]
struct QualificationRow {
    agent_id: String,
    validated: bool,
    answers: Value,
}

const LATEST_LEAD_LABEL: &str = r#"
    SELECT DISTINCT ON (labels.lead_id) labels.lead_id, labels.kind
    FROM decision_labels labels
    JOIN leads ON leads.id = labels.lead_id
    WHERE labels.profile_id = $1
      AND labels.kind IN ('lead_validated', 'lead_rejected')
"#;

const LATEST_ORDER: &str = " ORDER BY labels.lead_id, labels.decided_at DESC, labels.id DESC";

impl<'tx> CalibrationRepository for PgCalibrationRepository<'tx> {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "calibrations", profile.id = %profile_id), err)]
    async fn list(&mut self, profile_id: ProfileId) -> Result<Vec<Calibration>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, CalibrationRow>(
            "SELECT profile_id, kind, agent_id, points, n, positives, brier_before, brier_after, reliability, fitted_at, enabled
             FROM calibrations
             WHERE profile_id = $1
             ORDER BY kind ASC, agent_id ASC",
        )
        .bind(profile_id.0)
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter().map(Calibration::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "scoring_results", profile.id = %profile_id), err)]
    async fn labelled_scoring(
        &mut self,
        profile_id: ProfileId,
    ) -> Result<Vec<LabelledScoring>, CoreError> {
        let query = format!(
            "WITH latest AS ({LATEST_LEAD_LABEL}{NOT_SUPPRESSED}{LATEST_ORDER})
             SELECT r.agent_id, (latest.kind = 'lead_validated') AS validated, r.verdict
             FROM latest
             JOIN lead_qualification_state s ON s.lead_id = latest.lead_id
             JOIN scoring_results r
               ON r.lead_id = s.lead_id AND r.profile_revision = s.profile_revision
             WHERE r.status = 'valid' AND r.verdict IS NOT NULL
             ORDER BY latest.lead_id, r.agent_id"
        );
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, ScoringRow>(&query)
            .bind(profile_id.0)
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|row| LabelledScoring {
                agent_id: AgentId::new(row.agent_id),
                validated: row.validated,
                verdict: row.verdict,
            })
            .collect())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "qualification_results", profile.id = %profile_id), err)]
    async fn labelled_qualification(
        &mut self,
        profile_id: ProfileId,
    ) -> Result<Vec<LabelledQualification>, CoreError> {
        let query = format!(
            "WITH latest AS ({LATEST_LEAD_LABEL}{NOT_SUPPRESSED}{LATEST_ORDER})
             SELECT r.agent_id, (latest.kind = 'lead_validated') AS validated, r.answers
             FROM latest
             JOIN lead_qualification_state s ON s.lead_id = latest.lead_id
             JOIN qualification_results r
               ON r.lead_id = s.lead_id AND r.profile_revision = s.profile_revision
             WHERE r.status = 'valid'
             ORDER BY latest.lead_id, r.agent_id"
        );
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as::<_, QualificationRow>(&query)
            .bind(profile_id.0)
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|row| LabelledQualification {
                agent_id: AgentId::new(row.agent_id),
                validated: row.validated,
                answers: row.answers,
            })
            .collect())
    }

    #[tracing::instrument(skip(self, calibrations), fields(db.system = "postgresql", db.operation = "insert", db.table = "calibrations", profile.id = %profile_id), err)]
    async fn replace(
        &mut self,
        profile_id: ProfileId,
        calibrations: &[Calibration],
    ) -> Result<(), CoreError> {
        let mut tx = self.tx.lock().await;
        let kinds: Vec<&str> = calibrations
            .iter()
            .map(|calibration| calibration.kind.as_str())
            .collect();
        let agents: Vec<&str> = calibrations
            .iter()
            .map(|calibration| calibration.agent_id.as_str())
            .collect();
        sqlx::query(
            "DELETE FROM calibrations
             WHERE profile_id = $1
               AND (kind, agent_id) NOT IN (SELECT * FROM UNNEST($2::text[], $3::text[]))",
        )
        .bind(profile_id.0)
        .bind(&kinds)
        .bind(&agents)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        for calibration in calibrations {
            let points = serde_json::to_value(
                calibration
                    .map
                    .as_ref()
                    .map(IsotonicMap::points)
                    .unwrap_or_default(),
            )
            .map_err(|error| CoreError::Internal(error.to_string()))?;
            let reliability = serde_json::to_value(&calibration.reliability)
                .map_err(|error| CoreError::Internal(error.to_string()))?;
            let n = i32::try_from(calibration.n)
                .map_err(|_| CoreError::Internal("calibration count out of range".to_owned()))?;
            let positives = i32::try_from(calibration.positives)
                .map_err(|_| CoreError::Internal("calibration count out of range".to_owned()))?;
            sqlx::query(
                "INSERT INTO calibrations (profile_id, kind, agent_id, points, n, positives, brier_before, brier_after, reliability, fitted_at, enabled)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                 ON CONFLICT (profile_id, kind, agent_id) DO UPDATE SET
                    points = EXCLUDED.points,
                    n = EXCLUDED.n,
                    positives = EXCLUDED.positives,
                    brier_before = EXCLUDED.brier_before,
                    brier_after = EXCLUDED.brier_after,
                    reliability = EXCLUDED.reliability,
                    fitted_at = EXCLUDED.fitted_at,
                    enabled = EXCLUDED.enabled",
            )
            .bind(profile_id.0)
            .bind(calibration.kind.as_str())
            .bind(calibration.agent_id.as_str())
            .bind(points)
            .bind(n)
            .bind(positives)
            .bind(calibration.brier_before)
            .bind(calibration.brier_after)
            .bind(reliability)
            .bind(calibration.fitted_at)
            .bind(calibration.enabled)
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "lead_qualification_state", profile.id = %profile_id), err)]
    async fn mark_stale(&mut self, profile_id: ProfileId) -> Result<u64, CoreError> {
        let mut tx = self.tx.lock().await;
        let done = sqlx::query(
            "UPDATE lead_qualification_state state
             SET scored_at = NULL
             FROM leads
             WHERE leads.id = state.lead_id
               AND leads.profile_id = $1
               AND state.scored_at IS NOT NULL",
        )
        .bind(profile_id.0)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(done.rows_affected())
    }
}
