use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use lodestone_core::{FetchPurpose, FetchRequest, Fetcher, SourceName};
use serde::Deserialize;
use url::Url;

use crate::error::CtError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtLog {
    pub log_id: String,
    pub description: String,
    pub operator: String,
    pub monitoring_url: Url,
}

#[derive(Deserialize)]
struct LogList {
    operators: Vec<Operator>,
}

#[derive(Deserialize)]
struct Operator {
    name: String,
    #[serde(default)]
    tiled_logs: Vec<TiledLog>,
}

#[derive(Deserialize)]
struct TiledLog {
    log_id: String,
    #[serde(default)]
    description: String,
    monitoring_url: String,
    #[serde(default)]
    state: BTreeMap<String, serde_json::Value>,
    temporal_interval: Option<Interval>,
}

#[derive(Deserialize)]
struct Interval {
    start_inclusive: DateTime<Utc>,
    end_exclusive: DateTime<Utc>,
}

impl TiledLog {
    fn accepts(&self, now: DateTime<Utc>) -> bool {
        let state_ok = self
            .state
            .keys()
            .any(|key| key == "usable" || key == "qualified");
        let interval_ok = self
            .temporal_interval
            .as_ref()
            .is_none_or(|interval| interval.start_inclusive <= now && now < interval.end_exclusive);
        state_ok && interval_ok
    }
}

pub fn parse_log_list(json: &[u8], now: DateTime<Utc>) -> Result<Vec<CtLog>, CtError> {
    let list: LogList =
        serde_json::from_slice(json).map_err(|error| CtError::Malformed(error.to_string()))?;
    let mut logs = Vec::new();
    for operator in list.operators {
        for tiled in operator.tiled_logs {
            if !tiled.accepts(now) {
                continue;
            }
            let Ok(monitoring_url) = Url::parse(&tiled.monitoring_url) else {
                continue;
            };
            logs.push(CtLog {
                log_id: tiled.log_id,
                description: tiled.description,
                operator: operator.name.clone(),
                monitoring_url,
            });
        }
    }
    Ok(logs)
}

pub async fn fetch_log_list<F: Fetcher>(
    fetcher: &F,
    list_url: &Url,
) -> Result<Vec<CtLog>, CtError> {
    let body = fetch_body(fetcher, list_url.clone()).await?;
    parse_log_list(&body, Utc::now())
}

pub(crate) async fn fetch_body<F: Fetcher>(fetcher: &F, url: Url) -> Result<Vec<u8>, CtError> {
    let request = FetchRequest {
        url,
        purpose: FetchPurpose::Api,
        source: SourceName::CertificateTransparency,
    };
    let response = fetcher.fetch(&request).await?;
    if response.status != 200 {
        return Err(CtError::Status(response.status));
    }
    if response.truncated {
        return Err(CtError::Truncated);
    }
    Ok(response.body)
}
