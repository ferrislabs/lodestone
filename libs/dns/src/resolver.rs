use std::{collections::HashSet, future::Future, time::Duration};

use hickory_resolver::{
    TokioResolver,
    config::{CLOUDFLARE, ResolverConfig},
    net::{NetError, runtime::TokioRuntimeProvider},
    proto::rr::{RData, RecordType},
};
use lodestone_core::{DnsError, DnsResolver, Hostname, MxLookup, NormalizedDomain};
use thiserror::Error;

#[derive(Debug, Clone, Copy)]
pub struct DnsSettings {
    pub timeout: Duration,
    pub attempts: usize,
    pub max_chain: usize,
}

impl Default for DnsSettings {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(3),
            attempts: 2,
            max_chain: 8,
        }
    }
}

#[derive(Debug, Error)]
#[error("failed to build dns resolver: {0}")]
pub struct DnsBuildError(String);

#[derive(Clone)]
pub struct HickoryResolver {
    inner: TokioResolver,
    max_chain: usize,
}

impl HickoryResolver {
    pub fn from_system(settings: DnsSettings) -> Result<Self, DnsBuildError> {
        match TokioResolver::builder_tokio() {
            Ok(mut builder) => {
                apply(builder.options_mut(), settings);
                Self::finish(builder, settings)
            }
            Err(error) => {
                tracing::warn!(%error, "system dns config unusable, falling back to public resolvers");
                Self::with_config(ResolverConfig::udp_and_tcp(&CLOUDFLARE), settings)
            }
        }
    }

    pub fn with_config(
        config: ResolverConfig,
        settings: DnsSettings,
    ) -> Result<Self, DnsBuildError> {
        let mut builder =
            TokioResolver::builder_with_config(config, TokioRuntimeProvider::default());
        apply(builder.options_mut(), settings);
        Self::finish(builder, settings)
    }

    fn finish(
        builder: hickory_resolver::ResolverBuilder<TokioRuntimeProvider>,
        settings: DnsSettings,
    ) -> Result<Self, DnsBuildError> {
        let inner = builder.build().map_err(|e| DnsBuildError(e.to_string()))?;
        Ok(Self {
            inner,
            max_chain: settings.max_chain,
        })
    }

    async fn next_target(&self, name: &Hostname) -> Result<Option<String>, DnsError> {
        match self.inner.lookup(name.as_str(), RecordType::CNAME).await {
            Ok(lookup) => {
                let wanted = name.as_str();
                let mut fallback = None;
                for record in lookup.answers() {
                    if let RData::CNAME(target) = &record.data {
                        let owner = record.name.to_utf8();
                        let owner = owner.trim_end_matches('.');
                        if owner.eq_ignore_ascii_case(wanted) {
                            return Ok(Some(target.0.to_utf8()));
                        }
                        fallback.get_or_insert_with(|| target.0.to_utf8());
                    }
                }
                Ok(fallback)
            }
            Err(error) => classify(&error),
        }
    }
}

fn apply(options: &mut hickory_resolver::config::ResolverOpts, settings: DnsSettings) {
    options.timeout = settings.timeout;
    options.attempts = settings.attempts;
}

fn classify(error: &NetError) -> Result<Option<String>, DnsError> {
    if error.is_nx_domain() || error.is_no_records_found() {
        return Ok(None);
    }
    match error {
        NetError::Timeout => Err(DnsError::Timeout),
        other => {
            tracing::debug!(error = %other, "dns lookup failed");
            Err(DnsError::Failure(other.to_string()))
        }
    }
}

impl MxLookup for HickoryResolver {
    async fn has_mx(&self, domain: &NormalizedDomain) -> Result<bool, DnsError> {
        match self.inner.lookup(domain.as_str(), RecordType::MX).await {
            Ok(lookup) => Ok(lookup.answers().iter().any(|record| match &record.data {
                RData::MX(mx) => {
                    let exchange = mx.exchange.to_utf8();
                    !exchange.trim_end_matches('.').is_empty()
                }
                _ => false,
            })),
            Err(error) => classify(&error).map(|_| false),
        }
    }
}

impl DnsResolver for HickoryResolver {
    async fn cname_chain(&self, host: &Hostname) -> Result<Vec<Hostname>, DnsError> {
        walk(
            host,
            |name| async move { self.next_target(&name).await },
            self.max_chain,
        )
        .await
    }
}

async fn walk<F, Fut>(
    start: &Hostname,
    mut lookup: F,
    max: usize,
) -> Result<Vec<Hostname>, DnsError>
where
    F: FnMut(Hostname) -> Fut,
    Fut: Future<Output = Result<Option<String>, DnsError>>,
{
    let mut seen = HashSet::new();
    seen.insert(start.clone());
    let mut chain = Vec::new();
    let mut current = start.clone();
    while chain.len() < max {
        let Some(raw) = lookup(current.clone()).await? else {
            break;
        };
        let Ok(target) = Hostname::parse(&raw) else {
            break;
        };
        if !seen.insert(target.clone()) {
            break;
        }
        chain.push(target.clone());
        current = target;
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn host(raw: &str) -> Hostname {
        Hostname::parse(raw).unwrap()
    }

    fn table(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
            .collect()
    }

    async fn run(
        start: &str,
        map: &HashMap<String, String>,
        max: usize,
    ) -> Result<Vec<Hostname>, DnsError> {
        walk(
            &host(start),
            |name| async move { Ok(map.get(name.as_str()).cloned()) },
            max,
        )
        .await
    }

    #[tokio::test]
    async fn no_cname_is_empty() {
        let map = table(&[]);
        assert_eq!(run("a.example.com", &map, 8).await, Ok(vec![]));
    }

    #[tokio::test]
    async fn three_hops_in_order() {
        let map = table(&[
            ("a.example.com", "b.example.net."),
            ("b.example.net", "c.example.org."),
            ("c.example.org", "D.example.io."),
        ]);
        assert_eq!(
            run("a.example.com", &map, 8).await,
            Ok(vec![
                host("b.example.net"),
                host("c.example.org"),
                host("d.example.io")
            ])
        );
    }

    #[tokio::test]
    async fn loop_stops() {
        let map = table(&[
            ("a.example.com", "b.example.net"),
            ("b.example.net", "a.example.com"),
        ]);
        assert_eq!(
            run("a.example.com", &map, 8).await,
            Ok(vec![host("b.example.net")])
        );
    }

    #[tokio::test]
    async fn max_chain_enforced() {
        let map = table(&[
            ("a.example.com", "b.example.net"),
            ("b.example.net", "c.example.org"),
            ("c.example.org", "d.example.io"),
        ]);
        assert_eq!(
            run("a.example.com", &map, 2).await,
            Ok(vec![host("b.example.net"), host("c.example.org")])
        );
    }

    #[tokio::test]
    async fn invalid_target_ends_chain() {
        let map = table(&[
            ("a.example.com", "b.example.net"),
            ("b.example.net", "not a host"),
        ]);
        assert_eq!(
            run("a.example.com", &map, 8).await,
            Ok(vec![host("b.example.net")])
        );
    }

    #[tokio::test]
    async fn timeout_propagates() {
        let result = walk(
            &host("a.example.com"),
            |_| async { Err(DnsError::Timeout) },
            8,
        )
        .await;
        assert_eq!(result, Err(DnsError::Timeout));
    }
}
