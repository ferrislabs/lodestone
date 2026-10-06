use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{NormalizedDomain, company::CompanyDossier};

#[cfg_attr(test, mockall::automock)]
pub trait CompanyRepository: Send {
    fn due_domains(
        &mut self,
        now: DateTime<Utc>,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<NormalizedDomain>, CoreError>> + Send;

    fn count_due(
        &mut self,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn find(
        &mut self,
        domain: &NormalizedDomain,
    ) -> impl Future<Output = Result<Option<CompanyDossier>, CoreError>> + Send;

    fn save(
        &mut self,
        dossier: &CompanyDossier,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn request_refresh(
        &mut self,
        domain: &NormalizedDomain,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;
}
