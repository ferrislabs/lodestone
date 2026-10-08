use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use common::CoreError;
use lodestone_macros::repository;
use serde_json::{Value, from_value, to_value};
use sqlx::{query, query_as, query_scalar};
use uuid::Uuid;

use crate::{
    LeadId, NormalizedDomain, ProfileId, ValidatedProfile,
    domain::{
        ecosystem::{
            DomainEcosystem, DueDomain, EcosystemFinding, EcosystemModule, EcosystemReadiness,
            FindingExample, ModuleOutcome, ModuleRun, ModuleState, ModuleStats, ModuleStatus,
            STATS_RETENTION_DAYS, hosting_platform, ports::EcosystemRepository,
        },
        enrichment::{CERTIFICATE_SEEN_KIND, CONTACT_SOURCES, InspectionRecord, ListedEnrichment},
    },
    infrastructure::postgres::{SharedTx, error::map_sqlx_error},
};

#[repository(domain = Ecosystem, backend = Postgres)]
pub struct PgEcosystemRepository<'tx> {
    tx: SharedTx<'tx>,
}

impl<'tx> PgEcosystemRepository<'tx> {
    pub fn new(tx: &SharedTx<'tx>) -> Self {
        Self { tx: tx.clone() }
    }
}

struct DueRow {
    domain: String,
    leads: i64,
    profiles: Vec<Uuid>,
}

struct StateRow {
    domain: String,
    module: String,
    status: String,
    last_run: Option<DateTime<Utc>>,
    error: Option<String>,
}

struct FindingRow {
    module: String,
    finding_key: String,
    kind: String,
    label: String,
    value: String,
    source_url: String,
    observed_at: DateTime<Utc>,
    details: Value,
}

struct TotalsRow {
    module: String,
    leads_inspected: i64,
    findings_count: i64,
}

struct LastRow {
    module: String,
    last_run: Option<DateTime<Utc>>,
}

struct ExampleRow {
    module: String,
    latest_example: Option<Value>,
}

struct LeadRow {
    id: Uuid,
    profile_id: Uuid,
    domain: String,
    created_at: DateTime<Utc>,
}

struct DefinitionRow {
    profile_id: Uuid,
    definition: Value,
}

struct EvidenceRow {
    lead_id: Uuid,
    certificate: bool,
    evidence: bool,
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

fn signed(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn module_names(modules: &[EcosystemModule]) -> Vec<String> {
    modules
        .iter()
        .map(|module| module.as_str().to_owned())
        .collect()
}

fn profile_uuids(profiles: &[ProfileId]) -> Vec<Uuid> {
    profiles.iter().map(|profile| profile.0).collect()
}

fn module_state(row: &StateRow) -> Option<ModuleState> {
    Some(ModuleState {
        module: EcosystemModule::parse(&row.module)?,
        status: ModuleStatus::parse(&row.status)?,
        last_run: row.last_run,
        error: row.error.clone(),
    })
}

fn finding(row: FindingRow) -> Option<EcosystemFinding> {
    Some(EcosystemFinding {
        module: EcosystemModule::parse(&row.module)?,
        key: row.finding_key,
        kind: row.kind,
        label: row.label,
        value: row.value,
        source_url: row.source_url,
        observed_at: row.observed_at,
        details: row.details,
    })
}

fn inspection_enabled(definition: Value) -> bool {
    ValidatedProfile::parse(definition)
        .map(|profile| profile.definition().inspection.enabled)
        .unwrap_or(false)
}

impl<'tx> EcosystemRepository for PgEcosystemRepository<'tx> {
    #[tracing::instrument(skip(self, profiles), fields(db.system = "postgresql", db.operation = "select", db.table = "lead_ecosystem_modules", module = module.as_str()), err)]
    async fn due_domains(
        &mut self,
        module: EcosystemModule,
        profiles: &[ProfileId],
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<DueDomain>, CoreError> {
        let ids = profile_uuids(profiles);
        let mut tx = self.tx.lock().await;
        let rows = query_as!(
            DueRow,
            r#"
            SELECT leads.domain,
                COUNT(*) AS "leads!",
                array_agg(DISTINCT leads.profile_id) AS "profiles!"
            FROM leads
            LEFT JOIN lead_ecosystem_modules state
                ON state.domain = leads.domain AND state.module = $1
            WHERE leads.profile_id = ANY($2)
              AND (state.domain IS NULL OR state.next_run_at <= $3)
              AND NOT EXISTS (
                  SELECT 1 FROM suppression_list sup
                  WHERE sup.kind = 'domain'
                    AND (sup.value = leads.domain OR right(leads.domain, length(sup.value) + 1) = '.' || sup.value)
              )
            GROUP BY leads.domain, state.domain, state.next_run_at
            ORDER BY (state.domain IS NULL) DESC, state.next_run_at ASC NULLS FIRST, MIN(leads.created_at) ASC, leads.domain ASC
            LIMIT $4
            "#,
            module.as_str(),
            &ids,
            now,
            i64::from(limit),
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(DueDomain {
                    domain: NormalizedDomain::from_host(&row.domain).ok()?,
                    leads: count(row.leads),
                    profiles: row.profiles.into_iter().map(ProfileId).collect(),
                })
            })
            .collect())
    }

    #[tracing::instrument(skip(self, modules, profiles), fields(db.system = "postgresql", db.operation = "select", db.table = "lead_ecosystem_modules"), err)]
    async fn count_due(
        &mut self,
        modules: &[EcosystemModule],
        profiles: &[ProfileId],
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let names = module_names(modules);
        let ids = profile_uuids(profiles);
        let mut tx = self.tx.lock().await;
        let due = query_scalar!(
            r#"
            SELECT COUNT(*) AS "due!"
            FROM (SELECT DISTINCT domain FROM leads WHERE profile_id = ANY($2)) AS lead_domains
            CROSS JOIN UNNEST($1::text[]) AS expected(module)
            LEFT JOIN lead_ecosystem_modules state
                ON state.domain = lead_domains.domain AND state.module = expected.module
            WHERE (state.domain IS NULL OR state.next_run_at <= $3)
              AND NOT EXISTS (
                  SELECT 1 FROM suppression_list sup
                  WHERE sup.kind = 'domain'
                    AND (sup.value = lead_domains.domain OR right(lead_domains.domain, length(sup.value) + 1) = '.' || sup.value)
              )
            "#,
            &names,
            &ids,
            now,
        )
        .fetch_one(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(count(due))
    }

    #[tracing::instrument(skip(self, outcome), fields(db.system = "postgresql", db.operation = "upsert", db.table = "lead_ecosystem", module = module.as_str()), err)]
    async fn save_outcome(
        &mut self,
        domain: &NormalizedDomain,
        module: EcosystemModule,
        outcome: &ModuleOutcome,
        ran_at: DateTime<Utc>,
        next_run_at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let error = outcome.note();
        let mut tx = self.tx.lock().await;
        query!(
            r#"
            INSERT INTO lead_ecosystem_modules (domain, module, status, last_run, next_run_at, error)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (domain, module) DO UPDATE SET
                status = EXCLUDED.status,
                last_run = EXCLUDED.last_run,
                next_run_at = EXCLUDED.next_run_at,
                error = EXCLUDED.error
            "#,
            domain.as_str(),
            module.as_str(),
            outcome.status().as_str(),
            ran_at,
            next_run_at,
            error,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        if matches!(outcome, ModuleOutcome::Failed(_)) {
            return Ok(());
        }
        let findings = outcome.findings();
        let keys: Vec<&str> = findings.iter().map(|found| found.key.as_str()).collect();
        query!(
            "DELETE FROM lead_ecosystem WHERE domain = $1 AND module = $2 AND NOT (finding_key = ANY($3::text[]))",
            domain.as_str(),
            module.as_str(),
            &keys as &[&str],
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        for found in findings {
            query!(
                r#"
                INSERT INTO lead_ecosystem (domain, module, finding_key, kind, label, value, source_url, observed_at, details)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                ON CONFLICT (domain, module, finding_key) DO UPDATE SET
                    kind = EXCLUDED.kind,
                    label = EXCLUDED.label,
                    value = EXCLUDED.value,
                    source_url = EXCLUDED.source_url,
                    observed_at = EXCLUDED.observed_at,
                    details = EXCLUDED.details
                "#,
                domain.as_str(),
                module.as_str(),
                found.key,
                found.kind,
                found.label,
                found.value,
                found.source_url,
                found.observed_at,
                found.details,
            )
            .execute(&mut ***tx)
            .await
            .map_err(map_sqlx_error)?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "upsert", db.table = "lead_ecosystem_modules", module = module.as_str()), err)]
    async fn postpone(
        &mut self,
        domain: &NormalizedDomain,
        module: EcosystemModule,
        next_run_at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut tx = self.tx.lock().await;
        query!(
            r#"
            INSERT INTO lead_ecosystem_modules (domain, module, status, last_run, next_run_at, error)
            VALUES ($1, $2, 'pending', NULL, $3, NULL)
            ON CONFLICT (domain, module) DO UPDATE SET next_run_at = EXCLUDED.next_run_at
            "#,
            domain.as_str(),
            module.as_str(),
            next_run_at,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self, run), fields(db.system = "postgresql", db.operation = "insert", db.table = "ecosystem_module_runs", module = run.module.as_str()), err)]
    async fn record_run(&mut self, run: &ModuleRun) -> Result<(), CoreError> {
        let example = run
            .latest_example
            .as_ref()
            .map(to_value)
            .transpose()
            .map_err(|error| CoreError::Internal(error.to_string()))?;
        let retention = run.ran_at - Duration::days(STATS_RETENTION_DAYS);
        let mut tx = self.tx.lock().await;
        query!(
            r#"
            INSERT INTO ecosystem_module_runs (id, module, ran_at, leads_inspected, findings_count, latest_example)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            Uuid::now_v7(),
            run.module.as_str(),
            run.ran_at,
            signed(run.leads_inspected),
            signed(run.findings_count),
            example,
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        query!(
            "DELETE FROM ecosystem_module_runs WHERE ran_at < $1",
            retention
        )
        .execute(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "ecosystem_module_runs"), err)]
    async fn stats(&mut self, since: DateTime<Utc>) -> Result<Vec<ModuleStats>, CoreError> {
        let mut tx = self.tx.lock().await;
        let totals = query_as!(
            TotalsRow,
            r#"
            SELECT module,
                COALESCE(SUM(leads_inspected), 0)::bigint AS "leads_inspected!",
                COALESCE(SUM(findings_count), 0)::bigint AS "findings_count!"
            FROM ecosystem_module_runs
            WHERE ran_at >= $1
            GROUP BY module
            "#,
            since,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let last = query_as!(
            LastRow,
            "SELECT module, MAX(ran_at) AS last_run FROM ecosystem_module_runs GROUP BY module",
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let examples = query_as!(
            ExampleRow,
            r#"
            SELECT DISTINCT ON (module) module, latest_example
            FROM ecosystem_module_runs
            WHERE latest_example IS NOT NULL
            ORDER BY module, ran_at DESC
            "#,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(EcosystemModule::ALL
            .into_iter()
            .map(|module| {
                let mut stats = ModuleStats::empty(module);
                if let Some(row) = totals.iter().find(|row| row.module == module.as_str()) {
                    stats.leads_inspected = count(row.leads_inspected);
                    stats.findings_count = count(row.findings_count);
                }
                stats.last_run = last
                    .iter()
                    .find(|row| row.module == module.as_str())
                    .and_then(|row| row.last_run);
                stats.latest_example = examples
                    .iter()
                    .find(|row| row.module == module.as_str())
                    .and_then(|row| row.latest_example.clone())
                    .and_then(|value| from_value::<FindingExample>(value).ok());
                stats
            })
            .collect())
    }

    #[tracing::instrument(skip(self), fields(db.system = "postgresql", db.operation = "select", db.table = "lead_ecosystem"), err)]
    async fn of_domain(&mut self, domain: &NormalizedDomain) -> Result<DomainEcosystem, CoreError> {
        let mut tx = self.tx.lock().await;
        let states = query_as!(
            StateRow,
            "SELECT domain, module, status, last_run, error FROM lead_ecosystem_modules WHERE domain = $1",
            domain.as_str(),
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let findings = query_as!(
            FindingRow,
            r#"
            SELECT module, finding_key, kind, label, value, source_url, observed_at, details
            FROM lead_ecosystem
            WHERE domain = $1
            ORDER BY module, observed_at DESC, finding_key
            "#,
            domain.as_str(),
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        Ok(DomainEcosystem {
            modules: states.iter().filter_map(module_state).collect(),
            findings: findings.into_iter().filter_map(finding).collect(),
        })
    }

    #[tracing::instrument(skip(self, lead_ids), fields(db.system = "postgresql", db.operation = "select", db.table = "lead_ecosystem_modules", leads = lead_ids.len()), err)]
    async fn listed(
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
            "SELECT id, profile_id, domain, created_at FROM leads WHERE id = ANY($1)",
            &ids,
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let profile_ids: Vec<Uuid> = leads.iter().map(|lead| lead.profile_id).collect();
        let domains: Vec<&str> = leads.iter().map(|lead| lead.domain.as_str()).collect();
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
        let states = query_as!(
            StateRow,
            "SELECT domain, module, status, last_run, error FROM lead_ecosystem_modules WHERE domain = ANY($1::text[])",
            &domains as &[&str],
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
        let enabled: HashMap<Uuid, bool> = definitions
            .into_iter()
            .map(|row| (row.profile_id, inspection_enabled(row.definition)))
            .collect();
        let mut modules: HashMap<&str, Vec<ModuleState>> = HashMap::new();
        for row in &states {
            if let Some(state) = module_state(row) {
                modules.entry(row.domain.as_str()).or_default().push(state);
            }
        }
        let providers: Vec<&str> = EcosystemModule::AVAILABLE
            .iter()
            .filter_map(|module| module.provider())
            .collect();
        let ready = query_scalar!(
            "SELECT provider_key FROM source_provider_configurations WHERE status = 'ready' AND provider_key = ANY($1::text[])",
            &providers as &[&str],
        )
        .fetch_all(&mut ***tx)
        .await
        .map_err(map_sqlx_error)?;
        let readiness = EcosystemReadiness::from_ready_providers(&ready);
        let certificates_only: HashMap<Uuid, bool> = evidence
            .into_iter()
            .map(|row| (row.lead_id, row.certificate && !row.evidence))
            .collect();
        Ok(leads
            .iter()
            .map(|lead| {
                let platform = NormalizedDomain::from_host(&lead.domain)
                    .ok()
                    .and_then(|domain| hosting_platform(&domain));
                let record = InspectionRecord {
                    created_at: lead.created_at,
                    enabled: enabled.get(&lead.profile_id).copied().unwrap_or(false),
                    hosting_platform: platform,
                    modules: modules
                        .get(lead.domain.as_str())
                        .cloned()
                        .unwrap_or_default(),
                    readiness: readiness.clone(),
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
