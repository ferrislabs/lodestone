use std::{collections::BTreeSet, sync::Arc};

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, ContactFinder, ContactMethod, DnsError, EmailAddress, FoundAddress, MxLookup,
    NormalizedDomain, Provenance, SourceError, SourceName, UnitInterval,
};

const GUESS_CONFIDENCE: f64 = 0.2;

pub struct RoleGuessFinder<M: MxLookup> {
    mx: Arc<M>,
}

impl<M: MxLookup> RoleGuessFinder<M> {
    pub fn new(mx: Arc<M>) -> Self {
        Self { mx }
    }
}

pub fn guess_parts(profiles: &[ActiveProfile]) -> BTreeSet<String> {
    profiles
        .iter()
        .flat_map(|active| {
            active
                .revision
                .definition
                .definition()
                .contact_ranking
                .guess_local_parts
                .iter()
                .map(|part| part.to_ascii_lowercase())
        })
        .collect()
}

impl<M: MxLookup> ContactFinder for RoleGuessFinder<M> {
    fn name(&self) -> SourceName {
        SourceName::RoleGuess
    }

    fn requires(&self) -> Option<SourceName> {
        Some(SourceName::PublicPage)
    }

    async fn find(
        &self,
        domain: &NormalizedDomain,
        profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<FoundAddress>, SourceError> {
        let parts = guess_parts(profiles);
        if parts.is_empty() {
            return Ok(Vec::new());
        }
        let has_mx = match self.mx.has_mx(domain).await {
            Ok(has_mx) => has_mx,
            Err(DnsError::Timeout) => {
                return Err(SourceError::Upstream("mx lookup timed out".to_owned()));
            }
            Err(DnsError::Failure(message)) => return Err(SourceError::Upstream(message)),
        };
        if !has_mx {
            return Ok(Vec::new());
        }
        let provenance = Provenance::parse(&format!("dns:{}?type=MX", domain.as_str()))
            .map_err(|error| SourceError::Upstream(error.to_string()))?;
        let confidence = UnitInterval::new(GUESS_CONFIDENCE)
            .map_err(|error| SourceError::Upstream(error.to_string()))?;
        Ok(parts
            .iter()
            .filter_map(|part| EmailAddress::parse(&format!("{part}@{}", domain.as_str())).ok())
            .map(|address| FoundAddress {
                address,
                provenance: provenance.clone(),
                method: ContactMethod::RoleGuess,
                confidence,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests;
