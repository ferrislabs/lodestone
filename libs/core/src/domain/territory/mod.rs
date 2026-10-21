use crate::domain::{
    NormalizedDomain,
    ecosystem::saas_tenant,
    facts::country_of,
    lead::Signal,
    profile::definition::{CountryCode, ProfileDefinition},
};

pub const OUT_OF_COUNTRY_REASON: &str = "hors pays visé";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Territory {
    Listed,
    Generic,
    Foreign,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostAdmission {
    Admitted,
    SaasTenant,
    OutOfCountry,
}

pub fn lists_country(countries: &[CountryCode], code: &str) -> bool {
    countries
        .iter()
        .any(|country| country.as_str().eq_ignore_ascii_case(code))
}

pub fn territory_of(domain: &NormalizedDomain, countries: &[CountryCode]) -> Territory {
    match country_of(domain) {
        Some(country) if lists_country(countries, country.code) => Territory::Listed,
        Some(_) => Territory::Foreign,
        None => Territory::Generic,
    }
}

pub fn admit_host(
    domain: &NormalizedDomain,
    countries: &[CountryCode],
    head_office: Option<&str>,
) -> HostAdmission {
    if saas_tenant(domain.as_str()).is_some() {
        return HostAdmission::SaasTenant;
    }
    if countries.is_empty() {
        return HostAdmission::Admitted;
    }
    let admitted = match territory_of(domain, countries) {
        Territory::Listed => true,
        Territory::Foreign => false,
        Territory::Generic => head_office.is_none_or(|code| lists_country(countries, code)),
    };
    if admitted {
        HostAdmission::Admitted
    } else {
        HostAdmission::OutOfCountry
    }
}

pub fn out_of_country(
    definition: &ProfileDefinition,
    domain: &NormalizedDomain,
    signals: &[Signal],
    head_office: &str,
) -> bool {
    if country_of(domain).is_some() {
        return false;
    }
    let mut sourced = signals
        .iter()
        .filter_map(|signal| signal.agent.as_ref())
        .peekable();
    if sourced.peek().is_none() {
        return false;
    }
    sourced.all(|id| {
        definition
            .agents
            .iter()
            .find(|agent| agent.id() == id)
            .and_then(|agent| agent.host_countries())
            .is_some_and(|countries| {
                !countries.is_empty() && !lists_country(countries, head_office)
            })
    })
}

#[cfg(test)]
mod tests;
