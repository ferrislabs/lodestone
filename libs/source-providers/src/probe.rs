use fetch::{ApiFetcher, ApiRequest};
use lodestone_core::{
    ATS, BODACC, CERTIFICATE_TRANSPARENCY, COMPANY_RESEARCH, CONTACTS, Credential, DECP, DNS_CNAME,
    FetchError, HACKER_NEWS_HIRING, HACKER_NEWS_THREAD_TITLE, HTTP_PROBE, ProbeContext,
    ProbeOutcome, ProviderProbe, SourceName, TED, WELCOME_TO_THE_JUNGLE, provider_descriptor,
};
use serde_json::{Value, from_slice};
use sources::welcome_to_the_jungle::{Endpoints, check};
use url::Url;

use crate::error::fetch_reason;

pub use lodestone_core::{BOAMP, FRANCE_TRAVAIL};

pub fn fetch_source(key: &str) -> SourceName {
    match key {
        BOAMP => SourceName::Boamp,
        FRANCE_TRAVAIL => SourceName::FranceTravail,
        DECP => SourceName::Decp,
        TED => SourceName::Ted,
        ATS => SourceName::Ats,
        HACKER_NEWS_HIRING => SourceName::HackerNewsHiring,
        WELCOME_TO_THE_JUNGLE => SourceName::WelcomeToTheJungle,
        CERTIFICATE_TRANSPARENCY => SourceName::CertificateTransparency,
        DNS_CNAME => SourceName::DnsCname,
        HTTP_PROBE => SourceName::HttpProbe,
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

pub struct HackerNewsProbe<'f, F: ApiFetcher> {
    fetcher: &'f F,
}

fn has_hiring_thread(body: &[u8]) -> bool {
    let prefix = HACKER_NEWS_THREAD_TITLE.to_lowercase();
    from_slice::<Value>(body)
        .ok()
        .and_then(|list| list.get("hits").and_then(Value::as_array).cloned())
        .is_some_and(|hits| {
            hits.iter().any(|hit| {
                hit.get("title")
                    .and_then(Value::as_str)
                    .is_some_and(|title| title.to_lowercase().starts_with(&prefix))
            })
        })
}

impl<F: ApiFetcher> ProviderProbe for HackerNewsProbe<'_, F> {
    async fn probe(&self, context: &ProbeContext<'_>) -> ProbeOutcome {
        let Some(mut url) =
            setting(context, HACKER_NEWS_HIRING, "base_url").and_then(|raw| Url::parse(raw).ok())
        else {
            return ProbeOutcome::failure("L'URL de l'API de Hacker News est invalide.");
        };
        let Ok(mut segments) = url.path_segments_mut() else {
            return ProbeOutcome::failure("L'URL de l'API de Hacker News est invalide.");
        };
        segments.pop_if_empty().push("search_by_date");
        drop(segments);
        url.query_pairs_mut()
            .append_pair("tags", "story,author_whoishiring")
            .append_pair("hitsPerPage", "10");
        let request = ApiRequest {
            url,
            source: SourceName::HackerNewsHiring,
            bearer: None,
            headers: Vec::new(),
            form: None,
        };
        match self.fetcher.call(&request).await {
            Ok(response) if (200..300).contains(&response.status) => {
                if has_hiring_thread(&response.body) {
                    ProbeOutcome::success(
                        "L'API de Hacker News répond et le fil « Who is hiring? » est trouvé.",
                    )
                } else {
                    ProbeOutcome::failure(
                        "L'API de Hacker News répond mais aucun fil « Who is hiring? » n'a été trouvé.",
                    )
                }
            }
            Ok(response) => ProbeOutcome::failure(format!(
                "L'API de Hacker News a répondu HTTP {}.",
                response.status
            )),
            Err(error) => failure_of("L'API de Hacker News", &error),
        }
    }
}

pub struct WelcomeToTheJungleProbe<'f, F: ApiFetcher> {
    fetcher: &'f F,
}

impl<F: ApiFetcher> ProviderProbe for WelcomeToTheJungleProbe<'_, F> {
    async fn probe(&self, context: &ProbeContext<'_>) -> ProbeOutcome {
        let Ok(endpoints) = Endpoints::official() else {
            return ProbeOutcome::failure("L'adresse de Welcome to the Jungle est invalide.");
        };
        match check(self.fetcher, &endpoints, context.settings).await {
            Ok(message) => ProbeOutcome::success(message),
            Err(message) => ProbeOutcome::failure(message),
        }
    }
}

pub struct EndpointProbe<'f, F: ApiFetcher> {
    fetcher: &'f F,
    provider: &'static str,
    field: &'static str,
    service: &'static str,
    source: SourceName,
    query: &'static [(&'static str, &'static str)],
    any_answer: bool,
}

impl<F: ApiFetcher> ProviderProbe for EndpointProbe<'_, F> {
    async fn probe(&self, context: &ProbeContext<'_>) -> ProbeOutcome {
        let Some(mut url) =
            setting(context, self.provider, self.field).and_then(|raw| Url::parse(raw).ok())
        else {
            return ProbeOutcome::failure(format!("L'URL de {} est invalide.", self.service));
        };
        if !self.query.is_empty() {
            url.query_pairs_mut().extend_pairs(self.query);
        }
        let request = ApiRequest {
            url,
            source: self.source,
            bearer: None,
            headers: Vec::new(),
            form: None,
        };
        match self.fetcher.call(&request).await {
            Ok(response)
                if (200..300).contains(&response.status)
                    || (self.any_answer && response.status < 500) =>
            {
                ProbeOutcome::success(format!("{} répond.", self.service))
            }
            Ok(response) => ProbeOutcome::failure(format!(
                "{} a répondu HTTP {}.",
                self.service, response.status
            )),
            Err(error) => failure_of(self.service, &error),
        }
    }
}

pub struct OfflineProbe {
    message: &'static str,
}

impl ProviderProbe for OfflineProbe {
    async fn probe(&self, _context: &ProbeContext<'_>) -> ProbeOutcome {
        ProbeOutcome::success(self.message)
    }
}

const CRAWLER_MESSAGE: &str = "Cette source lit des pages publiques sur les sites des entreprises : aucun point d'accès externe à tester.";
const DNS_MESSAGE: &str =
    "Cette source interroge le DNS public : aucun point d'accès externe à tester.";
const COMPANY_QUERY: &[(&str, &str)] = &[("q", "ferrislabs"), ("per_page", "1")];

pub enum Probe<'f, F: ApiFetcher> {
    Boamp(BoampProbe<'f, F>),
    FranceTravail(FranceTravailProbe<'f, F>),
    HackerNews(HackerNewsProbe<'f, F>),
    WelcomeToTheJungle(WelcomeToTheJungleProbe<'f, F>),
    Endpoint(EndpointProbe<'f, F>),
    Offline(OfflineProbe),
}

impl<'f, F: ApiFetcher> Probe<'f, F> {
    pub fn for_key(key: &str, fetcher: &'f F) -> Option<Self> {
        match key {
            BOAMP => Some(Self::Boamp(BoampProbe { fetcher })),
            FRANCE_TRAVAIL => Some(Self::FranceTravail(FranceTravailProbe { fetcher })),
            HACKER_NEWS_HIRING => Some(Self::HackerNews(HackerNewsProbe { fetcher })),
            WELCOME_TO_THE_JUNGLE => Some(Self::WelcomeToTheJungle(WelcomeToTheJungleProbe {
                fetcher,
            })),
            DECP => Some(Self::endpoint(
                fetcher,
                DECP,
                "dataset_api_url",
                "L'API data.gouv.fr des DECP",
                SourceName::Decp,
                &[],
                false,
            )),
            TED => Some(Self::endpoint(
                fetcher,
                TED,
                "base_url",
                "L'API TED",
                SourceName::Ted,
                &[],
                true,
            )),
            CERTIFICATE_TRANSPARENCY => Some(Self::endpoint(
                fetcher,
                CERTIFICATE_TRANSPARENCY,
                "log_list_url",
                "La liste des journaux Certificate Transparency",
                SourceName::CertificateTransparency,
                &[],
                false,
            )),
            COMPANY_RESEARCH => Some(Self::endpoint(
                fetcher,
                COMPANY_RESEARCH,
                "entreprises_url",
                "L'API Recherche d'entreprises",
                SourceName::PublicPage,
                COMPANY_QUERY,
                false,
            )),
            BODACC => Some(Self::endpoint(
                fetcher,
                BODACC,
                "base_url",
                "Le jeu de données du BODACC",
                SourceName::PublicPage,
                &[],
                false,
            )),
            ATS | HTTP_PROBE | CONTACTS => Some(Self::Offline(OfflineProbe {
                message: CRAWLER_MESSAGE,
            })),
            DNS_CNAME => Some(Self::Offline(OfflineProbe {
                message: DNS_MESSAGE,
            })),
            _ => None,
        }
    }
}

impl<'f, F: ApiFetcher> Probe<'f, F> {
    fn endpoint(
        fetcher: &'f F,
        provider: &'static str,
        field: &'static str,
        service: &'static str,
        source: SourceName,
        query: &'static [(&'static str, &'static str)],
        any_answer: bool,
    ) -> Self {
        Self::Endpoint(EndpointProbe {
            fetcher,
            provider,
            field,
            service,
            source,
            query,
            any_answer,
        })
    }
}

impl<F: ApiFetcher> ProviderProbe for Probe<'_, F> {
    async fn probe(&self, context: &ProbeContext<'_>) -> ProbeOutcome {
        match self {
            Self::Boamp(probe) => probe.probe(context).await,
            Self::FranceTravail(probe) => probe.probe(context).await,
            Self::HackerNews(probe) => probe.probe(context).await,
            Self::WelcomeToTheJungle(probe) => probe.probe(context).await,
            Self::Endpoint(probe) => probe.probe(context).await,
            Self::Offline(probe) => probe.probe(context).await,
        }
    }
}
