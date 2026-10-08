use std::collections::HashMap;

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::repository;
use serde_json::Value;
use sqlx::query_as;
use uuid::Uuid;

use crate::{
    LeadId, ValidatedProfile,
    definition::Enrichment,
    domain::enrichment::{
        CERTIFICATE_SEEN_KIND, CONTACT_SOURCES, EnrichmentStep, LeadEnrichmentRecord,
        ListedEnrichment, StepRun, ports::EnrichmentRepository,
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

struct LeadRow {
    id: Uuid,
    profile_id: Uuid,
    created_at: DateTime<Utc>,
}

struct DefinitionRow {
    profile_id: Uuid,
    definition: Value,
}

struct RunRow {
    lead_id: Uuid,
    step: String,
    ran_at: Option<DateTime<Utc>>,
}

struct EvidenceRow {
    lead_id: Uuid,
    certificate: bool,
    evidence: bool,
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

    #[tracing::instrument(skip(self, lead_ids), fields(db.system = "postgresql", db.operation = "select", db.table = "lead_enrichments", leads = lead_ids.len()), err)]
    async fn records(
        &mut self,
        lead_ids: &[LeadId],
    ) -> Result<HashMap<LeadId, ListedEnrichment>, CoreError> {
        if lead_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<Uuid> = lead_ids.iter().map(|id| id.0).collect();
        let contact_sources: Vec<String> = CONTACT_SOURCES
            .iter()
            .map(|source| source.as_str().to_owned())
            .collect();
        let mut tx = self.tx.lock().await;
        let leads = query_as!(
            LeadRow,
            "SELECT id, profile_id, created_at FROM leads WHERE id = ANY($1)",
            &ids,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let profile_ids: Vec<Uuid> = leads.iter().map(|lead| lead.profile_id).collect();
        let definitions = query_as!(
            DefinitionRow,
            r#"
            SELECT DISTINCT ON (profile_id) profile_id, definition
            FROM profile_revisions
            WHERE profile_id = ANY($1)
            ORDER BY profile_id, revision DESC
            "#,
            &profile_ids,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let runs = query_as!(
            RunRow,
            "SELECT lead_id, step, ran_at FROM lead_enrichments WHERE lead_id = ANY($1)",
            &ids,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let evidence = query_as!(
            EvidenceRow,
            r#"
            SELECT lead_id,
                bool_or(kind = $2) AS "certificate!",
                bool_or(kind <> $2 AND NOT (source = ANY($3))) AS "evidence!"
            FROM signals
            WHERE lead_id = ANY($1)
            GROUP BY lead_id
            "#,
            &ids,
            CERTIFICATE_SEEN_KIND,
            &contact_sources,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let settings: HashMap<Uuid, Enrichment> = definitions
            .into_iter()
            .map(|row| (row.profile_id, settings_of(Some(row.definition))))
            .collect();
        let mut steps: HashMap<Uuid, Vec<StepRun>> = HashMap::new();
        for row in runs {
            let (Some(step), Some(ran_at)) = (EnrichmentStep::parse(&row.step), row.ran_at) else {
                continue;
            };
            steps
                .entry(row.lead_id)
                .or_default()
                .push(StepRun { step, ran_at });
        }
        let certificates_only: HashMap<Uuid, bool> = evidence
            .into_iter()
            .map(|row| (row.lead_id, row.certificate && !row.evidence))
            .collect();
        Ok(leads
            .into_iter()
            .map(|lead| {
                let record = LeadEnrichmentRecord {
                    created_at: lead.created_at,
                    runs: steps.remove(&lead.id).unwrap_or_default(),
                    settings: settings.get(&lead.profile_id).cloned().unwrap_or_default(),
                };
                let listed = ListedEnrichment {
                    record,
                    only_certificates: certificates_only.get(&lead.id).copied().unwrap_or(false),
                };
                (LeadId(lead.id), listed)
            })
            .collect())
    }
}
