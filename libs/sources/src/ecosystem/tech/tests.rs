use std::collections::HashSet;

use super::*;

fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

fn names(detections: &[Detection]) -> Vec<String> {
    detections.iter().map(Detection::display).collect()
}

fn one(headers: &[(String, String)], html: &str) -> Vec<Detection> {
    detect(&[PageEvidence {
        url: "https://acme.fr/",
        headers,
        html,
    }])
}

#[test]
fn the_table_is_large_unique_and_compiles() {
    assert!(
        (60..=140).contains(&TECH_SIGNATURES.len()),
        "{}",
        TECH_SIGNATURES.len()
    );
    let ids: HashSet<&str> = TECH_SIGNATURES
        .iter()
        .map(|signature| signature.id)
        .collect();
    assert_eq!(ids.len(), TECH_SIGNATURES.len());
    assert_eq!(COMPILED.len(), TECH_SIGNATURES.len());
    for signature in TECH_SIGNATURES {
        assert!(!signature.matchers.is_empty(), "{}", signature.id);
        assert!(!signature.name.is_empty());
    }
    for category in TechCategory::ALL {
        assert!(
            TECH_SIGNATURES
                .iter()
                .any(|signature| signature.category == category),
            "{}",
            category.as_str()
        );
        assert!(!category.label().is_empty());
    }
    for wanted in [
        "wordpress",
        "drupal",
        "joomla",
        "shopify",
        "wix",
        "webflow",
        "squarespace",
        "nextjs",
        "nuxt",
        "gatsby",
        "react",
        "vue",
        "angular",
        "laravel",
        "symfony",
        "django",
        "rails",
        "aspnet",
        "cloudflare",
        "fastly",
        "akamai",
        "vercel",
        "netlify",
        "cloudfront",
        "ovhcloud",
        "google_analytics",
        "google_tag_manager",
        "matomo",
        "plausible",
        "hotjar",
        "hubspot",
        "pardot",
        "marketo",
        "intercom",
        "crisp",
        "zendesk",
        "drift",
        "axeptio",
        "didomi",
        "onetrust",
        "stripe",
        "paypal",
        "keycloak",
        "auth0",
        "okta",
    ] {
        assert!(ids.contains(wanted), "{wanted}");
    }
}

#[test]
fn headers_reveal_hosting_servers_and_frameworks_with_versions() {
    let found = one(
        &headers(&[
            ("server", "nginx/1.25.3"),
            ("cf-ray", "8a1b2c3d4e5f-CDG"),
            ("x-powered-by", "PHP/8.2.12"),
            ("x-vercel-id", "cdg1::abc"),
            ("x-amz-cf-id", "abc=="),
            ("x-iplb-instance", "12345"),
        ]),
        "<html></html>",
    );
    assert_eq!(
        names(&found),
        [
            "PHP 8.2.12",
            "Cloudflare",
            "Vercel",
            "AWS CloudFront",
            "OVHcloud",
            "Nginx 1.25.3"
        ]
    );
    let cloudflare = found
        .iter()
        .find(|detection| detection.signature.id == "cloudflare")
        .unwrap();
    assert_eq!(cloudflare.evidence, Evidence::Header("cf-ray"));
    assert_eq!(cloudflare.signature.category, TechCategory::HostingCdn);
    assert_eq!(cloudflare.source, "https://acme.fr/");
}

#[test]
fn the_meta_generator_gives_the_cms_and_its_version() {
    let found = one(
        &[],
        "<html><head><meta content=\"WordPress 6.4.2\" name=\"generator\"></head></html>",
    );
    assert_eq!(names(&found), ["WordPress 6.4.2"]);
    assert_eq!(found[0].evidence, Evidence::Meta("generator"));
    let drupal = one(
        &[],
        "<meta name='Generator' content='Drupal 10 (https://www.drupal.org)'>",
    );
    assert_eq!(names(&drupal), ["Drupal 10"]);
    let unversioned = one(
        &[],
        "<meta name=\"generator\" content=\"Joomla! - Open Source Content Management\">",
    );
    assert_eq!(names(&unversioned), ["Joomla"]);
}

#[test]
fn script_and_link_sources_reveal_tools() {
    let html = r#"<html><head>
        <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.2/dist/css/bootstrap.min.css">
        <script src="/wp-content/themes/acme/app.js?ver=6.4"></script>
        <script async src="https://www.googletagmanager.com/gtag/js?id=G-ABC123"></script>
        <script src="https://js.hs-scripts.com/123456.js"></script>
        <script src="https://client.crisp.chat/l.js"></script>
        <script src="https://static.axept.io/sdk.js"></script>
        <script src="https://js.stripe.com/v3/"></script>
        <script src="/assets/jquery-3.6.0.min.js"></script>
        <script src="https://auth.acme.fr/js/keycloak.js"></script>
        <script src="https://plausible.io/js/script.js"></script>
    </head><body>Acme</body></html>"#;
    let found = one(&[], html);
    assert_eq!(
        names(&found),
        [
            "WordPress",
            "jQuery 3.6.0",
            "Bootstrap 5.3.2",
            "Google Analytics",
            "Plausible",
            "Crisp",
            "HubSpot",
            "Keycloak",
            "Stripe",
            "Axeptio"
        ]
    );
    assert!(
        found
            .iter()
            .filter(|detection| detection.signature.id != "wordpress")
            .all(|detection| detection.evidence == Evidence::Asset
                || detection.evidence == Evidence::Html)
    );
}

#[test]
fn cookie_names_reveal_frameworks_and_tools() {
    let found = one(
        &headers(&[
            ("set-cookie", "laravel_session=abc; path=/; httponly"),
            ("set-cookie", "hubspotutk=123; path=/"),
            ("set-cookie", "_shopify_y=xyz; path=/"),
            ("set-cookie", "PHPSESSID=1; path=/"),
        ]),
        "<html></html>",
    );
    assert_eq!(names(&found), ["Laravel", "PHP", "HubSpot", "Shopify"]);
    assert!(
        found
            .iter()
            .all(|detection| detection.evidence == Evidence::Cookie)
    );
}

#[test]
fn html_markers_reveal_frameworks() {
    let found = one(
        &[],
        "<html><body><div id=\"__next\"></div><script id=\"__NEXT_DATA__\" type=\"application/json\">{}</script><app-root ng-version=\"17.1.0\"></app-root><script>var _paq = window._paq || []; _paq.push(['trackPageView']);</script></body></html>",
    );
    assert_eq!(names(&found), ["Next.js", "Angular 17.1.0", "Matomo"]);
}

#[test]
fn a_version_found_on_a_later_page_completes_the_detection_once() {
    let first = headers(&[("server", "nginx")]);
    let second = headers(&[("server", "nginx/1.24.0")]);
    let found = detect(&[
        PageEvidence {
            url: "https://acme.fr/",
            headers: &first,
            html: "<p>a</p>",
        },
        PageEvidence {
            url: "https://acme.fr/contact",
            headers: &second,
            html: "<p>b</p>",
        },
    ]);
    assert_eq!(names(&found), ["Nginx 1.24.0"]);
    assert_eq!(found[0].source, "https://acme.fr/contact");
}

#[test]
fn plain_pages_match_nothing() {
    let found = one(
        &headers(&[("content-type", "text/html"), ("server", "")]),
        "<html><head><title>Acme</title></head><body><p>Logiciels de gestion pour les communes, la react ion et la vue d'ensemble.</p></body></html>",
    );
    assert!(found.is_empty(), "{:?}", names(&found));
}

#[test]
fn a_header_or_generator_is_explained_when_a_signature_names_it() {
    assert!(explains(Evidence::Header("server"), "cloudflare"));
    assert!(explains(Evidence::Header("server"), "nginx/1.25.3"));
    assert!(!explains(Evidence::Header("server"), "AcmeServer/1.0"));
    assert!(explains(Evidence::Header("x-powered-by"), "Next.js"));
    assert!(explains(Evidence::Meta("generator"), "WordPress 6.4"));
    assert!(!explains(Evidence::Meta("generator"), "Maison 2.0"));
    assert!(!explains(Evidence::Header("cf-ray"), "abc"));
}
