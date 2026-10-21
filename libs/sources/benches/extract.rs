//! Benchmarks for the HTML and JSON extraction done by the sources on every
//! fetched page: contact address extraction, ATS board discovery and job
//! board parsing.

use chrono::{TimeZone, Utc};
use lodestone_core::NormalizedDomain;
use sources::{
    ats::{
        discover::{Board, Vendor, extract_boards},
        jobs::{html_to_text, parse_board},
    },
    contact_pages::extract::extract_addresses,
};

const CONTACT_PAGES: &[(&str, &str)] = &[
    (
        "mentions_legales",
        include_str!("../tests/fixtures/contact_pages/mentions-legales-acme-industrie.html"),
    ),
    (
        "contact_mailto",
        include_str!("../tests/fixtures/contact_pages/contact-northwind-tools.html"),
    ),
    (
        "cloudflare",
        include_str!("../tests/fixtures/contact_pages/contact-bluefjord-labs.html"),
    ),
    (
        "homepage_footer",
        include_str!("../tests/fixtures/contact_pages/homepage-footer-lumenpress.html"),
    ),
];

const CAREERS_PAGES: &[&str] = &[
    include_str!("../tests/fixtures/ats/linear-careers.html"),
    include_str!("../tests/fixtures/ats/qonto-careers.html"),
    include_str!("../tests/fixtures/ats/mistral-careers.html"),
    include_str!("../tests/fixtures/ats/contentsquare-careers.html"),
    include_str!("../tests/fixtures/ats/alan-careers.html"),
];

const GREENHOUSE: &str = include_str!("../tests/fixtures/ats/greenhouse-gitlab.json");
const LEVER: &str = include_str!("../tests/fixtures/ats/lever-qonto.json");
const ASHBY: &str = include_str!("../tests/fixtures/ats/ashby-linear.json");

fn main() {
    // The extractors compile their regexes lazily on first use: warm them up
    // so the benchmarks measure steady-state extraction only.
    for (_, html) in CONTACT_PAGES {
        extract_addresses(html);
    }
    for html in CAREERS_PAGES {
        extract_boards(html);
    }
    html_to_text("<p>warm up</p>");
    divan::main();
}

mod contact_pages {
    use super::*;

    #[divan::bench(args = ["mentions_legales", "contact_mailto", "cloudflare", "homepage_footer"])]
    fn extract(name: &str) -> usize {
        let (_, html) = CONTACT_PAGES
            .iter()
            .find(|(page, _)| *page == name)
            .unwrap();
        extract_addresses(divan::black_box(html)).len()
    }
}

mod ats {
    use super::*;

    #[divan::bench]
    fn extract_careers_boards() -> usize {
        CAREERS_PAGES
            .iter()
            .map(|html| extract_boards(divan::black_box(html)).len())
            .sum()
    }

    #[divan::bench(args = ["greenhouse", "lever", "ashby"])]
    fn parse_job_board(bencher: divan::Bencher, name: &str) {
        let (vendor, slug, host, raw) = match name {
            "greenhouse" => (Vendor::Greenhouse, "gitlab", "gitlab.com", GREENHOUSE),
            "lever" => (Vendor::Lever, "qonto", "qonto.com", LEVER),
            _ => (Vendor::Ashby, "Linear", "linear.app", ASHBY),
        };
        let board = Board {
            vendor,
            slug: slug.to_owned(),
        };
        let body: serde_json::Value = serde_json::from_str(raw).unwrap();
        let domain = NormalizedDomain::from_host(host).unwrap();
        let now = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        assert!(!parse_board(&board, &body, &domain, now).is_empty());
        bencher.bench_local(|| {
            divan::black_box(parse_board(&board, divan::black_box(&body), &domain, now))
        });
    }

    #[divan::bench]
    fn greenhouse_html_to_text(bencher: divan::Bencher) {
        let body: serde_json::Value = serde_json::from_str(GREENHOUSE).unwrap();
        let content = body["jobs"][0]["content"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        bencher.bench_local(|| divan::black_box(html_to_text(divan::black_box(&content))));
    }
}
