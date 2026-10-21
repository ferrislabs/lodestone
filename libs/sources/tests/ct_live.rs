use std::{
    num::NonZeroU32,
    time::{Duration, Instant},
};

use chrono::Utc;
use ct::{TILE_WIDTH, fetch_data_tile, fetch_log_list, fetch_tree_size};
use fetch::{FetchSettings, PoliteFetcher};
use lodestone_core::FetchError;
use url::Url;

const LOG_LIST: &str = "https://www.gstatic.com/ct/log_list/v3/log_list.json";
const WINDOW: Duration = Duration::from_secs(60);

fn settings(rate: u32) -> FetchSettings {
    FetchSettings {
        contact: "ct-live-check@ferrislabs.dev".to_owned(),
        requests_per_host_per_minute: NonZeroU32::new(rate).unwrap_or(NonZeroU32::MIN),
        request_timeout: Duration::from_secs(15),
        max_body_bytes: 16 * 1024 * 1024,
        max_redirects: 5,
        robots_ttl: Duration::from_secs(86_400),
        source_quotas: Vec::new(),
        allow_private_targets: false,
    }
}

#[tokio::test]
async fn reads_one_log_for_a_minute_with_the_ct_limiter() {
    if std::env::var("LODESTONE_CT_LIVE").as_deref() != Ok("1") {
        return;
    }
    let rate = std::env::var("LODESTONE_CT_LIVE_RATE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(120);
    let fetcher = PoliteFetcher::new(settings(rate)).unwrap();
    let list_url = Url::parse(LOG_LIST).unwrap();
    let mut logs = fetch_log_list(&fetcher, &list_url).await.unwrap();
    logs.sort_by(|a, b| a.description.cmp(&b.description));
    let log = logs.first().unwrap();
    let full_tiles = fetch_tree_size(&fetcher, log).await.unwrap() / TILE_WIDTH;
    let mut tile = full_tiles.saturating_sub(2_000);
    let started = Instant::now();
    let mut tiles = 0u64;
    let mut certificates = 0u64;
    let mut limited = 0u64;
    while started.elapsed() < WINDOW && tile < full_tiles {
        match fetch_data_tile(&fetcher, log, tile).await {
            Ok(entries) => {
                certificates += u64::try_from(entries.len()).unwrap_or(0);
                tiles += 1;
                tile += 1;
            }
            Err(ct::CtError::Fetch(FetchError::RateLimited { retry_after })) => {
                limited += 1;
                tokio::time::sleep(retry_after.min(Duration::from_secs(5))).await;
            }
            Err(error) => panic!("tile read failed: {error}"),
        }
    }
    let minutes = started.elapsed().as_secs_f64() / 60.0;
    println!(
        "ct live: log={} rate={rate}/min tiles={tiles} certificates={certificates} rate_limited_waits={limited} tiles_per_min={:.1} at={}",
        log.description,
        tiles as f64 / minutes,
        Utc::now()
    );
}
