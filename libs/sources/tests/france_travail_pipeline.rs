use std::{
    io::Write,
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{TimeZone, Utc};
use common::Secret;
use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{Method::GET, MockServer};
use lodestone_core::{
    ActiveProfile, Profile, ProfileId, ProfileName, ProfileRevision, RevisionNumber, SourceError,
    ValidatedProfile,
};
use serde_json::json;
use sources::france_travail::{
    AccessProvider, FranceTravailAccess, FranceTravailSettings, FranceTravailSource,
};
use tracing_subscriber::fmt::MakeWriter;
use url::Url;
use uuid::Uuid;

const SEARCH: &str = include_str!("fixtures/france_travail/offres-search.synthetic.json");
const ACCESS_TOKEN: &str = "synthetic-access-token-0001";

struct StaticAccess {
    api_base_url: Url,
}

impl AccessProvider for StaticAccess {
    async fn access(&self) -> Result<FranceTravailAccess, SourceError> {
        Ok(FranceTravailAccess {
            token: Secret::new(ACCESS_TOKEN),
            api_base_url: self.api_base_url.clone(),
        })
    }

    fn invalidate(&self) {}
}

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Sink {
    type Writer = Sink;

    fn make_writer(&'a self) -> Sink {
        self.clone()
    }
}

fn profile() -> ActiveProfile {
    let value = json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": {"france_travail": {"keywords": ["keycloak"]}},
        "scoring": {"factors": [{"signal": "saas_provider", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
    });
    let profile_id = ProfileId(Uuid::from_u128(1));
    let epoch = Utc.timestamp_opt(0, 0).unwrap();
    ActiveProfile {
        profile: Profile {
            id: profile_id,
            name: ProfileName::parse("p").unwrap(),
            active: true,
            created_at: epoch,
            updated_at: epoch,
        },
        revision: ProfileRevision {
            profile_id,
            number: RevisionNumber::new(1).unwrap(),
            definition: ValidatedProfile::parse(value).unwrap(),
            created_by: None,
            created_at: epoch,
        },
    }
}

#[tokio::test]
async fn the_source_searches_over_http_with_its_access_and_never_logs_the_token() {
    let sink = Sink::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(sink.clone())
        .with_max_level(tracing::Level::TRACE)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let server = MockServer::start_async().await;
    let search = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/v2/offres/search")
                .query_param("motsCles", "keycloak")
                .query_param("range", "0-149")
                .query_param("publieeDepuis", "7")
                .header("authorization", "Bearer synthetic-access-token-0001");
            then.status(200)
                .header("content-type", "application/json")
                .body(SEARCH);
        })
        .await;
    let fetcher = Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact: "bot@example.com".to_owned(),
            requests_per_host_per_minute: NonZeroU32::new(1000).unwrap(),
            request_timeout: Duration::from_secs(5),
            max_body_bytes: 1024 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(60),
            source_quotas: Vec::new(),
            allow_private_targets: true,
        })
        .unwrap(),
    );
    let source = FranceTravailSource::new(
        fetcher,
        StaticAccess {
            api_base_url: Url::parse(&server.url("/v2")).unwrap(),
        },
        FranceTravailSettings {
            published_since_days: 7,
            max_pages: 5,
        },
    );
    let (candidates, stats) = source.collect_with_stats(&[profile()]).await.unwrap();
    assert_eq!(candidates.len(), 3);
    assert_eq!(stats.skipped_no_domain, 2);
    search.assert_hits_async(1).await;
    let logged = String::from_utf8(sink.0.lock().unwrap().clone()).unwrap();
    assert!(logged.contains("france travail fetch finished"));
    assert!(
        !logged.contains(ACCESS_TOKEN),
        "the token leaked into the logs"
    );
}
