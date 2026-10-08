use std::{env, num::NonZeroU32, sync::Arc, time::Duration};

use chrono::Utc;
use fetch::{FetchSettings, PoliteFetcher};
use lodestone_core::{
    Inspection, InspectionSettings, InspectionTarget, NormalizedDomain, Siren, SirenLookup,
};
use sources::{
    bodacc::{BODACC_URL, BodaccSettings, LegalLifeModule},
    company::{CompanyResearch, CompanySettings},
    decp::{
        ENTREPRISES_URL,
        holders::{DECP_HOLDERS_URL, HolderSettings, PublicContractsModule},
    },
};
use url::Url;

const DEFAULT_SIREN: &str = "492283445";

fn print(name: &str, inspection: &Inspection) {
    match inspection {
        Inspection::NotApplicable(reason) => println!("{name}: not applicable ({reason})"),
        Inspection::Found(findings) => {
            println!("{name}: {} findings", findings.len());
            for finding in findings {
                println!(
                    "  {} | {} | {} | {}",
                    finding.label, finding.value, finding.details, finding.source_url
                );
            }
        }
    }
}

#[tokio::test]
#[ignore = "reads the real BODACC and DECP open data APIs, set LODESTONE_ECOSYSTEM_LIVE=1"]
async fn the_registry_modules_read_the_real_open_data() {
    if env::var("LODESTONE_ECOSYSTEM_LIVE").ok().as_deref() != Some("1") {
        return;
    }
    let siren = Siren::parse(
        &env::var("LODESTONE_ECOSYSTEM_SIREN").unwrap_or_else(|_| DEFAULT_SIREN.to_owned()),
    )
    .unwrap();
    let fetcher = Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact: "https://github.com/ferrislabs/lodestone".to_owned(),
            requests_per_host_per_minute: NonZeroU32::new(6).unwrap(),
            request_timeout: Duration::from_secs(60),
            max_body_bytes: 8 * 1024 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(3_600),
            source_quotas: Vec::new(),
            allow_private_targets: false,
        })
        .unwrap(),
    );
    let wait = Duration::from_secs(30);
    let research = CompanyResearch::new(
        Arc::clone(&fetcher),
        CompanySettings {
            entreprises_url: Url::parse(ENTREPRISES_URL).unwrap().into(),
            max_pages: 1,
            max_rate_limit_wait: wait,
            origin: None,
        },
    );
    let legal = LegalLifeModule::new(
        Arc::clone(&fetcher),
        BodaccSettings {
            dataset_url: Url::parse(BODACC_URL).unwrap().into(),
            max_rate_limit_wait: wait,
        },
        research.clone(),
    );
    let contracts = PublicContractsModule::new(
        fetcher,
        HolderSettings {
            holders_url: Url::parse(DECP_HOLDERS_URL).unwrap().into(),
            max_rate_limit_wait: wait,
        },
        research,
    );
    let domain = NormalizedDomain::from_host("example.fr").unwrap();
    let settings = InspectionSettings::default();
    let target = InspectionTarget {
        domain: &domain,
        settings: &settings,
        dossier: None,
        siren: SirenLookup::Known(&siren),
        now: Utc::now(),
    };

    let announcements = legal.inspect(&target).await.unwrap();
    let awards = contracts.inspect(&target).await.unwrap();

    println!("siren: {siren}");
    print("legal_life", &announcements);
    print("public_contracts", &awards);
    assert!(matches!(announcements, Inspection::Found(_)));
    assert!(matches!(awards, Inspection::Found(_)));
}
