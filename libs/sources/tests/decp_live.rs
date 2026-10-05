use std::{num::NonZeroU32, sync::Arc, time::Duration};

use chrono::{TimeZone, Utc};
use fetch::{FetchSettings, PoliteFetcher};
use lodestone_core::{
    ActiveProfile, Profile, ProfileId, ProfileName, ProfileRevision, RevisionNumber,
    ValidatedProfile,
};
use sources::decp::{
    ANNUAIRE_URL, DATASET_API_URL, DEFAULT_LOOKBACK_DAYS, DEFAULT_NOTIFIED_WITHIN_DAYS,
    DecpSettings, DecpSource, ENTREPRISES_URL, partner_awards,
};
use url::Url;
use uuid::Uuid;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const MAX_BODY_BYTES: usize = 256 * 1024 * 1024;

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

fn days(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(default)
}

#[tokio::test]
#[ignore = "downloads the real DECP files, set LODESTONE_DECP_LIVE=1"]
async fn iam_profile_against_the_real_decp_dataset() {
    if std::env::var("LODESTONE_DECP_LIVE").ok().as_deref() != Some("1") {
        return;
    }
    let contact = std::env::var("LODESTONE_DECP_CONTACT")
        .unwrap_or_else(|_| "https://github.com/ferrislabs/lodestone".to_owned());
    let fetcher = Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact,
            requests_per_host_per_minute: NonZeroU32::new(120).unwrap(),
            request_timeout: Duration::from_secs(900),
            max_body_bytes: MAX_BODY_BYTES,
            max_redirects: 5,
            robots_ttl: Duration::from_secs(3600),
            source_quotas: Vec::new(),
            allow_private_targets: false,
        })
        .unwrap(),
    );
    let source = DecpSource::new(
        fetcher,
        DecpSettings {
            dataset_api_url: Url::parse(DATASET_API_URL).unwrap(),
            annuaire_url: Url::parse(ANNUAIRE_URL).unwrap(),
            entreprises_url: Url::parse(ENTREPRISES_URL).unwrap(),
            lookback_days: DEFAULT_LOOKBACK_DAYS,
            notified_within_days: days(
                "LODESTONE_DECP_NOTIFIED_WITHIN_DAYS",
                DEFAULT_NOTIFIED_WITHIN_DAYS,
            ),
            max_body_bytes: MAX_BODY_BYTES,
            max_rate_limit_wait: Duration::from_secs(75),
        },
    );
    let profile = iam_profile();
    let rules = profile
        .revision
        .definition
        .definition()
        .decp()
        .cloned()
        .unwrap();
    let (candidates, stats) = source
        .collect_with_stats(std::slice::from_ref(&profile), Utc::now())
        .await
        .unwrap();
    println!(
        "files used: {} ({:.1} MB downloaded)",
        stats.files_used.join(", "),
        stats.bytes as f64 / (1024.0 * 1024.0)
    );
    println!("contracts in files: {}", stats.contracts);
    println!("contracts in window: {}", stats.in_window);
    println!("keyword matches: {}", stats.matched);
    println!("duplicates: {}", stats.duplicates);
    println!("skipped malformed: {}", stats.skipped_malformed);
    println!("candidates: {}", stats.candidates);
    println!("unresolved buyers: {}", stats.unresolved_buyers);
    println!("lookups: {}", stats.lookups);
    for candidate in &candidates {
        println!(
            "  candidate {} | {} | {}",
            candidate.domain,
            candidate.details["buyer_name"].as_str().unwrap_or(""),
            candidate.title.chars().take(80).collect::<String>()
        );
    }
    let contracts = source.take_awards();
    let awards = partner_awards(&rules, &contracts);
    println!("partner awards: {}", awards.len());
    for award in &awards {
        println!(
            "  partner {} | {} | {} | {}",
            award.holder,
            award.holder_name.as_deref().unwrap_or("?"),
            award.notified_on,
            award.object.chars().take(60).collect::<String>()
        );
    }
}
