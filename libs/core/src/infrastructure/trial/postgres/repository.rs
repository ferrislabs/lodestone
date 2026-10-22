use common::CoreError;
use lodestone_macros::repository;
use sqlx::query_as;

use crate::{
    ProfileId,
    domain::trial::{
        DecidedLead, ProfileTrial, TrialId, TrialLead,
        ports::{ProfileTrialRepository, Transition},
    },
    infrastructure::{
        lead::postgres::repository::NOT_SUPPRESSED,
        postgres::{SharedTx, error::map_sqlx_error},
        trial::postgres::model::{DecidedRow, TrialLeadRow, TrialRow},
    },
};

const TRIAL_COLUMNS: &str = "t.id, t.profile_id, t.base_revision, t.definition, t.status, t.sample_size, t.budget_cents, t.spent_cents, t.error, t.created_by, t.created_at, t.started_at, t.finished_at, (SELECT count(*) FROM profile_trial_leads l WHERE l.trial_id = t.id) AS leads_tried";

const LATEST_DECISIONS: &str = "SELECT DISTINCT ON (labels.lead_id) labels.lead_id, labels.kind, labels.decided_at, labels.id FROM decision_labels labels JOIN leads ON leads.id = labels.lead_id WHERE labels.profile_id = $1 AND labels.kind IN ('lead_validated', 'lead_rejected')";

#[repository(domain = ProfileTrial, backend = Postgres)]
pub struct PgProfileTrialRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgProfileTrialRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

impl<'tx> ProfileTrialRepository for PgProfileTrialRepository<'tx> {
    #[tracing::instrument(skip(self, trial), fields(db.system = "postgresql", db.operation = "insert", db.table = "profile_trials", trial.id = %trial.id), err)]
    async fn create(&mut self, trial: &ProfileTrial) -> Result<(), CoreError> {
        let mut tx = self.tx.lock().await;
        sqlx::query(
            "INSERT INTO profile_trials (id, profile_id, base_revision, definition, status, sample_size, budget_cents, spent_cents, error, created_by, created_at, started_at, finished_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(trial.id.0)
        .bind(trial.profile_id.0)
        .bind(i32::try_from(trial.base_revision.get()).map_err(|_| {
            CoreError::Internal("trial base revision out of range".to_owned())
        })?)
        .bind(trial.definition.to_json())
        .bind(trial.status.as_str())
        .bind(i32::from(trial.sample_size.get()))
        .bind(i32::try_from(trial.budget.cents().get()).map_err(|_| {
            CoreError::Internal("trial budget out of range".to_owned())
        })?)
        .bind(trial.spent_cents)
        .bind(trial.error.as_deref())
        .bind(trial.created_by.map(|user| user.0))
        .bind(trial.created_at)
        .bind(trial.started_at)
        .bind(trial.finished_at)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "profile_trials", trial.id = %id), err)]
    async fn find(&mut self, id: TrialId) -> Result<Option<ProfileTrial>, CoreError> {
        let mut tx = self.tx.lock().await;
        let row = query_as::<_, TrialRow>(&format!(
            "SELECT {TRIAL_COLUMNS} FROM profile_trials t WHERE t.id = $1"
        ))
        .bind(id.0)
        .fetch_optional(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        row.map(ProfileTrial::try_from).transpose()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "profile_trials", profile.id = %profile_id), err)]
    async fn list_for_profile(
        &mut self,
        profile_id: ProfileId,
        limit: u32,
    ) -> Result<Vec<ProfileTrial>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = query_as::<_, TrialRow>(&format!(
            "SELECT {TRIAL_COLUMNS} FROM profile_trials t WHERE t.profile_id = $1 ORDER BY t.created_at DESC, t.id DESC LIMIT $2"
        ))
        .bind(profile_id.0)
        .bind(i64::from(limit))
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter().map(ProfileTrial::try_from).collect()
    }

    #[tracing::instrument(skip(self, change), fields(db.system = "postgresql", db.operation = "update", db.table = "profile_trials", trial.id = %id, trial.status = change.status.as_str()), err)]
    async fn transition(&mut self, id: TrialId, change: &Transition) -> Result<(), CoreError> {
        let mut tx = self.tx.lock().await;
        let result = sqlx::query(
            "UPDATE profile_trials SET
                status = $2,
                error = $3,
                started_at = CASE WHEN $2 = 'running' THEN COALESCE(started_at, $4) ELSE started_at END,
                finished_at = CASE WHEN $2 IN ('done', 'failed', 'budget_exhausted') THEN $4 ELSE NULL END
             WHERE id = $1",
        )
        .bind(id.0)
        .bind(change.status.as_str())
        .bind(change.error.as_deref())
        .bind(change.at)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        if result.rows_affected() == 0 {
            return Err(CoreError::NotFound);
        }
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "update", db.table = "profile_trials", trial.id = %id), err)]
    async fn record_spend(&mut self, id: TrialId, spent_cents: f64) -> Result<(), CoreError> {
        let mut tx = self.tx.lock().await;
        let result = sqlx::query(
            "UPDATE profile_trials SET spent_cents = GREATEST(spent_cents, $2) WHERE id = $1",
        )
        .bind(id.0)
        .bind(spent_cents)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        if result.rows_affected() == 0 {
            return Err(CoreError::NotFound);
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, lead), fields(db.system = "postgresql", db.operation = "upsert", db.table = "profile_trial_leads", trial.id = %trial_id, lead.id = %lead.lead_id), err)]
    async fn record_lead(&mut self, trial_id: TrialId, lead: &TrialLead) -> Result<(), CoreError> {
        let mut tx = self.tx.lock().await;
        sqlx::query(
            "INSERT INTO profile_trial_leads (trial_id, lead_id, label, current_verdict, current_confidence, trial_verdict, trial_confidence, error, tried_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (trial_id, lead_id) DO UPDATE SET
                label = EXCLUDED.label,
                current_verdict = EXCLUDED.current_verdict,
                current_confidence = EXCLUDED.current_confidence,
                trial_verdict = EXCLUDED.trial_verdict,
                trial_confidence = EXCLUDED.trial_confidence,
                error = EXCLUDED.error,
                tried_at = EXCLUDED.tried_at",
        )
        .bind(trial_id.0)
        .bind(lead.lead_id.0)
        .bind(lead.label.as_str())
        .bind(lead.current_verdict.map(|verdict| verdict.as_str()))
        .bind(lead.current_confidence)
        .bind(lead.trial_verdict.map(|verdict| verdict.as_str()))
        .bind(lead.trial_confidence)
        .bind(lead.error.as_deref())
        .bind(lead.tried_at)
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "profile_trial_leads", trial.id = %trial_id), err)]
    async fn list_leads(&mut self, trial_id: TrialId) -> Result<Vec<TrialLead>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = query_as::<_, TrialLeadRow>(
            "SELECT l.lead_id, leads.domain, l.label, l.current_verdict, l.current_confidence, l.trial_verdict, l.trial_confidence, l.error, l.tried_at
             FROM profile_trial_leads l
             JOIN leads ON leads.id = l.lead_id
             WHERE l.trial_id = $1
             ORDER BY l.tried_at ASC, leads.domain ASC",
        )
        .bind(trial_id.0)
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        rows.into_iter().map(TrialLead::try_from).collect()
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "decision_labels", profile.id = %profile_id), err)]
    async fn decided_leads(
        &mut self,
        profile_id: ProfileId,
    ) -> Result<Vec<DecidedLead>, CoreError> {
        let query = format!(
            "WITH latest AS ({LATEST_DECISIONS}{NOT_SUPPRESSED} ORDER BY labels.lead_id, labels.decided_at DESC, labels.id DESC)
             SELECT lead_id, kind FROM latest ORDER BY decided_at DESC, id DESC"
        );
        let mut tx = self.tx.lock().await;
        let rows = query_as::<_, DecidedRow>(&query)
            .bind(profile_id.0)
            .fetch_all(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        rows.into_iter().map(DecidedLead::try_from).collect()
    }
}
