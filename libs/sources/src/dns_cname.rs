use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, CandidateHost, CandidateHosts, DnsError, DnsResolver, Hostname, Observation,
    Provenance, SignalKind, Source, SourceError, SourceName, UnitInterval,
    definition::SourceTarget, transient_checked_at,
};
use serde_json::json;
use tokio::{sync::Semaphore, task::JoinSet};

const SIGNAL_KIND: &str = "saas_provider";
const STRENGTH: f64 = 0.9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DnsCnameSettings {
    pub batch_size: u32,
    pub concurrency: usize,
    pub revisit_interval: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedHost {
    pub host: CandidateHost,
    pub chain: Vec<Hostname>,
    pub resolved_at: DateTime<Utc>,
}

enum Lookup {
    Resolved(ResolvedHost),
    Failed(Hostname),
    Transient(Hostname),
}

#[derive(Default)]
struct Resolution {
    resolved: Vec<ResolvedHost>,
    failed: Vec<Hostname>,
    transient: Vec<Hostname>,
}

pub struct DnsCnameSource<R: DnsResolver, H: CandidateHosts> {
    resolver: Arc<R>,
    hosts: Arc<H>,
    settings: DnsCnameSettings,
}

impl<R: DnsResolver, H: CandidateHosts> DnsCnameSource<R, H> {
    pub fn new(resolver: Arc<R>, hosts: Arc<H>, settings: DnsCnameSettings) -> Self {
        Self {
            resolver,
            hosts,
            settings,
        }
    }
}

fn upstream(error: impl std::fmt::Display) -> SourceError {
    SourceError::Upstream(error.to_string())
}

fn hop_matches(hop: &str, suffix: &str) -> bool {
    hop == suffix
        || hop
            .strip_suffix(suffix)
            .is_some_and(|rest| rest.ends_with('.'))
}

impl<R, H> DnsCnameSource<R, H>
where
    R: DnsResolver + 'static,
    H: CandidateHosts + 'static,
{
    async fn resolve_all(&self, due: Vec<CandidateHost>, now: DateTime<Utc>) -> Resolution {
        let permits = Arc::new(Semaphore::new(self.settings.concurrency.max(1)));
        let mut tasks = JoinSet::new();
        for (index, host) in due.into_iter().enumerate() {
            let resolver = Arc::clone(&self.resolver);
            let permits = Arc::clone(&permits);
            tasks.spawn(async move {
                let _permit = permits.acquire_owned().await.ok()?;
                match resolver.cname_chain(&host.host).await {
                    Ok(chain) => Some((
                        index,
                        Lookup::Resolved(ResolvedHost {
                            host,
                            chain,
                            resolved_at: now,
                        }),
                    )),
                    Err(error) => {
                        tracing::debug!(host = %host.host, %error, "cname lookup failed");
                        Some((
                            index,
                            match error {
                                DnsError::Timeout => Lookup::Transient(host.host),
                                DnsError::Failure(_) => Lookup::Failed(host.host),
                            },
                        ))
                    }
                }
            });
        }
        let mut entries = Vec::new();
        while let Some(joined) = tasks.join_next().await {
            match joined {
                Ok(Some(entry)) => entries.push(entry),
                Ok(None) => {}
                Err(error) => tracing::debug!(%error, "cname lookup task aborted"),
            }
        }
        entries.sort_by_key(|(index, _)| *index);
        let mut resolution = Resolution::default();
        for (_, lookup) in entries {
            match lookup {
                Lookup::Resolved(item) => resolution.resolved.push(item),
                Lookup::Failed(host) => resolution.failed.push(host),
                Lookup::Transient(host) => resolution.transient.push(host),
            }
        }
        resolution
    }
}

impl<R, H> Source for DnsCnameSource<R, H>
where
    R: DnsResolver + 'static,
    H: CandidateHosts + 'static,
{
    type Item = ResolvedHost;

    fn name(&self) -> SourceName {
        SourceName::DnsCname
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> Result<Vec<Self::Item>, SourceError> {
        let interval = chrono::Duration::from_std(self.settings.revisit_interval)
            .map_err(|_| upstream("revisit interval is out of range"))?;
        let checked_before = now
            .checked_sub_signed(interval)
            .ok_or_else(|| upstream("revisit interval is out of range"))?;
        let due = self
            .hosts
            .list_due(
                SourceName::DnsCname,
                checked_before,
                self.settings.batch_size,
            )
            .await
            .map_err(upstream)?;
        let Resolution {
            resolved,
            failed,
            transient,
        } = self.resolve_all(due, now).await;
        let mut names: Vec<Hostname> = resolved.iter().map(|item| item.host.host.clone()).collect();
        names.extend(failed);
        self.hosts
            .mark_checked(&names, SourceName::DnsCname, now)
            .await
            .map_err(upstream)?;
        if !transient.is_empty() {
            self.hosts
                .mark_checked(
                    &transient,
                    SourceName::DnsCname,
                    transient_checked_at(now, self.settings.revisit_interval),
                )
                .await
                .map_err(upstream)?;
        }
        Ok(resolved)
    }

    fn evaluate(
        &self,
        item: &Self::Item,
        _profile: &ActiveProfile,
        target: &SourceTarget<'_>,
    ) -> Vec<Observation> {
        let Some(config) = target.dns() else {
            return Vec::new();
        };
        let Ok(kind) = SignalKind::parse(SIGNAL_KIND) else {
            return Vec::new();
        };
        let Ok(strength) = UnitInterval::new(STRENGTH) else {
            return Vec::new();
        };
        let Ok(provenance) = Provenance::parse(&format!("dns:{}?type=CNAME", item.host.host))
        else {
            return Vec::new();
        };
        let mut seen: Vec<&str> = Vec::new();
        let mut observations = Vec::new();
        for pattern in &config.patterns {
            if seen.contains(&pattern.provider.as_str()) {
                continue;
            }
            let suffix = pattern.target_suffix.to_ascii_lowercase();
            let Some(matched) = item
                .chain
                .iter()
                .find(|hop| hop_matches(hop.as_str(), &suffix))
            else {
                continue;
            };
            seen.push(pattern.provider.as_str());
            observations.push(Observation {
                domain: item.host.domain.clone(),
                kind: kind.clone(),
                value: Some(pattern.provider.clone()),
                raw: json!({
                    "host": item.host.host.as_str(),
                    "chain": item.chain.iter().map(Hostname::as_str).collect::<Vec<_>>(),
                    "matched": matched.as_str(),
                }),
                provenance: provenance.clone(),
                strength,
                observed_at: item.resolved_at,
                agent: None,
            });
        }
        observations
    }
}

#[cfg(test)]
mod tests;
