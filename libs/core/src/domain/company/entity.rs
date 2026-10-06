use crate::domain::{
    NormalizedDomain,
    company::Siren,
    lead::LeadId,
    profile::{ProfileId, ProfileName},
    qualification::Verdict,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyEntity {
    pub siren: Siren,
    pub legal_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SiblingLead {
    pub id: LeadId,
    pub domain: NormalizedDomain,
    pub profile_id: ProfileId,
    pub profile_name: ProfileName,
    pub verdict: Option<Verdict>,
    pub confidence: Option<f64>,
}

impl SiblingLead {
    pub fn shares_profile(&self, profile_id: ProfileId) -> bool {
        self.profile_id == profile_id
    }
}
