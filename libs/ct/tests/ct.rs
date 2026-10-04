use std::collections::HashMap;

use chrono::{TimeZone, Utc};
use ct::{
    CtError, CtLog, TILE_WIDTH, data_tile_url, fetch_data_tile, fetch_log_list, fetch_tree_size,
    parse_checkpoint, parse_data_tile, parse_log_list,
};
use lodestone_core::{FetchError, FetchPurpose, FetchRequest, FetchResponse, Fetcher, SourceName};
use proptest::prelude::*;
use url::Url;

const LIST: &[u8] = include_bytes!("fixtures/log_list.json");
const CHECKPOINT: &str = include_str!("fixtures/checkpoint.txt");
const TILE: &[u8] = include_bytes!("fixtures/le-sycamore2026h2-tile-3874719.bin");
const TILE_INDEX: u64 = 3874719;

fn log() -> CtLog {
    CtLog {
        log_id: "id".to_owned(),
        description: "d".to_owned(),
        operator: "o".to_owned(),
        monitoring_url: Url::parse("https://mon.example.org/2026h2/").unwrap(),
    }
}

fn path(log: &CtLog, index: u64) -> String {
    data_tile_url(log, index).path().to_owned()
}

#[test]
fn tile_path_encoding() {
    let log = log();
    assert_eq!(path(&log, 0), "/2026h2/tile/data/000");
    assert_eq!(path(&log, 5), "/2026h2/tile/data/005");
    assert_eq!(path(&log, 999), "/2026h2/tile/data/999");
    assert_eq!(path(&log, 1000), "/2026h2/tile/data/x001/000");
    assert_eq!(path(&log, 1_234_067), "/2026h2/tile/data/x001/x234/067");
    assert_eq!(
        path(&log, 1_000_000_000),
        "/2026h2/tile/data/x001/x000/x000/000"
    );
}

#[test]
fn tile_path_without_trailing_slash() {
    let mut log = log();
    log.monitoring_url = Url::parse("https://mon.example.org/2026h2").unwrap();
    assert_eq!(path(&log, 5), "/2026h2/tile/data/005");
}

#[test]
fn checkpoint_good() {
    assert_eq!(parse_checkpoint(CHECKPOINT).unwrap(), 991_928_821);
}

#[test]
fn checkpoint_missing_size() {
    assert!(matches!(
        parse_checkpoint("origin-only"),
        Err(CtError::Malformed(_))
    ));
    assert!(matches!(parse_checkpoint(""), Err(CtError::Malformed(_))));
}

#[test]
fn checkpoint_non_numeric() {
    assert!(matches!(
        parse_checkpoint("origin\nabc\nhash\n"),
        Err(CtError::Malformed(_))
    ));
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap()
}

#[test]
fn log_list_filters_state_and_time() {
    let logs = parse_log_list(LIST, now()).unwrap();
    let descriptions: Vec<&str> = logs.iter().map(|l| l.description.as_str()).collect();
    assert_eq!(
        descriptions,
        vec![
            "Google 'ParcelYard2026h2' log",
            "Let's Encrypt 'Sycamore2026h2'",
            "Let's Encrypt 'NoInterval'",
        ]
    );
    assert_eq!(logs[0].operator, "Google");
    assert_eq!(logs[1].operator, "Let's Encrypt");
    assert_eq!(
        logs[1].monitoring_url.as_str(),
        "https://mon.sycamore.ct.letsencrypt.org/2026h2/"
    );
}

#[test]
fn log_list_time_window_moves() {
    let later = Utc.with_ymd_and_hms(2027, 2, 1, 0, 0, 0).unwrap();
    let logs = parse_log_list(LIST, later).unwrap();
    let descriptions: Vec<&str> = logs.iter().map(|l| l.description.as_str()).collect();
    assert!(descriptions.contains(&"Google 'ParcelYard2027h1' log"));
    assert!(!descriptions.contains(&"Google 'ParcelYard2026h2' log"));
}

#[test]
fn log_list_rejects_garbage() {
    assert!(matches!(
        parse_log_list(b"not json", now()),
        Err(CtError::Malformed(_))
    ));
}

#[test]
fn real_tile_parses() {
    let first = TILE_INDEX * TILE_WIDTH;
    let entries = parse_data_tile(TILE, first).unwrap();
    assert_eq!(entries.len() as u64, TILE_WIDTH);
    for (offset, entry) in entries.iter().enumerate() {
        assert_eq!(entry.index, first + offset as u64);
        assert!(entry.timestamp_ms > 1_700_000_000_000);
        for name in &entry.dns_names {
            assert_eq!(name, &name.to_ascii_lowercase());
        }
        let mut sorted = entry.dns_names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), entry.dns_names.len());
    }
    let named = entries.iter().filter(|e| !e.dns_names.is_empty()).count();
    assert!(named * 10 >= entries.len() * 9, "named {named}");
}

fn x509_leaf(cert: &[u8]) -> Vec<u8> {
    let mut leaf = Vec::new();
    leaf.extend_from_slice(&1u64.to_be_bytes());
    leaf.extend_from_slice(&0u16.to_be_bytes());
    leaf.extend_from_slice(&(cert.len() as u32).to_be_bytes()[1..]);
    leaf.extend_from_slice(cert);
    leaf.extend_from_slice(&0u16.to_be_bytes());
    leaf.extend_from_slice(&0u16.to_be_bytes());
    leaf
}

#[test]
fn undecodable_certificate_yields_empty_names() {
    let entries = parse_data_tile(&x509_leaf(b"garbage"), 7).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].index, 7);
    assert!(entries[0].dns_names.is_empty());
}

#[test]
fn truncated_leaf_is_malformed() {
    let leaf = x509_leaf(b"garbage");
    for cut in 1..leaf.len() {
        assert!(matches!(
            parse_data_tile(&leaf[..cut], 0),
            Err(CtError::Malformed(_))
        ));
    }
}

#[test]
fn bad_entry_type_is_malformed() {
    let mut leaf = x509_leaf(b"garbage");
    leaf[8] = 0;
    leaf[9] = 9;
    assert!(matches!(
        parse_data_tile(&leaf, 0),
        Err(CtError::Malformed(_))
    ));
}

#[test]
fn oversized_length_is_malformed() {
    let mut leaf = Vec::new();
    leaf.extend_from_slice(&1u64.to_be_bytes());
    leaf.extend_from_slice(&0u16.to_be_bytes());
    leaf.extend_from_slice(&[0xff, 0xff, 0xff]);
    leaf.extend_from_slice(b"short");
    assert!(matches!(
        parse_data_tile(&leaf, 0),
        Err(CtError::Malformed(_))
    ));
}

#[test]
fn bad_fingerprint_length_is_malformed() {
    let mut leaf = x509_leaf(b"garbage");
    let len = leaf.len();
    leaf[len - 2..].copy_from_slice(&5u16.to_be_bytes());
    leaf.extend_from_slice(&[0u8; 5]);
    assert!(matches!(
        parse_data_tile(&leaf, 0),
        Err(CtError::Malformed(_))
    ));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_truncation_never_panics(cut in 0usize..TILE.len()) {
        let _ = parse_data_tile(&TILE[..cut], 0);
    }

    #[test]
    fn random_corruption_never_panics(position in 0usize..TILE.len(), value in any::<u8>()) {
        let mut bytes = TILE[..20_000].to_vec();
        bytes[position % 20_000] = value;
        let _ = parse_data_tile(&bytes, 0);
    }
}

struct FakeFetcher {
    responses: HashMap<String, (u16, Vec<u8>, bool)>,
}

impl FakeFetcher {
    fn new(entries: Vec<(&str, u16, Vec<u8>, bool)>) -> Self {
        Self {
            responses: entries
                .into_iter()
                .map(|(url, status, body, truncated)| (url.to_owned(), (status, body, truncated)))
                .collect(),
        }
    }
}

impl Fetcher for FakeFetcher {
    fn fetch(
        &self,
        request: &FetchRequest,
    ) -> impl Future<Output = Result<FetchResponse, FetchError>> + Send {
        assert_eq!(request.purpose, FetchPurpose::Api);
        assert_eq!(request.source, SourceName::CertificateTransparency);
        let result = match self.responses.get(request.url.as_str()) {
            Some((status, body, truncated)) => Ok(FetchResponse {
                status: *status,
                headers: Vec::new(),
                body: body.clone(),
                final_url: request.url.clone(),
                truncated: *truncated,
            }),
            None => Err(FetchError::Network("unknown url".to_owned())),
        };
        async move { result }
    }
}

const TILE_URL: &str = "https://mon.example.org/2026h2/tile/data/x003/x874/719";
const CHECKPOINT_URL: &str = "https://mon.example.org/2026h2/checkpoint";

#[tokio::test]
async fn fetch_tree_size_reads_checkpoint() {
    let fetcher = FakeFetcher::new(vec![(
        CHECKPOINT_URL,
        200,
        CHECKPOINT.as_bytes().to_vec(),
        false,
    )]);
    assert_eq!(
        fetch_tree_size(&fetcher, &log()).await.unwrap(),
        991_928_821
    );
}

#[tokio::test]
async fn fetch_data_tile_reads_full_tile() {
    let fetcher = FakeFetcher::new(vec![(TILE_URL, 200, TILE.to_vec(), false)]);
    let entries = fetch_data_tile(&fetcher, &log(), TILE_INDEX).await.unwrap();
    assert_eq!(entries.len() as u64, TILE_WIDTH);
    assert_eq!(entries[0].index, TILE_INDEX * TILE_WIDTH);
}

#[tokio::test]
async fn fetch_data_tile_errors() {
    let truncated = FakeFetcher::new(vec![(TILE_URL, 200, TILE.to_vec(), true)]);
    assert_eq!(
        fetch_data_tile(&truncated, &log(), TILE_INDEX).await,
        Err(CtError::Truncated)
    );
    let missing = FakeFetcher::new(vec![(TILE_URL, 404, Vec::new(), false)]);
    assert_eq!(
        fetch_data_tile(&missing, &log(), TILE_INDEX).await,
        Err(CtError::Status(404))
    );
    let unreachable = FakeFetcher::new(vec![]);
    assert!(matches!(
        fetch_data_tile(&unreachable, &log(), TILE_INDEX).await,
        Err(CtError::Fetch(_))
    ));
    let short = FakeFetcher::new(vec![(
        TILE_URL,
        200,
        TILE[..TILE.len() / 2].to_vec(),
        false,
    )]);
    assert!(matches!(
        fetch_data_tile(&short, &log(), TILE_INDEX).await,
        Err(CtError::Malformed(_)) | Err(CtError::Truncated)
    ));
}

#[tokio::test]
async fn fetch_log_list_reads_json() {
    let url = Url::parse("https://example.org/log_list.json").unwrap();
    let fetcher = FakeFetcher::new(vec![(url.as_str(), 200, LIST.to_vec(), false)]);
    let logs = fetch_log_list(&fetcher, &url).await.unwrap();
    assert!(!logs.is_empty());
}
