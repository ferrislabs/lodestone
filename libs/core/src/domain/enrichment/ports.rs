use std::collections::HashMap;

use common::CoreError;

use crate::domain::{
    enrichment::{LeadEnrichmentRecord, ListedEnrichment},
    lead::LeadId,
};

pub trait EnrichmentRepository: Send {
    fn record(
        &mut self,
        lead_id: LeadId,
    ) -> impl Future<Output = Result<Option<LeadEnrichmentRecord>, CoreError>> + Send;

    fn records(
        &mut self,
        lead_ids: &[LeadId],
    ) -> impl Future<Output = Result<HashMap<LeadId, ListedEnrichment>, CoreError>> + Send;
}
