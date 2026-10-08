use std::{env, num::NonZeroU32, sync::Arc, time::Duration};

use fetch::{FetchSettings, PoliteFetcher};
use lodestone_core::{CompanyBuilder, NormalizedDomain};
use sources::company::{CompanyResearch, CompanySettings};
use url::Url;

const SITE: &str = "https://jawagg.notion.site/";

#[tokio::test]
#[ignore = "reads jawagg.notion.site with at most six requests, set LODESTONE_LINKS_LIVE=1"]
async fn a_notion_site_keeps_only_its_confirmed_pages() {
    if env::var("LODESTONE_LINKS_LIVE").ok().as_deref() != Some("1") {
        return;
    }
    let fetcher = Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact: "https://github.com/ferrislabs/lodestone".to_owned(),
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
            entreprises_url: Url::parse("https://recherche-entreprises.api.gouv.fr/search")
                .unwrap()
                .into(),
            max_pages: 4,
            max_rate_limit_wait: Duration::from_secs(10),
            origin: Some(Url::parse(SITE).unwrap()),
        },
    );
    let domain = NormalizedDomain::from_host("jawagg.notion.site").unwrap();

    let build = research.build(&domain).await.unwrap();

    println!("state: {}", build.state().as_str());
    println!("failure: {:?}", build.failure);
    for page in &build.pages {
        println!("page: {} ({} chars)", page.url, page.text.chars().count());
    }
    println!("unconfirmed: {}", build.unconfirmed);
    assert!(
        build
            .pages
            .iter()
            .all(|page| page.url.trim_end_matches('/') == SITE.trim_end_matches('/'))
    );
}
