use common::CoreError;
use lodestone_macros::repository;

use crate::{
    Assessment, LeadId,
    domain::lead::ports::AssessmentRepository,
    infrastructure::{
        assessment::postgres::model::{AssessmentRow, evidence_to_json},
        postgres::{SharedTx, error::map_sqlx_error},
    },
};

#[repository(domain = Assessment, backend = Postgres)]
pub struct PgAssessmentRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgAssessmentRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> AssessmentRepository for PgAssessmentRepository<'tx> {
    #[tracing::instrument(skip(self, assessment), fields(db.system = "postgresql", db.operation = "insert", db.table = "assessments", assessment.id = %assessment.id), err)]
    async fn insert(&mut self, assessment: &Assessment) -> Result<(), CoreError> {
        let revision = i32::try_from(assessment.profile_revision.get())
            .map_err(|_| CoreError::Internal("profile revision number out of range".to_owned()))?;
        let evidence = evidence_to_json(&assessment.evidence)?;
        let mut tx = self.tx.lock().await;
        sqlx::query!(
            r#"
            INSERT INTO assessments (id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            assessment.id.0,
            assessment.lead_id.0,
            revision,
            assessment.summary,
            assessment.approach_angle,
            evidence,
            assessment.model,
            assessment.prompt_version,
            assessment.created_at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "assessments", lead.id = %lead_id), err)]
    async fn list_for_lead(&mut self, lead_id: LeadId) -> Result<Vec<Assessment>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = sqlx::query_as!(
            AssessmentRow,
            r#"
            SELECT id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, created_at
            FROM assessments
            WHERE lead_id = $1
            ORDER BY created_at DESC, id DESC
            "#,
            lead_id.0,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(Assessment::try_from).collect()
    }
}
