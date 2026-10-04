use lodestone_core::Fetcher;
use url::Url;

use crate::error::CtError;
use crate::list::{CtLog, fetch_body};
use crate::names::{names_from_certificate, names_from_tbs};

pub const TILE_WIDTH: u64 = 256;

const ENTRY_X509: u16 = 0;
const ENTRY_PRECERT: u16 = 1;
const ISSUER_KEY_HASH_LEN: usize = 32;
const FINGERPRINT_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtEntry {
    pub index: u64,
    pub timestamp_ms: u64,
    pub dns_names: Vec<String>,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn is_empty(&self) -> bool {
        self.position >= self.bytes.len()
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], CtError> {
        let end = self
            .position
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| {
                CtError::Malformed(format!(
                    "need {count} bytes at offset {}, {} available",
                    self.position,
                    self.bytes.len().saturating_sub(self.position)
                ))
            })?;
        let slice = &self.bytes[self.position..end];
        self.position = end;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, CtError> {
        let raw = self.take(2)?;
        Ok(u16::from_be_bytes([raw[0], raw[1]]))
    }

    fn u64(&mut self) -> Result<u64, CtError> {
        let raw = self.take(8)?;
        let mut array = [0u8; 8];
        array.copy_from_slice(raw);
        Ok(u64::from_be_bytes(array))
    }

    fn vec_u16(&mut self) -> Result<&'a [u8], CtError> {
        let length = usize::from(self.u16()?);
        self.take(length)
    }

    fn vec_u24(&mut self) -> Result<&'a [u8], CtError> {
        let raw = self.take(3)?;
        let length = (usize::from(raw[0]) << 16) | (usize::from(raw[1]) << 8) | usize::from(raw[2]);
        self.take(length)
    }
}

fn parse_leaf(cursor: &mut Cursor<'_>, index: u64) -> Result<CtEntry, CtError> {
    let timestamp_ms = cursor.u64()?;
    let entry_type = cursor.u16()?;
    let dns_names = match entry_type {
        ENTRY_X509 => {
            let certificate = cursor.vec_u24()?;
            cursor.vec_u16()?;
            names_from_certificate(certificate)
        }
        ENTRY_PRECERT => {
            cursor.take(ISSUER_KEY_HASH_LEN)?;
            let tbs = cursor.vec_u24()?;
            cursor.vec_u16()?;
            cursor.vec_u24()?;
            names_from_tbs(tbs)
        }
        other => {
            return Err(CtError::Malformed(format!(
                "unknown entry type {other} at index {index}"
            )));
        }
    };
    let fingerprints = cursor.vec_u16()?;
    if fingerprints.len() % FINGERPRINT_LEN != 0 {
        return Err(CtError::Malformed(format!(
            "fingerprint list of {} bytes at index {index}",
            fingerprints.len()
        )));
    }
    Ok(CtEntry {
        index,
        timestamp_ms,
        dns_names,
    })
}

pub fn parse_data_tile(bytes: &[u8], first_index: u64) -> Result<Vec<CtEntry>, CtError> {
    let mut cursor = Cursor::new(bytes);
    let mut entries = Vec::new();
    let mut index = first_index;
    while !cursor.is_empty() {
        entries.push(parse_leaf(&mut cursor, index)?);
        index = index
            .checked_add(1)
            .ok_or_else(|| CtError::Malformed("entry index overflow".to_owned()))?;
    }
    Ok(entries)
}

pub fn parse_checkpoint(text: &str) -> Result<u64, CtError> {
    let line = text
        .lines()
        .nth(1)
        .ok_or_else(|| CtError::Malformed("checkpoint has no tree size line".to_owned()))?;
    line.trim()
        .parse::<u64>()
        .map_err(|_| CtError::Malformed(format!("checkpoint tree size is not a number: {line:?}")))
}

fn encode_tile_index(tile_index: u64) -> Vec<String> {
    let digits = tile_index.to_string();
    let padding = (3 - digits.len() % 3) % 3;
    let padded = format!("{}{digits}", "0".repeat(padding));
    let groups: Vec<&str> = padded
        .as_bytes()
        .chunks(3)
        .filter_map(|chunk| std::str::from_utf8(chunk).ok())
        .collect();
    let last = groups.len().saturating_sub(1);
    groups
        .iter()
        .enumerate()
        .map(|(position, group)| {
            if position < last {
                format!("x{group}")
            } else {
                (*group).to_owned()
            }
        })
        .collect()
}

fn monitoring_path(log: &CtLog, segments: &[&str]) -> Url {
    let mut url = log.monitoring_url.clone();
    if let Ok(mut path) = url.path_segments_mut() {
        path.pop_if_empty().extend(segments);
    }
    url
}

pub fn data_tile_url(log: &CtLog, tile_index: u64) -> Url {
    let encoded = encode_tile_index(tile_index);
    let mut segments: Vec<&str> = vec!["tile", "data"];
    segments.extend(encoded.iter().map(String::as_str));
    monitoring_path(log, &segments)
}

pub async fn fetch_tree_size<F: Fetcher>(fetcher: &F, log: &CtLog) -> Result<u64, CtError> {
    let body = fetch_body(fetcher, monitoring_path(log, &["checkpoint"])).await?;
    let text = String::from_utf8_lossy(&body);
    parse_checkpoint(&text)
}

pub async fn fetch_data_tile<F: Fetcher>(
    fetcher: &F,
    log: &CtLog,
    tile_index: u64,
) -> Result<Vec<CtEntry>, CtError> {
    let first_index = tile_index
        .checked_mul(TILE_WIDTH)
        .ok_or_else(|| CtError::Malformed("tile index overflow".to_owned()))?;
    let body = fetch_body(fetcher, data_tile_url(log, tile_index)).await?;
    let entries = parse_data_tile(&body, first_index)?;
    if entries.len() as u64 != TILE_WIDTH {
        return Err(CtError::Malformed(format!(
            "expected {TILE_WIDTH} entries, found {}",
            entries.len()
        )));
    }
    Ok(entries)
}
