use lodestone_core::{FetchError, InspectionSettings, definition::Fingerprint};
use serde_json::{Value, from_value, json};

use super::*;
use crate::ecosystem::fake::{FakeFetcher, Reply, domain, html, target};

const HOME: &str = "<html><head><title>Acme</title><meta name=\"generator\" content=\"WordPress 6.4\"></head><body><h1>Acme, logiciels pour les collectivités</h1><p>Nous construisons des outils de gestion depuis 2004.</p></body></html>";

fn fingerprint(value: Value) -> Fingerprint {
    from_value(value).unwrap()
}

fn website(fetcher: FakeFetcher) -> WebsiteModule<FakeFetcher> {
    WebsiteModule::new(
        Arc::new(fetcher),
        WebsiteSettings {
            max_rate_limit_wait: Duration::ZERO,
            origin: None,
        },
    )
}

#[tokio::test]
async fn the_website_module_reads_the_home_page_and_the_profile_fingerprints() {
    let module = website(FakeFetcher::new(vec![
        ("https://acme.fr/", html(HOME)),
        (
            "https://auth.acme.fr/realms/master",
            Ok((
                200,
                vec![("x-keycloak-version", "21.1.2")],
                "{\"realm\":\"master\"}".to_owned(),
            )),
        ),
    ]));
    let settings = InspectionSettings {
        fingerprints: vec![fingerprint(json!({
            "id": "keycloak-realm",
            "path": "/realms/master",
            "product": "keycloak",
            "matchers": [{"kind": "status", "equals": 200}],
            "version": {"from": "header", "name": "x-keycloak-version", "pattern": "^(\\d+\\.\\d+)"},
            "debt_markers": [{"id": "old_keycloak", "rule": {"kind": "version_below", "version": "22.0"}}]
        }))],
        excluded_products: Vec::new(),
        extra_subdomains: vec!["auth".to_owned()],
        highlight_keywords: Vec::new(),
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
            ("website", "https://acme.fr/"),
            ("product", "keycloak 21.1"),
            ("debt_marker", "old_keycloak"),
            ("technology", "WordPress 6.4"),
            ("technology", "Nginx"),
        ]
    );
    assert_eq!(findings[1].source_url, "https://auth.acme.fr/realms/master");
    assert_eq!(findings[3].label, "CMS");
    assert_eq!(findings[3].key, "technology:wordpress");
    assert_eq!(findings[3].details["category"], "cms");
    assert_eq!(findings[3].details["version"], "6.4");
    assert_eq!(findings[3].details["evidence"], "balise meta generator");
    assert!(
        findings
            .iter()
            .all(|found| found.module == EcosystemModule::WebsiteStack)
    );
}

#[tokio::test]
async fn excluded_products_and_unconfirmed_pages_leave_no_finding() {
    let module = website(FakeFetcher::new(vec![(
        "https://acme.fr/realms/master",
        Ok((200, Vec::new(), "ok".to_owned())),
    )]));
    let settings = InspectionSettings {
        fingerprints: vec![fingerprint(json!({
            "id": "ferriskey",
            "path": "/realms/master",
            "product": "ferriskey",
            "matchers": [{"kind": "status", "equals": 200}]
        }))],
        excluded_products: vec!["ferriskey".to_owned()],
        extra_subdomains: Vec::new(),
        highlight_keywords: Vec::new(),
    };
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    assert!(findings.is_empty());
}

#[tokio::test]
async fn a_rate_limited_home_page_stops_the_module() {
    let module = website(FakeFetcher::new(vec![(
        "https://acme.fr/",
        Err(FetchError::RateLimited {
            retry_after: Duration::from_secs(30),
        }),
    )]));
    let settings = InspectionSettings::default();
    let domain = domain();
    let error = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap_err();
    assert!(matches!(error, SourceError::RateLimited { .. }));
}

fn page(body: &str, headers: Vec<(&'static str, &'static str)>) -> Reply {
    Ok((200, headers, body.to_owned()))
}

const RICH_HOME: &str = r#"<html><head><title>Acme</title>
<script src="https://www.googletagmanager.com/gtm.js?id=GTM-ABC"></script>
<script src="https://client.crisp.chat/l.js"></script>
</head><body><h1>Acme, logiciels pour les collectivités</h1>
<p>Nous construisons des outils de gestion depuis 2004 pour les communes.</p>
<a href="/produits">Produits</a><a href="/produits/gestion">Gestion</a><a href="/contact">Contact</a>
<a href="/brochure.pdf">Brochure</a><a href="https://twitter.com/acme">Twitter</a></body></html>"#;

const PRODUCTS: &str = r#"<html><head><title>Produits</title>
<script src="https://js.stripe.com/v3/"></script></head>
<body><h1>Nos produits</h1><p>Une suite complète de logiciels de gestion municipale et de facturation.</p></body></html>"#;

const CONTACT: &str = r#"<html><head><title>Contact</title></head>
<body><h1>Nous contacter</h1><p>Écrivez-nous, notre équipe répond sous 48 heures ouvrées.</p></body></html>"#;

#[test]
fn internal_pages_are_two_distinct_sections_of_the_site() {
    let base = Url::parse("https://www.acme.fr/").unwrap();
    let pages = internal_pages(RICH_HOME, &base, &domain());
    let paths: Vec<&str> = pages.iter().map(Url::path).collect();
    assert_eq!(paths, ["/produits", "/contact"]);
}

#[tokio::test]
async fn the_website_module_reads_up_to_two_confirmed_internal_pages() {
    let module = website(FakeFetcher::new(vec![
        (
            "https://acme.fr/",
            page(
                RICH_HOME,
                vec![
                    ("content-type", "text/html"),
                    ("cf-ray", "8a1b-CDG"),
                    ("set-cookie", "PHPSESSID=1; path=/"),
                ],
            ),
        ),
        (
            "https://acme.fr/produits",
            page(PRODUCTS, vec![("content-type", "text/html")]),
        ),
        (
            "https://acme.fr/contact",
            page(
                CONTACT,
                vec![("content-type", "text/html"), ("x-powered-by", "PHP/8.1.2")],
            ),
        ),
    ]));
    let settings = InspectionSettings::default();
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    let technologies: Vec<(&str, &str, &str)> = findings
        .iter()
        .filter(|found| found.kind == "technology")
        .map(|found| {
            (
                found.label.as_str(),
                found.value.as_str(),
                found.source_url.as_str(),
            )
        })
        .collect();
    assert_eq!(
        technologies,
        [
            ("Framework", "PHP 8.1.2", "https://acme.fr/contact"),
            ("Hébergement et CDN", "Cloudflare", "https://acme.fr/"),
            (
                "Mesure d'audience",
                "Google Tag Manager",
                "https://acme.fr/"
            ),
            ("Chat et support", "Crisp", "https://acme.fr/"),
            ("Paiement", "Stripe", "https://acme.fr/produits"),
        ]
    );
    assert!(findings.iter().all(|found| found.kind != "powered_by"));
}

#[tokio::test]
async fn an_unconfirmed_internal_page_brings_no_technology() {
    let fetcher = FakeFetcher::new(vec![
        (
            "https://acme.fr/",
            page(RICH_HOME, vec![("content-type", "text/html")]),
        ),
        (
            "https://acme.fr/produits",
            page(
                "<html><head><title>Page introuvable</title><script src=\"https://js.stripe.com/v3/\"></script></head><body><p>Erreur 404 : page introuvable</p></body></html>",
                vec![("content-type", "text/html")],
            ),
        ),
    ]);
    let module = website(fetcher);
    let settings = InspectionSettings::default();
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    assert!(findings.iter().all(|found| found.value != "Stripe"));
}

#[tokio::test]
async fn an_unknown_server_and_generator_stay_as_raw_findings() {
    let module = website(FakeFetcher::new(vec![(
        "https://acme.fr/",
        page(
            "<html><head><title>Acme</title><meta name=\"generator\" content=\"Maison 2.0\"></head><body><h1>Acme, logiciels pour les collectivités</h1><p>Depuis 2004.</p></body></html>",
            vec![("content-type", "text/html"), ("server", "AcmeServer/1.0")],
        ),
    )]));
    let settings = InspectionSettings::default();
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
            ("website", "https://acme.fr/"),
            ("server", "AcmeServer/1.0"),
            ("generator", "Maison 2.0"),
        ]
    );
}
