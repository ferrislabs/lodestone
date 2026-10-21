use fetch::{ApiFetcher, ApiRequest};
use lodestone_core::{
    Credential, FetchError, ProbeContext, ProbeOutcome, ProviderProbe, SourceName,
    provider_descriptor,
};
use serde_json::Value;
use url::Url;

use crate::error::fetch_reason;

pub const BOAMP: &str = "boamp";
pub const FRANCE_TRAVAIL: &str = "france_travail";

pub fn fetch_source(key: &str) -> SourceName {
    match key {
        BOAMP => SourceName::Boamp,
        FRANCE_TRAVAIL => SourceName::FranceTravail,
        _ => SourceName::PublicPage,
    }
}

fn setting<'a>(context: &'a ProbeContext<'_>, provider: &str, key: &str) -> Option<&'a str> {
    let default = provider_descriptor(provider)?.field(key)?.default?;
    Some(
        context
            .settings
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(default),
    )
}

fn failure_of(service: &str, error: &FetchError) -> ProbeOutcome {
    ProbeOutcome::failure(format!(
        "{service} est injoignable ({}).",
        fetch_reason(error)
    ))
}

pub struct BoampProbe<'f, F: ApiFetcher> {
    fetcher: &'f F,
}

impl<F: ApiFetcher> ProviderProbe for BoampProbe<'_, F> {
    async fn probe(&self, context: &ProbeContext<'_>) -> ProbeOutcome {
        let Some(mut url) =
            setting(context, BOAMP, "base_url").and_then(|raw| Url::parse(raw).ok())
        else {
            return ProbeOutcome::failure("L'URL de l'API du BOAMP est invalide.");
        };
        url.query_pairs_mut().append_pair("limit", "1");
        let request = ApiRequest {
            url,
            source: SourceName::Boamp,
            bearer: None,
            headers: Vec::new(),
            form: None,
        };
        match self.fetcher.call(&request).await {
            Ok(response) if (200..300).contains(&response.status) => {
                ProbeOutcome::success("L'API du BOAMP répond.")
            }
            Ok(response) => ProbeOutcome::failure(format!(
                "L'API du BOAMP a répondu HTTP {}.",
                response.status
            )),
            Err(error) => failure_of("L'API du BOAMP", &error),
        }
    }
}

pub struct FranceTravailProbe<'f, F: ApiFetcher> {
    fetcher: &'f F,
}

impl<F: ApiFetcher> ProviderProbe for FranceTravailProbe<'_, F> {
    async fn probe(&self, context: &ProbeContext<'_>) -> ProbeOutcome {
        let Credential::Bearer(token) = context.credential else {
            return ProbeOutcome::failure("Aucun jeton d'accès n'est disponible.");
        };
        let Some(base) = setting(context, FRANCE_TRAVAIL, "api_base_url") else {
            return ProbeOutcome::failure("L'URL de l'API France Travail est invalide.");
        };
        let Ok(mut url) = Url::parse(&format!("{}/offres/search", base.trim_end_matches('/')))
        else {
            return ProbeOutcome::failure("L'URL de l'API France Travail est invalide.");
        };
        url.query_pairs_mut().append_pair("range", "0-0");
        let request = ApiRequest {
            url,
            source: SourceName::FranceTravail,
            bearer: Some(token.clone()),
            headers: Vec::new(),
            form: None,
        };
        match self.fetcher.call(&request).await {
            Ok(response) if matches!(response.status, 200 | 204 | 206) => {
                ProbeOutcome::success("Jeton obtenu et API Offres d'emploi accessible.")
            }
            Ok(response) if matches!(response.status, 401 | 403) => ProbeOutcome::failure(format!(
                "L'API France Travail a refusé le jeton (HTTP {}) : vérifiez les droits de l'application.",
                response.status
            )),
            Ok(response) => ProbeOutcome::failure(format!(
                "L'API France Travail a répondu HTTP {}.",
                response.status
            )),
            Err(error) => failure_of("L'API France Travail", &error),
        }
    }
}

pub enum Probe<'f, F: ApiFetcher> {
    Boamp(BoampProbe<'f, F>),
    FranceTravail(FranceTravailProbe<'f, F>),
}

impl<'f, F: ApiFetcher> Probe<'f, F> {
    pub fn for_key(key: &str, fetcher: &'f F) -> Option<Self> {
        match key {
            BOAMP => Some(Self::Boamp(BoampProbe { fetcher })),
            FRANCE_TRAVAIL => Some(Self::FranceTravail(FranceTravailProbe { fetcher })),
            _ => None,
        }
    }
}

impl<F: ApiFetcher> ProviderProbe for Probe<'_, F> {
    async fn probe(&self, context: &ProbeContext<'_>) -> ProbeOutcome {
        match self {
            Self::Boamp(probe) => probe.probe(context).await,
            Self::FranceTravail(probe) => probe.probe(context).await,
        }
    }
}
