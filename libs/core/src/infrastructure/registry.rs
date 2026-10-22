//! Repository registry: maps a (domain, backend) pair to a concrete repository
//! implementation. Each `Pg*Repository` declares its membership via
//! `#[lodestone_macros::repository(domain = X, backend = Postgres)]`, which expands
//! to a [`RepoFor`] impl. Use-case macros resolve the right type by querying
//! `<Backend as RepoFor<Domain>>::Repo`, so swapping the active backend is a
//! single-line change ([`Backend`] alias below).

use crate::infrastructure::postgres::SharedTx;

/// Domain markers — one zero-sized type per aggregate root.
pub mod domain {
    pub struct AgentWork;
    pub struct Assessment;
    pub struct Calibration;
    pub struct Company;
    pub struct Contact;
    pub struct Decision;
    pub struct Ecosystem;
    pub struct HostVisit;
    pub struct KnownHost;
    pub struct Lead;
    pub struct LlmProvider;
    pub struct OrganisationSetting;
    pub struct Partner;
    pub struct Profile;
    pub struct ProfileTrial;
    pub struct Qualification;
    pub struct Run;
    pub struct ScoringAgent;
    pub struct Signal;
    pub struct SourceProvider;
    pub struct Suppression;
    pub struct Timeline;
    pub struct User;
}

/// Backend markers — one zero-sized type per supported persistence layer.
pub mod backend {
    pub struct Postgres;
}

/// Active backend for the whole process. To migrate (e.g. Postgres → Mongo),
/// change this single alias and provide [`RepoFor`] impls for the new backend.
pub type Backend = backend::Postgres;

/// Resolves a domain to its concrete repository type for a given backend.
pub trait RepoFor<D> {
    type Repo<'tx>;

    fn build<'tx>(tx: &SharedTx<'tx>) -> Self::Repo<'tx>;
}
