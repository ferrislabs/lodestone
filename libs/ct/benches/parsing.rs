//! Benchmarks for Certificate Transparency parsing. Every data tile fetched
//! from a CT log goes through `parse_data_tile`, which decodes up to 256
//! entries and extracts the DNS names of each certificate.

use chrono::{TimeZone, Utc};
use ct::{parse_checkpoint, parse_data_tile, parse_log_list};

const LIST: &[u8] = include_bytes!("../tests/fixtures/log_list.json");
const CHECKPOINT: &str = include_str!("../tests/fixtures/checkpoint.txt");
const TILE: &[u8] = include_bytes!("../tests/fixtures/le-sycamore2026h2-tile-3874719.bin");
const TILE_INDEX: u64 = 3874719;

fn main() {
    divan::main();
}

#[divan::bench]
fn data_tile() -> usize {
    parse_data_tile(divan::black_box(TILE), TILE_INDEX)
        .unwrap()
        .len()
}

#[divan::bench]
fn log_list() -> usize {
    let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
    parse_log_list(divan::black_box(LIST), now).unwrap().len()
}

#[divan::bench]
fn checkpoint() -> u64 {
    parse_checkpoint(divan::black_box(CHECKPOINT)).unwrap()
}
