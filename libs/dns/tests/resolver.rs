use dns::{DnsSettings, HickoryResolver};
use lodestone_core::{DnsResolver, Hostname};

#[tokio::test]
async fn resolves_real_cname_chain() {
    if std::env::var("LODESTONE_NETWORK_TESTS").as_deref() != Ok("1") {
        println!("skipped: set LODESTONE_NETWORK_TESTS=1");
        return;
    }
    let resolver = HickoryResolver::from_system(DnsSettings::default()).unwrap();
    let host = Hostname::parse("www.github.com").unwrap();
    let chain = resolver.cname_chain(&host).await.unwrap();
    println!("chain: {chain:?}");
    assert!(!chain.is_empty());
}
