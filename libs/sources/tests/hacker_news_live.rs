use std::{env::var, num::NonZeroU32, sync::Arc, time::Duration};

use chrono::{TimeZone, Utc};
use fetch::{FetchSettings, PoliteFetcher};
use lodestone_core::{
    ActiveProfile, Profile, ProfileId, ProfileName, ProfileRevision, RevisionNumber,
    ValidatedProfile,
};
use serde_json::json;
use sources::hacker_news::{HackerNewsSettings, HackerNewsSource};
use url::Url;
use uuid::Uuid;

const ENDPOINT: &str = "https://hn.algolia.com/api/v1/";

fn profile() -> ActiveProfile {
    let legacy = json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": {"boamp": {"keywords": ["keycloak"]}},
        "scoring": {"factors": [{"signal": "saas_provider", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
    });
    let mut value = ValidatedProfile::parse(legacy).unwrap().to_json();
    value["agents"] = json!([{
        "type": "search",
        "id": "hn",
        "name": "hn",
        "providers": ["hacker_news_hiring"],
        "keywords": ["rust"],
    }]);
    let profile_id = ProfileId(Uuid::from_u128(1));
    let epoch = Utc.timestamp_opt(0, 0).unwrap();
    ActiveProfile {
        profile: Profile {
            id: profile_id,
            name: ProfileName::parse("hn").unwrap(),
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
#[ignore = "calls the real Hacker News search API, set LODESTONE_HN_LIVE=1"]
async fn one_keyword_against_the_real_hacker_news_api() {
    if var("LODESTONE_HN_LIVE").ok().as_deref() != Some("1") {
        return;
    }
    let contact = var("LODESTONE_HN_CONTACT")
        .unwrap_or_else(|_| "https://github.com/ferrislabs/lodestone".to_owned());
    let fetcher = Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact,
            requests_per_host_per_minute: NonZeroU32::new(30).unwrap(),
            request_timeout: Duration::from_secs(60),
            max_body_bytes: 8 * 1024 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(3600),
            source_quotas: Vec::new(),
            allow_private_targets: false,
        })
        .unwrap(),
    );
    let source = HackerNewsSource::new(
        fetcher,
        HackerNewsSettings {
            base_url: Url::parse(ENDPOINT).unwrap().into(),
            max_pages: 1,
        },
    );
    let (candidates, stats) = source.collect_with_stats(&[profile()]).await.unwrap();
    println!("threads: {}", stats.threads);
    println!("requests: {}", stats.requests);
    println!("records: {}", stats.records);
    println!("replies: {}", stats.replies);
    println!("candidates: {}", stats.candidates);
    println!("skipped without domain: {}", stats.skipped_no_domain);
    println!("skipped malformed: {}", stats.skipped_malformed);
    let remote = candidates
        .iter()
        .filter(|candidate| candidate.details["work_mode"] == "full_remote")
        .count();
    println!("full remote: {remote}");
}
