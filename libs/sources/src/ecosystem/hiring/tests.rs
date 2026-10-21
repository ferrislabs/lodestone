use std::{sync::Arc, time::Duration};

use chrono::TimeZone;
use lodestone_core::{EcosystemModule, InspectionSettings};
use serde_json::json;

use super::*;
use crate::{
    ats::{
        BoardJobs,
        discover::{Board, Vendor},
        jobs::parse_board,
    },
    ecosystem::fake::{FakeFetcher, domain, html, now, target, with_previous},
};

const HOME: &str = "<html><head><title>Acme</title></head><body><h1>Acme, logiciels pour les collectivités</h1><p>Nous construisons des outils de gestion depuis 2004.</p></body></html>";

fn offer(title: &str, function: JobFunction) -> Offer {
    Offer {
        title: title.to_owned(),
        url: format!("https://jobs.acme.fr/{}", title.len()),
        location: None,
        team: None,
        ats: None,
        function,
        contracts: Vec::new(),
        remote: false,
        posted_at: None,
        keywords: Vec::new(),
    }
}

#[test]
fn titles_are_classified_by_function_in_french_and_english() {
    let cases = [
        ("Ingénieur DevOps", JobFunction::Engineering),
        ("Développeur·se Rust senior", JobFunction::Engineering),
        ("Senior Backend Engineer", JobFunction::Engineering),
        ("Data Engineer", JobFunction::Data),
        ("Analyste BI", JobFunction::Data),
        ("Sales Engineer", JobFunction::Sales),
        ("Ingénieur commercial grands comptes", JobFunction::Sales),
        ("Account Executive Mid-Market", JobFunction::Sales),
        ("Growth Marketing Manager", JobFunction::Marketing),
        ("Chargé(e) de communication", JobFunction::Marketing),
        ("Product Owner", JobFunction::Product),
        ("UX Designer", JobFunction::Product),
        ("Customer Success Manager", JobFunction::Support),
        ("Technicien support N2", JobFunction::Support),
        ("Comptable fournisseurs", JobFunction::Ops),
        ("Office Manager", JobFunction::Ops),
        ("Chargé de recrutement", JobFunction::Ops),
        ("Chef de chantier", JobFunction::Other),
        (
            "Senior Software Engineer - IT & Sales Tooling",
            JobFunction::Engineering,
        ),
        ("Customer Success Manager - Italy", JobFunction::Support),
        ("IT Workplace - Internship", JobFunction::Engineering),
        (
            "Stage (6 mois) - Assistant marketing",
            JobFunction::Marketing,
        ),
    ];
    for (title, expected) in cases {
        assert_eq!(classify(title, None), expected, "{title}");
    }
    assert_eq!(
        classify("Lead", Some("Engineering")),
        JobFunction::Engineering
    );
    assert_eq!(JobFunction::ALL.len(), FUNCTION_KEYWORDS.len() + 1);
}

#[test]
fn contracts_and_remote_are_read_from_the_offer_texts() {
    assert_eq!(contract_types(&["CDI", "Développeur"]), ["CDI"]);
    assert_eq!(contract_types(&["FullTime", "Engineer"]), ["Temps plein"]);
    assert_eq!(
        contract_types(&["", "Stage - Assistant marketing (6 mois)"]),
        ["Stage"]
    );
    assert_eq!(
        contract_types(&["", "Alternance : chargé de communication"]),
        ["Alternance"]
    );
    assert!(contract_types(&["", "Ingénieur"]).is_empty());
    assert!(remote_mentioned(&["Développeur (Full Remote)", "France"]));
    assert!(remote_mentioned(&["Développeur", "Lyon ou télétravail"]));
    assert!(!remote_mentioned(&["Développeur", "Lyon"]));
}

#[test]
fn the_summary_counts_functions_and_compares_with_the_previous_run() {
    let offers = vec![
        offer("Backend Engineer", JobFunction::Engineering),
        offer("Frontend Engineer", JobFunction::Engineering),
        offer("Account Executive", JobFunction::Sales),
    ];
    let previous = [EcosystemFinding::new(
        EcosystemModule::HiringActivity,
        SUMMARY_KIND,
        "Recrutement",
        "1 offre ouverte",
        "https://jobs.acme.fr",
        now(),
    )
    .with_details(json!({
        "count": 1,
        "history": [{"at": "2026-10-01", "count": 1}],
    }))];
    let summary = summarize(&offers, &previous, now());
    assert_eq!(summary.delta(), Some(2));
    assert_eq!(
        summary.sentence(),
        "3 offres ouvertes : 2 engineering, 1 sales (+2 depuis le dernier passage)"
    );
    assert_eq!(
        summary.history,
        [
            HistoryPoint {
                at: "2026-10-01".to_owned(),
                count: 1
            },
            HistoryPoint {
                at: "2026-10-08".to_owned(),
                count: 3
            }
        ]
    );
    let first = summarize(&offers[..1], &[], now());
    assert_eq!(first.sentence(), "1 offre ouverte : 1 engineering");
    let fewer = summarize(&[], &previous, now());
    assert_eq!(
        fewer.sentence(),
        "Aucune offre ouverte (-1 depuis le dernier passage)"
    );
}

#[test]
fn the_history_keeps_the_last_points_only() {
    let points: Vec<_> = (1..=20)
        .map(|day| json!({"at": format!("2026-09-{day:02}"), "count": day}))
        .collect();
    let previous = [EcosystemFinding::new(
        EcosystemModule::HiringActivity,
        SUMMARY_KIND,
        "Recrutement",
        "20 offres ouvertes",
        "https://jobs.acme.fr",
        now(),
    )
    .with_details(json!({"count": 20, "history": points}))];
    let summary = summarize(&[], &previous, now());
    assert_eq!(summary.history.len(), MAX_HISTORY);
    assert_eq!(summary.history.last().unwrap().at, "2026-10-08");
}

fn board(count: usize) -> String {
    let jobs: Vec<_> = (0..count)
        .map(|index| {
            json!({
                "id": index,
                "title": if index == 0 { "Ingénieur IAM".to_owned() } else { format!("Backend Engineer {index}") },
                "absolute_url": format!("https://boards.greenhouse.io/acme/jobs/{index}"),
                "content": if index == 0 { "Keycloak et SSO" } else { "Rust" },
                "location": {"name": if index % 2 == 0 { "Lyon" } else { "Paris (Remote)" }},
                "first_published": format!("2026-09-{:02}T10:00:00Z", (index % 28) + 1),
            })
        })
        .collect();
    json!({"jobs": jobs}).to_string()
}

fn scripted(jobs: usize) -> FakeFetcher {
    FakeFetcher::new(vec![
        (
            "https://acme.fr/",
            html(&format!(
                "{HOME}<a href=\"https://boards.greenhouse.io/acme\">Carrières</a>"
            )),
        ),
        (
            "https://boards-api.greenhouse.io/v1/boards/acme/jobs?content=true",
            Ok((200, vec![("content-type", "application/json")], board(jobs))),
        ),
    ])
}

#[tokio::test]
async fn the_hiring_module_lists_every_offer_and_highlights_keywords() {
    let module = HiringModule::new(AtsScanner::new(Arc::new(scripted(3)), 2, Duration::ZERO));
    let settings = InspectionSettings {
        highlight_keywords: vec!["keycloak".to_owned()],
        ..InspectionSettings::default()
    };
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    let pairs: Vec<(&str, &str)> = findings
        .iter()
        .map(|found| (found.kind.as_str(), found.value.as_str()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("hiring_summary", "3 offres ouvertes : 3 engineering"),
            ("ats_board", "greenhouse (acme)"),
            ("job_offer", "Ingénieur IAM"),
            ("job_offer", "Backend Engineer 2"),
            ("job_offer", "Backend Engineer 1"),
        ]
    );
    let summary = &findings[0].details;
    assert_eq!(summary["count"], 3);
    assert_eq!(summary["remote"], 1);
    assert_eq!(summary["locations"][0], json!({"name": "Lyon", "count": 2}));
    assert_eq!(
        findings[0].source_url,
        "https://boards-api.greenhouse.io/v1/boards/acme/jobs?content=true"
    );
    assert_eq!(findings[1].details["open_positions"], 3);
    assert_eq!(findings[2].details["highlighted"], true);
    assert_eq!(findings[2].details["keywords"], json!(["keycloak"]));
    assert_eq!(findings[3].details["function"], "engineering");
    assert_eq!(findings[3].details["posted_at"], "2026-09-03");
    assert_eq!(findings[4].details["remote"], true);
    assert_eq!(
        findings[2].source_url,
        "https://boards.greenhouse.io/acme/jobs/0"
    );
}

#[tokio::test]
async fn the_hiring_module_stores_fifty_offers_at_most_and_a_trend() {
    let module = HiringModule::new(AtsScanner::new(Arc::new(scripted(60)), 2, Duration::ZERO));
    let settings = InspectionSettings::default();
    let domain = domain();
    let previous = [EcosystemFinding::new(
        EcosystemModule::HiringActivity,
        SUMMARY_KIND,
        "Recrutement",
        "56 offres ouvertes",
        "https://boards-api.greenhouse.io/v1/boards/acme/jobs?content=true",
        Utc.with_ymd_and_hms(2026, 10, 5, 9, 0, 0).unwrap(),
    )
    .with_details(json!({"count": 56, "history": [{"at": "2026-10-05", "count": 56}]}))];
    let findings = module
        .inspect(&with_previous(&domain, &settings, None, &previous))
        .await
        .unwrap();
    let offers = findings
        .iter()
        .filter(|found| found.kind == "job_offer")
        .count();
    assert_eq!(offers, MAX_JOB_OFFERS);
    assert_eq!(findings[0].details["count"], 60);
    assert_eq!(findings[0].details["delta"], 4);
    assert!(
        findings[0]
            .value
            .ends_with("(+4 depuis le dernier passage)")
    );
    assert_eq!(findings[0].details["history"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn offers_published_on_the_careers_page_count_without_a_board() {
    let careers = r#"<html><head><title>Carrières</title>
<script type="application/ld+json">{"@context":"https://schema.org","@graph":[
 {"@type":"JobPosting","title":"Développeur Rust (CDI)","url":"https://acme.fr/carrieres/dev-rust","datePosted":"2026-09-20","employmentType":"FULL_TIME","jobLocation":{"@type":"Place","address":{"addressLocality":"Nantes","addressCountry":"FR"}}},
 {"@type":"JobPosting","title":"Commercial terrain","datePosted":"2026-09-25","jobLocationType":"TELECOMMUTE"}
]}</script></head>
<body><h1>Rejoignez Acme</h1><p>Nous recrutons des personnes passionnées pour construire nos logiciels.</p></body></html>"#;
    let fetcher = FakeFetcher::new(vec![
        (
            "https://acme.fr/",
            html(&format!("{HOME}<a href=\"/carrieres\">Carrières</a>")),
        ),
        ("https://acme.fr/carrieres", html(careers)),
    ]);
    let module = HiringModule::new(AtsScanner::new(Arc::new(fetcher), 2, Duration::ZERO));
    let settings = InspectionSettings::default();
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    let pairs: Vec<(&str, &str, &str)> = findings
        .iter()
        .map(|found| {
            (
                found.kind.as_str(),
                found.value.as_str(),
                found.source_url.as_str(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        [
            (
                "hiring_summary",
                "2 offres ouvertes : 1 engineering, 1 sales",
                "https://acme.fr/carrieres"
            ),
            (
                "careers_page",
                "https://acme.fr/carrieres",
                "https://acme.fr/carrieres"
            ),
            (
                "job_offer",
                "Commercial terrain",
                "https://acme.fr/carrieres"
            ),
            (
                "job_offer",
                "Développeur Rust (CDI)",
                "https://acme.fr/carrieres/dev-rust"
            ),
        ]
    );
    assert_eq!(findings[2].details["remote"], true);
    assert_eq!(findings[3].details["location"], "Nantes, FR");
    assert_eq!(
        findings[3].details["contracts"],
        json!(["CDI", "Temps plein"])
    );
}

#[tokio::test]
async fn a_careers_page_without_readable_offers_gives_no_count() {
    let fetcher = FakeFetcher::new(vec![
        (
            "https://acme.fr/",
            html(&format!("{HOME}<a href=\"/jobs\">Jobs</a>")),
        ),
        (
            "https://acme.fr/jobs",
            html(
                "<html><head><title>Jobs</title></head><body><h1>Rejoignez-nous</h1><p>Nos offres sont publiées sur notre page partenaire.</p></body></html>",
            ),
        ),
    ]);
    let module = HiringModule::new(AtsScanner::new(Arc::new(fetcher), 2, Duration::ZERO));
    let settings = InspectionSettings::default();
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    let kinds: Vec<&str> = findings.iter().map(|found| found.kind.as_str()).collect();
    assert_eq!(kinds, ["careers_page"]);
}

#[tokio::test]
async fn a_site_without_careers_page_gives_no_hiring_finding() {
    let module = HiringModule::new(AtsScanner::new(
        Arc::new(FakeFetcher::new(vec![("https://acme.fr/", html(HOME))])),
        2,
        Duration::ZERO,
    ));
    let settings = InspectionSettings::default();
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    assert!(findings.is_empty());
}

#[test]
fn lever_and_ashby_offers_carry_contract_remote_and_date() {
    let lever = Board {
        vendor: Vendor::Lever,
        slug: "acme".to_owned(),
    };
    let body = json!([{
        "id": "a",
        "text": "Backend Engineer",
        "hostedUrl": "https://jobs.lever.co/acme/a",
        "categories": {"commitment": "Full-time", "location": "Paris"},
        "workplaceType": "remote",
        "createdAt": 1_790_000_000_000_i64
    }]);
    let candidates = parse_board(&lever, &body, &domain(), now());
    assert_eq!(candidates[0].details["contract"], "Full-time");
    assert_eq!(candidates[0].details["remote"], true);
    assert!(candidates[0].details["posted_at"].is_string());
    let ashby = Board {
        vendor: Vendor::Ashby,
        slug: "acme".to_owned(),
    };
    let body = json!({"jobs": [{
        "id": "b",
        "title": "Data Analyst",
        "jobUrl": "https://jobs.ashbyhq.com/acme/b",
        "employmentType": "Intern",
        "isRemote": false,
        "publishedAt": "2026-09-01T00:00:00Z"
    }]});
    let candidates = parse_board(&ashby, &body, &domain(), now());
    assert_eq!(candidates[0].details["contract"], "Intern");
    assert_eq!(candidates[0].details["remote"], false);
    assert_eq!(
        candidates[0].details["posted_at"],
        "2026-09-01T00:00:00+00:00"
    );
    let scan = DomainScan {
        boards: vec![BoardJobs {
            board: ashby,
            candidates,
        }],
        ..DomainScan::default()
    };
    let offers = offers_of(&scan, &[]);
    assert_eq!(offers[0].function, JobFunction::Data);
    assert_eq!(offers[0].contracts, ["Stage"]);
    assert!(!offers[0].remote);
}
