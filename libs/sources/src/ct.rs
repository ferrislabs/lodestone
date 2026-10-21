use std::{
    collections::{BTreeSet, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use ct::{CtError, CtLog, TILE_WIDTH, fetch_data_tile, fetch_log_list, fetch_tree_size};
use lodestone_core::{
    ActiveProfile, CandidateHost, CandidateHosts, CtCursors, FetchError, Fetcher, HostOrigin,
    Hostname, Observation, Source, SourceError, SourceName, definition::Agent,
};
use tracing::{info, warn};
use url::Url;

use crate::endpoint::Endpoint;

#[derive(Debug, Clone)]
pub struct CtSettings {
    pub log_list_url: Endpoint,
    pub log_list_ttl: Duration,
    pub max_logs: usize,
    pub tiles_per_log: u64,
    pub max_lag_tiles: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CtRunStats {
    pub entries: u64,
    pub names: u64,
    pub matched: u64,
    pub skipped_tiles: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct LogPass {
    tiles: u64,
    entries: u64,
    matched: BTreeSet<Hostname>,
    lag_tiles: u64,
}

fn log_summary(name: &str, pass: &LogPass) -> String {
    format!(
        "{name}: tiles={} certificates={} matched={} lag={}",
        pass.tiles,
        pass.entries,
        pass.matched.len(),
        pass.lag_tiles
    )
}

struct CachedLogs {
    fetched_at: DateTime<Utc>,
    url: Url,
    logs: Arc<[CtLog]>,
}

pub struct CtSource<F: Fetcher, H: CandidateHosts, C: CtCursors> {
    fetcher: Arc<F>,
    hosts: Arc<H>,
    cursors: Arc<C>,
    settings: CtSettings,
    logs: Mutex<Option<CachedLogs>>,
    tiles_read: AtomicU64,
    lag_tiles: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CtPass {
    pub tiles_read: u64,
    pub lag_tiles: u64,
}

pub fn label_filter(profiles: &[ActiveProfile]) -> HashSet<String> {
    profiles
        .iter()
        .flat_map(|active| {
            active
                .revision
                .definition
                .definition()
                .enabled_agents(SourceName::CertificateTransparency)
        })
        .filter_map(|agent| agent.certificate_transparency())
        .flat_map(|ct| ct.labels.iter().cloned())
        .collect()
}

pub fn matching_host(name: &str, labels: &HashSet<String>) -> Option<Hostname> {
    let name = name.strip_prefix("*.").unwrap_or(name);
    let host = Hostname::parse(name).ok()?;
    let domain = host.registrable_domain()?;
    let subdomain = host
        .as_str()
        .strip_suffix(domain.as_str())?
        .strip_suffix('.')?;
    subdomain
        .split('.')
        .any(|label| labels.contains(label))
        .then_some(host)
}

fn upstream(error: impl std::fmt::Display) -> SourceError {
    SourceError::Upstream(error.to_string())
}

impl<F, H, C> CtSource<F, H, C>
where
    F: Fetcher,
    H: CandidateHosts,
    C: CtCursors,
{
    pub fn new(fetcher: Arc<F>, hosts: Arc<H>, cursors: Arc<C>, settings: CtSettings) -> Self {
        Self {
            fetcher,
            hosts,
            cursors,
            settings,
            logs: Mutex::new(None),
            tiles_read: AtomicU64::new(0),
            lag_tiles: AtomicU64::new(0),
        }
    }

    pub fn last_pass(&self) -> CtPass {
        CtPass {
            tiles_read: self.tiles_read.load(Ordering::Relaxed),
            lag_tiles: self.lag_tiles.load(Ordering::Relaxed),
        }
    }

    async fn logs(&self, now: DateTime<Utc>) -> Result<Arc<[CtLog]>, SourceError> {
        let ttl = chrono::Duration::from_std(self.settings.log_list_ttl)
            .unwrap_or(chrono::Duration::hours(6));
        let url = self.settings.log_list_url.resolve().await?;
        if let Ok(guard) = self.logs.lock()
            && let Some(cached) = guard.as_ref()
            && cached.url == url
            && now - cached.fetched_at < ttl
        {
            return Ok(Arc::clone(&cached.logs));
        }
        let mut logs = fetch_log_list(self.fetcher.as_ref(), &url)
            .await
            .map_err(upstream)?;
        logs.sort_by(|a, b| a.description.cmp(&b.description));
        logs.truncate(self.settings.max_logs);
        let logs: Arc<[CtLog]> = logs.into();
        if let Ok(mut guard) = self.logs.lock() {
            *guard = Some(CachedLogs {
                fetched_at: now,
                url,
                logs: Arc::clone(&logs),
            });
        }
        Ok(logs)
    }

    async fn read_log(
        &self,
        log: &CtLog,
        labels: &HashSet<String>,
        now: DateTime<Utc>,
        pass: &mut LogPass,
        stats: &mut CtRunStats,
    ) -> Result<(), CtError> {
        let full_tiles = fetch_tree_size(self.fetcher.as_ref(), log).await? / TILE_WIDTH;
        let Some(next_index) = self
            .cursors
            .next_index(&log.log_id)
            .await
            .map_err(|e| CtError::Malformed(e.to_string()))?
        else {
            self.advance(log, full_tiles, now).await?;
            info!(log = %log.description, start_tile = full_tiles, "ct log cursor initialised at the current tree size");
            return Ok(());
        };
        let mut next_tile = next_index / TILE_WIDTH;
        let lag = full_tiles.saturating_sub(next_tile);
        if lag > self.settings.max_lag_tiles {
            let skipped = lag - self.settings.max_lag_tiles;
            warn!(log = %log.description, skipped_tiles = skipped, "ct log too far behind, skipping ahead");
            stats.skipped_tiles += skipped;
            next_tile += skipped;
        }
        let end = full_tiles.min(next_tile + self.settings.tiles_per_log);
        pass.lag_tiles = full_tiles.saturating_sub(next_tile);
        self.lag_tiles.fetch_add(pass.lag_tiles, Ordering::Relaxed);
        for tile in next_tile..end {
            let entries = fetch_data_tile(self.fetcher.as_ref(), log, tile).await?;
            for entry in &entries {
                stats.entries += 1;
                pass.entries += 1;
                for name in &entry.dns_names {
                    stats.names += 1;
                    if let Some(host) = matching_host(name, labels) {
                        pass.matched.insert(host);
                    }
                }
            }
            self.advance(log, tile + 1, now).await?;
            pass.tiles += 1;
            pass.lag_tiles = pass.lag_tiles.saturating_sub(1);
            self.tiles_read.fetch_add(1, Ordering::Relaxed);
            self.lag_tiles.fetch_sub(1, Ordering::Relaxed);
        }
        Ok(())
    }

    async fn advance(&self, log: &CtLog, tile: u64, now: DateTime<Utc>) -> Result<(), CtError> {
        self.cursors
            .advance(&log.log_id, tile * TILE_WIDTH, now)
            .await
            .map_err(|e| CtError::Malformed(e.to_string()))
    }
}

impl<F, H, C> Source for CtSource<F, H, C>
where
    F: Fetcher,
    H: CandidateHosts,
    C: CtCursors,
{
    type Item = CandidateHost;

    fn name(&self) -> SourceName {
        SourceName::CertificateTransparency
    }

    async fn collect(
        &self,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<Vec<CandidateHost>, SourceError> {
        self.tiles_read.store(0, Ordering::Relaxed);
        self.lag_tiles.store(0, Ordering::Relaxed);
        let labels = label_filter(profiles);
        if labels.is_empty() {
            return Ok(Vec::new());
        }
        let logs = self.logs(now).await?;
        let mut found = BTreeSet::new();
        let mut stats = CtRunStats::default();
        let mut summaries = Vec::with_capacity(logs.len());
        let mut tiles = 0u64;
        let mut lag = 0u64;
        for log in logs.iter() {
            let mut pass = LogPass::default();
            let result = self
                .read_log(log, &labels, now, &mut pass, &mut stats)
                .await;
            tiles += pass.tiles;
            lag += pass.lag_tiles;
            summaries.push(log_summary(&log.description, &pass));
            found.extend(pass.matched);
            match result {
                Ok(()) => {}
                Err(CtError::Fetch(FetchError::RateLimited { .. })) => {
                    info!(log = %log.description, "ct log rate limited, resuming next run");
                }
                Err(error) => warn!(log = %log.description, %error, "ct log read failed"),
            }
        }
        let hosts: Vec<CandidateHost> = found
            .into_iter()
            .filter_map(|host| {
                CandidateHost::discovered(host, HostOrigin::CertificateTransparency, now).ok()
            })
            .collect();
        stats.matched = u64::try_from(hosts.len()).unwrap_or(u64::MAX);
        info!(
            logs = logs.len(),
            tiles_read = tiles,
            certificates = stats.entries,
            names = stats.names,
            matched = stats.matched,
            lag_tiles = lag,
            skipped_tiles = stats.skipped_tiles,
            per_log = %summaries.join("; "),
            "ct ingestion pass"
        );
        if !hosts.is_empty() {
            self.hosts.upsert(&hosts).await.map_err(upstream)?;
        }
        Ok(hosts)
    }

    fn serves(&self, item: &CandidateHost, agent: &Agent) -> bool {
        agent.certificate_transparency().is_some_and(|ct| {
            let labels: HashSet<String> = ct.labels.iter().cloned().collect();
            matching_host(item.host.as_str(), &labels).is_some()
        })
    }

    fn evaluate(
        &self,
        _item: &CandidateHost,
        _profile: &ActiveProfile,
        _agent: &Agent,
    ) -> Vec<Observation> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests;
