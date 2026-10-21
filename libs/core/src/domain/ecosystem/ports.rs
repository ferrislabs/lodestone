use std::collections::HashMap;

use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    NormalizedDomain,
    ecosystem::{
        DomainEcosystem, DueDomain, EcosystemModule, ModuleOutcome, ModuleRun, ModuleStats,
    },
    enrichment::ListedEnrichment,
    lead::LeadId,
    profile::ProfileId,
};

pub trait EcosystemRepository: Send {
    fn due_domains(
        &mut self,
        module: EcosystemModule,
        profiles: &[ProfileId],
        now: DateTime<Utc>,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<DueDomain>, CoreError>> + Send;

    fn count_due(
        &mut self,
        modules: &[EcosystemModule],
        profiles: &[ProfileId],
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn save_outcome(
        &mut self,
        domain: &NormalizedDomain,
        module: EcosystemModule,
        outcome: &ModuleOutcome,
        ran_at: DateTime<Utc>,
        next_run_at: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn postpone(
        &mut self,
        domain: &NormalizedDomain,
        module: EcosystemModule,
        next_run_at: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn record_run(&mut self, run: &ModuleRun)
    -> impl Future<Output = Result<(), CoreError>> + Send;

    fn stats(
        &mut self,
        since: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<ModuleStats>, CoreError>> + Send;

    fn of_domain(
        &mut self,
        domain: &NormalizedDomain,
    ) -> impl Future<Output = Result<DomainEcosystem, CoreError>> + Send;

    fn listed(
        &mut self,
        lead_ids: &[LeadId],
    ) -> impl Future<Output = Result<HashMap<LeadId, ListedEnrichment>, CoreError>> + Send;
}
