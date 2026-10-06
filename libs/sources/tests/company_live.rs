use std::{num::NonZeroU32, sync::Arc, time::Duration};

use fetch::{FetchSettings, PoliteFetcher};
use lodestone_core::{CompanyBuilder, NormalizedDomain};
use sources::{
    company::{CompanyResearch, CompanySettings},
    decp::ENTREPRISES_URL,
};
use url::Url;

const DEFAULT_DOMAIN: &str = "scaleway.com";

#[tokio::test]
#[ignore = "reads a real public company site, set LODESTONE_COMPANY_LIVE=1"]
async fn a_real_french_company_site_gives_a_dossier() {
    if std::env::var("LODESTONE_COMPANY_LIVE").ok().as_deref() != Some("1") {
        return;
    }
    let contact = std::env::var("LODESTONE_COMPANY_CONTACT")
        .unwrap_or_else(|_| "https://github.com/ferrislabs/lodestone".to_owned());
    let host =
        std::env::var("LODESTONE_COMPANY_DOMAIN").unwrap_or_else(|_| DEFAULT_DOMAIN.to_owned());
    let fetcher = Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact,
            requests_per_host_per_minute: NonZeroU32::new(12).unwrap(),
            request_timeout: Duration::from_secs(20),
            max_body_bytes: 2 * 1024 * 1024,
            max_redirects: 5,
            robots_ttl: Duration::from_secs(3_600),
            source_quotas: Vec::new(),
            allow_private_targets: false,
        })
        .unwrap(),
    );
    let research = CompanyResearch::new(
        fetcher,
        CompanySettings {
            entreprises_url: Url::parse(ENTREPRISES_URL).unwrap(),
            max_pages: 5,
            max_rate_limit_wait: Duration::from_secs(75),
            origin: None,
        },
    );
    let domain = NormalizedDomain::from_host(&host).unwrap();

    let build = research.build(&domain).await.unwrap();

    println!("domain: {host}");
    println!("state: {}", build.state().as_str());
    println!("failure: {:?}", build.failure);
    println!(
        "siren: {:?}",
        build.siren.as_ref().map(|siren| siren.as_str())
    );
    for page in &build.pages {
        println!("page: {} ({} chars)", page.url, page.text.chars().count());
    }
    match &build.official {
        Some(official) => {
            println!("legal_name: {}", official.legal_name);
            println!("naf: {:?} / {:?}", official.naf_code, official.naf_label);
            println!("headcount: {:?}", official.headcount_range);
            println!(
                "head_office: {:?} {:?}, {:?}",
                official.head_office_postcode,
                official.head_office_city,
                official.head_office_country
            );
            println!("created_on: {:?}", official.created_on);
        }
        None => println!("official: none"),
    }
    assert!(build.reachable);
    assert!(!build.pages.is_empty());
}
