use common::CoreError;

use crate::domain::{enrichment::LeadEnrichmentRecord, lead::LeadId};

pub trait EnrichmentRepository: Send {
    fn record(
        &mut self,
        lead_id: LeadId,
    ) -> impl Future<Output = Result<Option<LeadEnrichmentRecord>, CoreError>> + Send;
}
