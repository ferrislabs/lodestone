use std::sync::atomic::{AtomicUsize, Ordering};

use fetch::{ApiFetcher, ApiRequest};
use lodestone_core::{
    Credential, FetchError, FetchResponse, ProbeContext, ProviderProbe, SourceName,
    WELCOME_TO_THE_JUNGLE,
};
use serde_json::{Map, Value, json};
use source_providers::{Probe, fetch_source};

#[derive(Default)]
struct Offline {
    calls: AtomicUsize,
}

impl ApiFetcher for Offline {
    async fn call(&self, _request: &ApiRequest) -> Result<FetchResponse, FetchError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Err(FetchError::Network("offline".to_owned()))
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

#[tokio::test]
async fn the_probe_is_not_ready_without_the_referer_and_sends_nothing() {
    let fetcher = Offline::default();
    let probe = Probe::for_key(WELCOME_TO_THE_JUNGLE, &fetcher).unwrap();
    let settings = object(json!({
        "authorised": true,
        "authorisation_reference": "Courriel du 9 octobre 2026",
    }));

    let outcome = probe
        .probe(&ProbeContext {
            settings: &settings,
            credential: &Credential::None,
        })
        .await;

    assert!(!outcome.ok);
    assert!(outcome.message.contains("Referer"), "{}", outcome.message);
    assert_eq!(fetcher.calls.load(Ordering::Relaxed), 0);
    assert_eq!(
        fetch_source(WELCOME_TO_THE_JUNGLE),
        SourceName::WelcomeToTheJungle
    );
}

#[tokio::test]
async fn the_probe_reports_an_unreachable_site() {
    let fetcher = Offline::default();
    let probe = Probe::for_key(WELCOME_TO_THE_JUNGLE, &fetcher).unwrap();
    let settings = object(json!({
        "authorised": true,
        "authorisation_reference": "Courriel du 9 octobre 2026",
        "referer": "https://www.welcometothejungle.com/",
    }));

    let outcome = probe
        .probe(&ProbeContext {
            settings: &settings,
            credential: &Credential::None,
        })
        .await;

    assert!(!outcome.ok);
    assert!(
        outcome.message.contains("injoignable"),
        "{}",
        outcome.message
    );
    assert_eq!(fetcher.calls.load(Ordering::Relaxed), 1);
}
