use std::{num::NonZeroU32, sync::Arc, time::Duration};

use chrono::Utc;
use fetch::{FetchSettings, PoliteFetcher};
use lodestone_core::{NormalizedDomain, Route, ValidatedProfile, route};
use sources::ats::AtsScanner;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const DOMAINS: [&str; 4] = ["linear.app", "qonto.com", "mistral.ai", "contentsquare.com"];

#[tokio::test]
#[ignore = "calls real careers pages and ATS APIs, set LODESTONE_ATS_LIVE=1"]
async fn iam_keywords_against_real_careers_pages_and_boards() {
    if std::env::var("LODESTONE_ATS_LIVE").ok().as_deref() != Some("1") {
        return;
    }
    let definition = ValidatedProfile::parse(serde_json::from_str(IAM).unwrap()).unwrap();
    let rules = definition
        .definition()
        .enabled_keyword_rules(lodestone_core::SourceName::Ats)
        .next()
        .cloned()
        .unwrap();
    let contact = std::env::var("LODESTONE_ATS_CONTACT")
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
    let scanner = AtsScanner::new(fetcher, 6, Duration::from_secs(75));
    for host in DOMAINS {
        let domain = NormalizedDomain::from_host(host).unwrap();
        let scan = scanner.scan(&domain, Utc::now()).await.unwrap();
        println!(
            "{host}: {} pages fetched, {} boards",
            scan.pages.len(),
            scan.boards.len()
        );
        for board in &scan.boards {
            let (mut accept, mut reject, mut ambiguous) = (0, 0, 0);
            for candidate in &board.candidates {
                let content = format!("{}\n\n{}", candidate.title, candidate.text);
                match route(&content, &rules) {
                    Route::Accept { matched } => {
                        accept += 1;
                        println!("    accept: {} | {matched:?}", candidate.title);
                    }
                    Route::Reject => reject += 1,
                    Route::Ambiguous { matched, .. } => {
                        ambiguous += 1;
                        println!("    ambiguous: {} | {matched:?}", candidate.title);
                    }
                }
            }
            println!(
                "  board {} {}: {} jobs, accept {accept}, reject {reject}, ambiguous {ambiguous}",
                board.board.vendor.label(),
                board.board.slug,
                board.candidates.len()
            );
        }
    }
}
