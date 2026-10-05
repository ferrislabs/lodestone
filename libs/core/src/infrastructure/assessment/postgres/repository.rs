use std::collections::HashMap;

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::{
    Assessment, AssessmentId, DraftEdit, DraftPage, DraftRecord, DraftReview, DraftStatus, Lead,
    LeadId, ProfileId, ProfileName,
    domain::lead::ports::AssessmentRepository,
    infrastructure::{
        assessment::postgres::model::{ASSESSMENT_COLUMNS, AssessmentRow, evidence_to_json},
        lead::postgres::{model::LeadRow, repository::NOT_SUPPRESSED},
        postgres::{SharedTx, error::map_sqlx_error},
    },
};

const LEAD_COLUMNS: &str = "id, profile_id, domain, status, score, score_breakdown, first_seen_at, last_seen_at, updated_at";

const LATEST_ASSESSMENT: &str = " AND assessments.id = (SELECT latest.id FROM assessments latest WHERE latest.lead_id = leads.id ORDER BY latest.created_at DESC, latest.id DESC LIMIT 1)";

fn push_queue_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, profile_id: Option<ProfileId>) {
    builder.push(
        " FROM assessments JOIN leads ON leads.id = assessments.lead_id WHERE assessments.draft_status = 'pending' AND leads.status = 'qualified'",
    );
    builder.push(LATEST_ASSESSMENT);
    if let Some(profile_id) = profile_id {
        builder
            .push(" AND leads.profile_id = ")
            .push_bind(profile_id.0);
    }
    builder.push(NOT_SUPPRESSED);
}

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
            INSERT INTO assessments (id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, draft_subject, draft_body, draft_status, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
            assessment.id.0,
            assessment.lead_id.0,
            revision,
            assessment.summary,
            assessment.approach_angle,
            evidence,
            assessment.model,
            assessment.prompt_version,
            assessment.draft.as_ref().map(|draft| draft.subject.as_str()),
            assessment.draft.as_ref().map(|draft| draft.body.as_str()),
            assessment
                .draft
                .as_ref()
                .map(|_| assessment
                    .draft_review
                    .as_ref()
                    .map_or(DraftStatus::Pending, |review| review.status)
                    .as_str()),
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
        let rows = sqlx::query_as::<_, AssessmentRow>(&format!(
            "SELECT {ASSESSMENT_COLUMNS} FROM assessments WHERE lead_id = $1 ORDER BY created_at DESC, id DESC"
        ))
        .bind(lead_id.0)
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(Assessment::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "assessments", assessment.id = %id), err)]
    async fn find_draft(&mut self, id: AssessmentId) -> Result<Option<DraftRecord>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = sqlx::query_as::<_, AssessmentRow>(&format!(
            "SELECT {ASSESSMENT_COLUMNS} FROM assessments WHERE id = $1 AND draft_status IS NOT NULL"
        ))
        .bind(id.0)
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let mut records = hydrate(&mut tx, vec![row]).await?;
        Ok(records.pop())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "assessments"), err)]
    async fn list_pending_drafts(
        &mut self,
        profile_id: Option<ProfileId>,
        limit: u32,
        offset: u64,
    ) -> Result<DraftPage, CoreError> {
        let mut count = QueryBuilder::<Postgres>::new("SELECT COUNT(*)");
        push_queue_filters(&mut count, profile_id);

        let mut select = QueryBuilder::<Postgres>::new(format!("SELECT {ASSESSMENT_COLUMNS}"));
        push_queue_filters(&mut select, profile_id);
        select.push(" ORDER BY leads.score DESC, assessments.created_at ASC, assessments.id ASC");
        select.push(" LIMIT ").push_bind(i64::from(limit));
        select
            .push(" OFFSET ")
            .push_bind(i64::try_from(offset).unwrap_or(i64::MAX));

        let mut tx = self.tx.lock().await;
        let total: i64 = count
            .build_query_scalar()
            .fetch_one(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        let rows = select
            .build_query_as::<AssessmentRow>()
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;

        Ok(DraftPage {
            items: hydrate(&mut tx, rows).await?,
            total: u64::try_from(total).unwrap_or_default(),
        })
    }

    #[tracing::instrument(skip(self, edit), fields(db.system = "postgresql", db.operation = "update", db.table = "assessments", assessment.id = %id), err)]
    async fn edit_draft(
        &mut self,
        id: AssessmentId,
        edit: &DraftEdit,
        at: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = sqlx::query(
            r#"
            UPDATE assessments
            SET draft_original_subject = COALESCE(draft_original_subject, draft_subject),
                draft_original_body    = COALESCE(draft_original_body, draft_body),
                draft_subject          = COALESCE($2, draft_subject),
                draft_body             = COALESCE($3, draft_body),
                draft_edited_at        = $4
            WHERE id = $1 AND draft_status = 'pending'
            "#,
        )
        .bind(id.0)
        .bind(edit.subject())
        .bind(edit.body())
        .bind(at)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(result.rows_affected() == 1)
    }

    #[tracing::instrument(skip(self, review), fields(db.system = "postgresql", db.operation = "update", db.table = "assessments", assessment.id = %id), err)]
    async fn decide_draft(
        &mut self,
        id: AssessmentId,
        review: &DraftReview,
    ) -> Result<bool, CoreError> {
        let mut tx = self.tx.lock().await;
        let result = sqlx::query(
            r#"
            UPDATE assessments
            SET draft_status           = $2,
                draft_reviewed_by      = $3,
                draft_reviewed_at      = $4,
                draft_rejection_reason = $5
            WHERE id = $1 AND draft_status = 'pending'
            "#,
        )
        .bind(id.0)
        .bind(review.status.as_str())
        .bind(review.reviewed_by.map(|user| user.0))
        .bind(review.reviewed_at)
        .bind(review.rejection_reason.as_deref())
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;

        Ok(result.rows_affected() == 1)
    }
}

async fn hydrate(
    conn: &mut sqlx::PgConnection,
    rows: Vec<AssessmentRow>,
) -> Result<Vec<DraftRecord>, CoreError> {
    let lead_ids: Vec<Uuid> = rows.iter().map(|row| row.lead_id).collect();
    let lead_rows = sqlx::query_as::<_, LeadRow>(&format!(
        "SELECT {LEAD_COLUMNS} FROM leads WHERE id = ANY($1)"
    ))
    .bind(&lead_ids)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_sqlx_error)?;
    let mut leads: HashMap<Uuid, Lead> = HashMap::with_capacity(lead_rows.len());
    for row in lead_rows {
        let lead = Lead::try_from(row)?;
        leads.insert(lead.id.0, lead);
    }

    let profile_ids: Vec<Uuid> = leads.values().map(|lead| lead.profile_id.0).collect();
    let profile_rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM profiles WHERE id = ANY($1)")
            .bind(&profile_ids)
            .fetch_all(&mut *conn)
            .await
            .map_err(map_sqlx_error)?;
    let mut names: HashMap<Uuid, ProfileName> = HashMap::with_capacity(profile_rows.len());
    for (id, name) in profile_rows {
        let name = ProfileName::parse(&name)
            .map_err(|_| CoreError::Internal("stored profile name is invalid".to_owned()))?;
        names.insert(id, name);
    }

    rows.into_iter()
        .map(|row| {
            let lead = leads
                .remove(&row.lead_id)
                .ok_or_else(|| CoreError::Internal("draft lead is missing".to_owned()))?;
            let profile_name = names
                .get(&lead.profile_id.0)
                .cloned()
                .ok_or_else(|| CoreError::Internal("draft profile is missing".to_owned()))?;
            Ok(DraftRecord {
                profile_id: lead.profile_id,
                profile_name,
                lead,
                assessment: Assessment::try_from(row)?,
            })
        })
        .collect()
}
