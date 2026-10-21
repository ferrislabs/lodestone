use std::{collections::HashSet, future::Future, time::Duration};

use hickory_resolver::{
    TokioResolver,
    config::{CLOUDFLARE, ResolverConfig},
    net::{NetError, runtime::TokioRuntimeProvider},
    proto::rr::{RData, RecordType},
};
use lodestone_core::{DnsError, DnsRecords, DnsResolver, Hostname, MxLookup, NormalizedDomain};
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
    over_tcp: TokioResolver,
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
        let mut tcp_builder = TokioResolver::builder_with_config(
            ResolverConfig::from_name_servers(CLOUDFLARE.tcp().collect()),
            TokioRuntimeProvider::default(),
        );
        apply(tcp_builder.options_mut(), settings);
        let over_tcp = tcp_builder
            .build()
            .map_err(|e| DnsBuildError(e.to_string()))?;
        Ok(Self {
            inner,
            over_tcp,
            max_chain: settings.max_chain,
        })
    }

    async fn records(&self, name: &str, kind: RecordType) -> Result<Vec<RData>, NetError> {
        let lookup = match self.inner.lookup(name, kind).await {
            Err(NetError::Truncated) => self.over_tcp.lookup(name, kind).await,
            other => other,
        }?;
        Ok(lookup
            .answers()
            .iter()
            .map(|record| record.data.clone())
            .collect())
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
    options.edns0 = true;
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

pub fn txt_text(parts: &[Box<[u8]>]) -> String {
    parts
        .iter()
        .map(|part| String::from_utf8_lossy(part))
        .collect::<Vec<_>>()
        .concat()
}

pub fn mx_order(mut exchanges: Vec<(u16, String)>) -> Vec<Hostname> {
    exchanges.sort_by_key(|(preference, _)| *preference);
    let mut hosts: Vec<Hostname> = Vec::new();
    for (_, exchange) in exchanges {
        if let Ok(host) = Hostname::parse(&exchange)
            && !hosts.contains(&host)
        {
            hosts.push(host);
        }
    }
    hosts
}

impl DnsRecords for HickoryResolver {
    async fn mx_hosts(&self, domain: &NormalizedDomain) -> Result<Vec<Hostname>, DnsError> {
        match self.records(domain.as_str(), RecordType::MX).await {
            Ok(records) => Ok(mx_order(
                records
                    .iter()
                    .filter_map(|data| match data {
                        RData::MX(mx) => Some((mx.preference, mx.exchange.to_utf8())),
                        _ => None,
                    })
                    .collect(),
            )),
            Err(error) => classify(&error).map(|_| Vec::new()),
        }
    }

    async fn txt_records(&self, name: &str) -> Result<Vec<String>, DnsError> {
        match self.records(name, RecordType::TXT).await {
            Ok(records) => Ok(records
                .iter()
                .filter_map(|data| match data {
                    RData::TXT(txt) => Some(txt_text(&txt.txt_data)),
                    _ => None,
                })
                .collect()),
            Err(error) => classify(&error).map(|_| Vec::new()),
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

    #[test]
    fn txt_character_strings_are_joined() {
        let parts: Vec<Box<[u8]>> = vec![
            b"v=spf1 include:_spf.google.com "
                .to_vec()
                .into_boxed_slice(),
            b"~all".to_vec().into_boxed_slice(),
        ];
        assert_eq!(txt_text(&parts), "v=spf1 include:_spf.google.com ~all");
    }

    #[test]
    fn mx_hosts_follow_their_preference_without_duplicates() {
        let hosts = mx_order(vec![
            (20, "alt1.aspmx.l.google.com.".to_owned()),
            (10, "aspmx.l.google.com.".to_owned()),
            (30, "aspmx.l.google.com.".to_owned()),
            (40, ".".to_owned()),
        ]);
        assert_eq!(
            hosts,
            [host("aspmx.l.google.com"), host("alt1.aspmx.l.google.com")]
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
