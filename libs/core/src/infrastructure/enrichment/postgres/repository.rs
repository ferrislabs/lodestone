use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use serde_json::Value;
use sqlx::query_as;

use crate::{
    LeadId, ValidatedProfile,
    definition::Enrichment,
    domain::enrichment::{
        EnrichmentStep, LeadEnrichmentRecord, StepRun, ports::EnrichmentRepository,
    },
    infrastructure::postgres::{SharedTx, error::map_sqlx_error},
};

#[repository(domain = Enrichment, backend = Postgres)]
pub struct PgEnrichmentRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgEnrichmentRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

struct StepRow {
    step: Option<String>,
    ran_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    definition: Option<Value>,
}

fn settings_of(definition: Option<Value>) -> Enrichment {
    definition
        .and_then(|definition| ValidatedProfile::parse(definition).ok())
        .map(|profile| profile.definition().enrichment.clone())
        .unwrap_or_default()
}

impl<'tx> EnrichmentRepository for PgEnrichmentRepository<'tx> {
    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "lead_enrichments", lead.id = %lead_id.0), err)]
    async fn record(&mut self, lead_id: LeadId) -> Result<Option<LeadEnrichmentRecord>, CoreError> {
        let mut tx = self.tx.lock().await;
        let rows = query_as!(
            StepRow,
            r#"
            SELECT e.step AS "step?", e.ran_at AS "ran_at?", leads.created_at,
                (
                    SELECT r.definition FROM profile_revisions r
                    WHERE r.profile_id = leads.profile_id
                    ORDER BY r.revision DESC
                    LIMIT 1
                ) AS "definition?"
            FROM leads
            LEFT JOIN lead_enrichments e ON e.lead_id = leads.id
            WHERE leads.id = $1
            ORDER BY e.step
            "#,
            lead_id.0,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let runs = rows
            .iter()
            .filter_map(|row| {
                Some(StepRun {
                    step: EnrichmentStep::parse(row.step.as_deref()?)?,
                    ran_at: row.ran_at?,
                })
            })
            .collect();
        let Some(first) = rows.into_iter().next() else {
            return Ok(None);
        };
        let created_at = first.created_at;
        let settings = settings_of(first.definition);
        Ok(Some(LeadEnrichmentRecord {
            created_at,
            runs,
            settings,
        }))
    }
}
