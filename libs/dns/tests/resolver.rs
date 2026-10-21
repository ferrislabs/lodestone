use std::env::var;

use dns::{DnsSettings, HickoryResolver};
use lodestone_core::{DnsRecords, DnsResolver, Hostname, NormalizedDomain};

#[tokio::test]
async fn resolves_real_cname_chain() {
    if var("LODESTONE_NETWORK_TESTS").as_deref() != Ok("1") {
        println!("skipped: set LODESTONE_NETWORK_TESTS=1");
        return;
    }
    let resolver = HickoryResolver::from_system(DnsSettings::default()).unwrap();
    let host = Hostname::parse("www.github.com").unwrap();
    let chain = resolver.cname_chain(&host).await.unwrap();
    println!("chain: {chain:?}");
    assert!(!chain.is_empty());
}

#[tokio::test]
async fn reads_real_mx_and_txt_records() {
    if var("LODESTONE_NETWORK_TESTS").as_deref() != Ok("1") {
        println!("skipped: set LODESTONE_NETWORK_TESTS=1");
        return;
    }
    let resolver = HickoryResolver::from_system(DnsSettings::default()).unwrap();
    let domain = NormalizedDomain::from_host("gitlab.com").unwrap();
    let mx = resolver.mx_hosts(&domain).await.unwrap();
    println!("mx: {mx:?}");
    assert!(!mx.is_empty());
    let txt = resolver.txt_records("gitlab.com").await.unwrap();
    println!("txt: {}", txt.len());
    assert!(txt.iter().any(|record| record.starts_with("v=spf1")));
}
