use std::{num::NonZeroU32, sync::Arc, time::Duration};

use chrono::{TimeZone, Utc};
use fetch::{FetchSettings, PoliteFetcher};
use lodestone_core::{
    ActiveProfile, Profile, ProfileId, ProfileName, ProfileRevision, RevisionNumber, Route,
    ValidatedProfile, route,
};
use sources::ted::{TedSettings, TedSource};
use url::Url;
use uuid::Uuid;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const ENDPOINT: &str = "https://api.ted.europa.eu/v3/notices/search";

fn iam_profile() -> ActiveProfile {
    let epoch = Utc.timestamp_opt(0, 0).unwrap();
    let profile_id = ProfileId(Uuid::from_u128(1));
    ActiveProfile {
        profile: Profile {
            id: profile_id,
            name: ProfileName::parse("iam").unwrap(),
            active: true,
            created_at: epoch,
            updated_at: epoch,
        },
        revision: ProfileRevision {
            profile_id,
            number: RevisionNumber::new(1).unwrap(),
            definition: ValidatedProfile::parse(serde_json::from_str(IAM).unwrap()).unwrap(),
            created_by: None,
            created_at: epoch,
        },
    }
}

#[tokio::test]
#[ignore = "calls the real TED API, set LODESTONE_TED_LIVE=1"]
async fn iam_profile_against_the_real_ted_api() {
    if std::env::var("LODESTONE_TED_LIVE").ok().as_deref() != Some("1") {
        return;
    }
    let contact = std::env::var("LODESTONE_TED_CONTACT")
        .unwrap_or_else(|_| "https://github.com/ferrislabs/lodestone".to_owned());
    let fetcher = Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact,
            requests_per_host_per_minute: NonZeroU32::new(30).unwrap(),
            request_timeout: Duration::from_secs(60),
            max_body_bytes: 16 * 1024 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(3600),
            source_quotas: Vec::new(),
            allow_private_targets: false,
        })
        .unwrap(),
    );
    let source = TedSource::new(
        fetcher,
        TedSettings {
            base_url: Url::parse(ENDPOINT).unwrap(),
            lookback_days: 7,
            max_pages: 10,
            buyer_countries: vec!["FRA".to_owned()],
        },
    );
    let profile = iam_profile();
    let rules = profile
        .revision
        .definition
        .definition()
        .enabled_keyword_rules(lodestone_core::SourceName::Ted)
        .next()
        .cloned()
        .unwrap();
    let (candidates, stats) = source
        .collect_with_stats(std::slice::from_ref(&profile), Utc::now())
        .await
        .unwrap();
    let (mut accepted, mut rejected, mut ambiguous) = (0, 0, 0);
    for candidate in &candidates {
        let content = format!("{}\n\n{}", candidate.title, candidate.text);
        match route(&content, &rules) {
            Route::Accept { .. } => accepted += 1,
            Route::Reject => rejected += 1,
            Route::Ambiguous { .. } => ambiguous += 1,
        }
    }
    println!("pages fetched: {}", stats.pages);
    println!("total matching upstream: {}", stats.total_matching);
    println!("records fetched: {}", stats.records);
    println!("candidates: {}", stats.candidates);
    println!("skipped without domain: {}", stats.skipped_no_domain);
    println!("skipped malformed: {}", stats.skipped_malformed);
    println!("route accept: {accepted}, reject: {rejected}, ambiguous: {ambiguous}");
    for candidate in candidates.iter().take(15) {
        println!(
            "  {} | {} | {}",
            candidate.domain,
            candidate.details["publication_number"]
                .as_str()
                .unwrap_or(""),
            candidate.title.chars().take(80).collect::<String>()
        );
    }
}
