use std::{num::NonZeroU32, sync::Arc, time::Duration};

use chrono::{TimeZone, Utc};
use common::Secret;
use fetch::{FetchSettings, PoliteFetcher};
use lodestone_core::{
    ActiveProfile, Profile, ProfileId, ProfileName, ProfileRevision, RevisionNumber, Route,
    ValidatedProfile, route,
};
use sources::france_travail::{FranceTravailSettings, FranceTravailSource};
use url::Url;
use uuid::Uuid;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const TOKEN_URL: &str =
    "https://entreprise.francetravail.fr/connexion/oauth2/access_token?realm=%2Fpartenaire";
const API_BASE_URL: &str = "https://api.francetravail.io/partenaire/offresdemploi/v2";

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

fn credential(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

#[tokio::test]
#[ignore = "calls the real France Travail API, set LODESTONE_FRANCE_TRAVAIL_LIVE=1, FRANCE_TRAVAIL_CLIENT_ID and FRANCE_TRAVAIL_CLIENT_SECRET"]
async fn iam_profile_against_the_real_france_travail_api() {
    if std::env::var("LODESTONE_FRANCE_TRAVAIL_LIVE")
        .ok()
        .as_deref()
        != Some("1")
    {
        println!("skipped: LODESTONE_FRANCE_TRAVAIL_LIVE is not 1");
        return;
    }
    let (Some(client_id), Some(client_secret)) = (
        credential("FRANCE_TRAVAIL_CLIENT_ID"),
        credential("FRANCE_TRAVAIL_CLIENT_SECRET"),
    ) else {
        println!("skipped: FRANCE_TRAVAIL_CLIENT_ID and FRANCE_TRAVAIL_CLIENT_SECRET are required");
        return;
    };
    let contact = std::env::var("LODESTONE_FRANCE_TRAVAIL_CONTACT")
        .unwrap_or_else(|_| "https://github.com/ferrislabs/lodestone".to_owned());
    let fetcher = Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact,
            requests_per_host_per_minute: NonZeroU32::new(180).unwrap(),
            request_timeout: Duration::from_secs(60),
            max_body_bytes: 16 * 1024 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(3600),
            source_quotas: Vec::new(),
            allow_private_targets: false,
        })
        .unwrap(),
    );
    let source = FranceTravailSource::new(
        fetcher,
        FranceTravailSettings {
            token_url: Url::parse(TOKEN_URL).unwrap(),
            api_base_url: Url::parse(API_BASE_URL).unwrap(),
            client_id: Secret::new(client_id),
            client_secret: Secret::new(client_secret),
            published_since_days: 7,
            max_pages: 5,
        },
    );
    let profile = iam_profile();
    let rules = profile
        .revision
        .definition
        .definition()
        .enabled_keyword_rules(lodestone_core::SourceName::FranceTravail)
        .next()
        .cloned()
        .unwrap();
    let (candidates, stats) = source
        .collect_with_stats(std::slice::from_ref(&profile))
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
    println!("queries: {}", stats.queries);
    println!("requests: {}", stats.requests);
    println!("total matching upstream: {}", stats.total_matching);
    println!("records fetched: {}", stats.records);
    println!("duplicates: {}", stats.duplicates);
    println!("candidates: {}", stats.candidates);
    println!("skipped without domain: {}", stats.skipped_no_domain);
    println!("skipped malformed: {}", stats.skipped_malformed);
    println!("route accept: {accepted}, reject: {rejected}, ambiguous: {ambiguous}");
    for candidate in candidates.iter().take(15) {
        println!(
            "  {} | {} | {}",
            candidate.domain,
            candidate.details["offer_id"].as_str().unwrap_or(""),
            candidate.title.chars().take(80).collect::<String>()
        );
    }
}
