use std::{env::var, num::NonZeroU32, sync::Arc, time::Duration};

use chrono::{TimeZone, Utc};
use fetch::{FetchSettings, PoliteFetcher};
use lodestone_core::{
    ActiveProfile, Profile, ProfileId, ProfileName, ProfileRevision, RevisionNumber,
    ValidatedProfile,
};
use serde_json::json;
use sources::welcome_to_the_jungle::{
    Access, AccessSource, Endpoints, WelcomeToTheJungleSettings, WelcomeToTheJungleSource,
    is_offer_url,
};
use url::Url;
use uuid::Uuid;

fn profile(keyword: &str) -> ActiveProfile {
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
        "id": "wttj",
        "name": "wttj",
        "providers": ["welcome_to_the_jungle"],
        "keywords": [keyword],
    }]);
    let profile_id = ProfileId(Uuid::from_u128(1));
    let epoch = Utc.timestamp_opt(0, 0).unwrap();
    ActiveProfile {
        profile: Profile {
            id: profile_id,
            name: ProfileName::parse("wttj").unwrap(),
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
#[ignore = "calls the real Welcome to the Jungle search, set LODESTONE_WTTJ_LIVE=1 and WTTJ_REFERER"]
async fn one_keyword_one_page_against_the_real_welcome_to_the_jungle_search() {
    if var("LODESTONE_WTTJ_LIVE").ok().as_deref() != Some("1") {
        return;
    }
    let Ok(referer) = var("WTTJ_REFERER") else {
        println!("WTTJ_REFERER is not set, the live check is skipped");
        return;
    };
    let contact = var("LODESTONE_WTTJ_CONTACT")
        .unwrap_or_else(|_| "https://github.com/ferrislabs/lodestone".to_owned());
    let keyword = var("LODESTONE_WTTJ_KEYWORD").unwrap_or_else(|_| "keycloak".to_owned());
    let fetcher = Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact,
            requests_per_host_per_minute: NonZeroU32::new(10).unwrap(),
            request_timeout: Duration::from_secs(30),
            max_body_bytes: 4 * 1024 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(3600),
            source_quotas: Vec::new(),
            allow_private_targets: false,
        })
        .unwrap(),
    );
    let source = WelcomeToTheJungleSource::new(
        fetcher,
        WelcomeToTheJungleSettings {
            endpoints: Endpoints::official().unwrap(),
            access: AccessSource::Fixed(Access {
                referer,
                jobs_index: var("WTTJ_JOBS_INDEX").ok(),
                requests_per_minute: 10,
                max_pages: 1,
            }),
            max_company_lookups: 3,
        },
    );
    let (candidates, stats) = source
        .collect_with_stats(&[profile(&keyword)])
        .await
        .unwrap();
    println!("requests: {}", stats.requests);
    println!("discovery requests: {}", stats.discovery_requests);
    println!("records: {}", stats.records);
    println!("duplicates: {}", stats.duplicates);
    println!("candidates: {}", stats.candidates);
    println!("company lookups: {}", stats.company_lookups);
    println!("company cache hits: {}", stats.company_cache_hits);
    println!("skipped without domain: {}", stats.skipped_no_domain);
    println!(
        "skipped over lookup budget: {}",
        stats.skipped_company_budget
    );
    println!("skipped malformed: {}", stats.skipped_malformed);
    assert!(stats.company_lookups <= 3);
    assert!(candidates.iter().all(|candidate| {
        Url::parse(candidate.provenance.as_str()).is_ok_and(|url| is_offer_url(&url))
    }));
}
