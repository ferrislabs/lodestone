use chrono::{DateTime, Utc};
use common::CoreError;

use crate::domain::{
    NormalizedDomain,
    company::{CompanyDossier, CompanyEntity, SiblingLead},
    lead::LeadId,
};

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

    fn unchecked_domains(
        &mut self,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<NormalizedDomain>, CoreError>> + Send;

    fn count_unchecked(&mut self) -> impl Future<Output = Result<u64, CoreError>> + Send;

    fn save_review(
        &mut self,
        dossier: &CompanyDossier,
        built_at: Option<DateTime<Utc>>,
    ) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn request_refresh(
        &mut self,
        domain: &NormalizedDomain,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn entity_of(
        &mut self,
        domain: &NormalizedDomain,
    ) -> impl Future<Output = Result<Option<CompanyEntity>, CoreError>> + Send;

    fn siblings(
        &mut self,
        lead_id: LeadId,
    ) -> impl Future<Output = Result<Vec<SiblingLead>, CoreError>> + Send;
}
