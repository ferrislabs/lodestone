use std::collections::HashMap;

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::{
    Assessment, AssessmentId, DraftEdit, DraftPage, DraftRecord, DraftReview, Lead, LeadId,
    ProfileId, ProfileName,
    domain::lead::{ports::AssessmentRepository, query::VerdictFilter},
    infrastructure::{
        assessment::postgres::model::{ASSESSMENT_COLUMNS, AssessmentRow, evidence_to_json},
        lead::postgres::{
            model::LeadRow,
            repository::{NOT_SUPPRESSED, push_verdicts},
        },
        postgres::{SharedTx, error::map_sqlx_error},
    },
};

const LEAD_COLUMNS: &str = "id, profile_id, domain, status, score, confidence, verdict, llm_score, qualified_at, first_seen_at, last_seen_at, updated_at";

const LATEST_ASSESSMENT: &str = " AND assessments.id = (SELECT latest.id FROM assessments latest WHERE latest.lead_id = leads.id ORDER BY latest.created_at DESC, latest.id DESC LIMIT 1)";

fn push_queue_filters<'a>(
    builder: &mut QueryBuilder<'a, Postgres>,
    profile_id: Option<ProfileId>,
    verdicts: &[VerdictFilter],
) {
    builder.push(
        " FROM assessments JOIN leads ON leads.id = assessments.lead_id WHERE assessments.draft_status = 'pending' AND leads.status = 'qualified'",
    );
    builder.push(LATEST_ASSESSMENT);
    if let Some(profile_id) = profile_id {
        builder
            .push(" AND leads.profile_id = ")
            .push_bind(profile_id.0);
    }
    push_verdicts(builder, verdicts);
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
        let generation = serde_json::to_value(assessment.provenance.generation)
            .map_err(|error| CoreError::Internal(error.to_string()))?;
        let samples = i32::try_from(assessment.provenance.samples)
            .map_err(|_| CoreError::Internal("assessment samples out of range".to_owned()))?;
        let mut tx = self.tx.lock().await;
        let review = assessment.draft.as_ref().map(|_| {
            assessment
                .draft_review
                .clone()
                .unwrap_or_else(DraftReview::pending)
        });
        let draft = assessment.draft.as_ref();
        let warnings: Vec<&str> = assessment
            .quality_warnings
            .iter()
            .map(|warning| warning.as_str())
            .collect();
        sqlx::query(
            r#"
            INSERT INTO assessments (
                id, lead_id, profile_revision, summary, verdict_narrative, approach_angle, evidence,
                evidence_verified, model, prompt_version, draft_subject, draft_body, draft_status,
                draft_reviewed_by, draft_reviewed_at, draft_edited_at, draft_rejection_reason,
                draft_original_subject, draft_original_body, draft_suggested_subject,
                draft_suggested_body, draft_outdated_at, created_at, quality_warnings,
                provider_id, provider_name, generation, samples
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28)
            "#,
        )
        .bind(assessment.id.0)
        .bind(assessment.lead_id.0)
        .bind(revision)
        .bind(&assessment.summary)
        .bind(&assessment.verdict_narrative)
        .bind(&assessment.approach_angle)
        .bind(&evidence)
        .bind(assessment.evidence_verified)
        .bind(&assessment.provenance.model)
        .bind(&assessment.provenance.prompt_version)
        .bind(draft.map(|draft| draft.subject.as_str()))
        .bind(draft.map(|draft| draft.body.as_str()))
        .bind(review.as_ref().map(|review| review.status.as_str()))
        .bind(
            review
                .as_ref()
                .and_then(|review| review.reviewed_by)
                .map(|user| user.0),
        )
        .bind(review.as_ref().and_then(|review| review.reviewed_at))
        .bind(review.as_ref().and_then(|review| review.edited_at))
        .bind(
            review
                .as_ref()
                .and_then(|review| review.rejection_reason.as_deref()),
        )
        .bind(
            review
                .as_ref()
                .and_then(|review| review.original.as_ref())
                .map(|original| original.subject.as_str()),
        )
        .bind(
            review
                .as_ref()
                .and_then(|review| review.original.as_ref())
                .map(|original| original.body.as_str()),
        )
        .bind(
            review
                .as_ref()
                .and_then(|review| review.suggestion.as_ref())
                .map(|suggestion| suggestion.subject.as_str()),
        )
        .bind(
            review
                .as_ref()
                .and_then(|review| review.suggestion.as_ref())
                .map(|suggestion| suggestion.body.as_str()),
        )
        .bind(review.as_ref().and_then(|review| review.outdated_at))
        .bind(assessment.created_at)
        .bind(&warnings)
        .bind(assessment.provenance.provider_id.map(|provider| provider.0))
        .bind(&assessment.provenance.provider_name)
        .bind(&generation)
        .bind(samples)
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

    #[tracing::instrument(skip(self, lead_ids), fields(db.system = "postgresql", db.operation = "select", db.table = "assessments"), err)]
    async fn latest_summaries(
        &mut self,
        lead_ids: &[LeadId],
    ) -> Result<HashMap<LeadId, String>, CoreError> {
        let ids: Vec<Uuid> = lead_ids.iter().map(|id| id.0).collect();
        let mut tx = self.tx.lock().await;
        let rows: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT DISTINCT ON (lead_id) lead_id, summary FROM assessments WHERE lead_id = ANY($1) ORDER BY lead_id, created_at DESC, id DESC",
        )
        .bind(&ids)
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(rows
            .into_iter()
            .map(|(id, summary)| (LeadId(id), summary))
            .collect())
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
        verdicts: &[VerdictFilter],
        limit: u32,
        offset: u64,
    ) -> Result<DraftPage, CoreError> {
        let mut count = QueryBuilder::<Postgres>::new("SELECT COUNT(*)");
        push_queue_filters(&mut count, profile_id, verdicts);

        let mut select = QueryBuilder::<Postgres>::new(format!("SELECT {ASSESSMENT_COLUMNS}"));
        push_queue_filters(&mut select, profile_id, verdicts);
        select.push(" ORDER BY leads.confidence DESC NULLS LAST, leads.score DESC, assessments.created_at ASC, assessments.id ASC");
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
