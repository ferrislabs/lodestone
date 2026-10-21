use std::{sync::Arc, time::Duration};

use chrono::TimeZone;
use lodestone_core::{EcosystemModule, InspectionSettings};

use super::*;
use crate::ecosystem::fake::{FakeFetcher, domain, html, now, target, xml};

const INDEX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap><loc>https://acme.fr/page-sitemap.xml</loc><lastmod>2026-01-02</lastmod></sitemap>
  <sitemap><loc>https://acme.fr/post-sitemap.xml</loc><lastmod>2026-10-01T08:00:00+02:00</lastmod></sitemap>
  <sitemap><loc>https://acme.fr/category-sitemap.xml</loc></sitemap>
  <sitemap><loc>https://acme.fr/product-sitemap.xml</loc><lastmod>2026-09-01</lastmod></sitemap>
</sitemapindex>"#;

const POSTS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:news="http://www.google.com/schemas/sitemap-news/0.9">
  <url><loc>https://acme.fr/blog/</loc><lastmod>2026-10-02</lastmod></url>
  <url><loc>https://acme.fr/blog/nouvelle-version</loc><lastmod>2026-10-01T10:00:00+00:00</lastmod></url>
  <url><loc><![CDATA[https://acme.fr/blog/salon?x=1&amp;y=2]]></loc><lastmod>2026-09-15</lastmod>
    <news:news><news:title>Acme au salon des maires</news:title></news:news></url>
  <url><loc>https://acme.fr/blog/ancien</loc><lastmod>2025-01-01</lastmod></url>
  <url><loc>https://acme.fr/blog/sans-date</loc></url>
  <url><loc>https://acme.fr/blog/futur</loc><lastmod>2027-01-01</lastmod></url>
</urlset>"#;

#[test]
fn a_sitemap_index_lists_its_children_and_prefers_posts() {
    let Sitemap::Index(children) = parse_sitemap(INDEX) else {
        panic!("not an index");
    };
    assert_eq!(children.len(), 4);
    assert_eq!(
        children[1].lastmod,
        Some(Utc.with_ymd_and_hms(2026, 10, 1, 6, 0, 0).unwrap())
    );
    let chosen: Vec<&str> = child_sitemaps(&children)
        .into_iter()
        .map(|entry| entry.url.as_str())
        .collect();
    assert_eq!(
        chosen,
        [
            "https://acme.fr/post-sitemap.xml",
            "https://acme.fr/product-sitemap.xml",
            "https://acme.fr/page-sitemap.xml"
        ]
    );
}

#[test]
fn recent_urls_are_those_of_the_last_ninety_days() {
    let Sitemap::Urls(entries) = parse_sitemap(POSTS) else {
        panic!("not an url set");
    };
    assert_eq!(entries.len(), 6);
    assert_eq!(entries[2].url, "https://acme.fr/blog/salon?x=1&y=2");
    assert_eq!(
        entries[2].title.as_deref(),
        Some("Acme au salon des maires")
    );
    let recent: Vec<&str> = recent_entries(&entries, now())
        .into_iter()
        .map(|entry| entry.url.as_str())
        .collect();
    assert_eq!(
        recent,
        [
            "https://acme.fr/blog/",
            "https://acme.fr/blog/nouvelle-version",
            "https://acme.fr/blog/salon?x=1&y=2"
        ]
    );
}

#[test]
fn robots_lists_sitemaps() {
    let robots = "User-agent: *\nDisallow: /admin\nSitemap: https://acme.fr/sitemap_index.xml\nsitemap:https://acme.fr/news.xml\n";
    assert_eq!(
        robots_sitemaps(robots),
        [
            "https://acme.fr/sitemap_index.xml",
            "https://acme.fr/news.xml"
        ]
    );
}

#[test]
fn dates_are_read_in_french_english_and_numeric_forms() {
    let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d);
    assert_eq!(date_in("Publié le 3 octobre 2026"), day(2026, 10, 3));
    assert_eq!(date_in("1er février 2026"), day(2026, 2, 1));
    assert_eq!(date_in("October 3, 2026"), day(2026, 10, 3));
    assert_eq!(date_in("12 March 2026"), day(2026, 3, 12));
    assert_eq!(date_in("Mis à jour : 2026-09-30"), day(2026, 9, 30));
    assert_eq!(date_in("05/09/2026"), day(2026, 9, 5));
    assert_eq!(date_in("Depuis 2004"), None);
    assert_eq!(date_in("Le 3 truc 2026"), None);
}

const HOME: &str = r#"<html><head><title>Acme</title></head><body><h1>Acme, logiciels pour les collectivités</h1>
<p>Nous construisons des outils de gestion depuis 2004.</p>
<nav><a href="/solutions">Solutions</a><a href="/actualites">Actualités</a><a href="/blog/nouvelle-version">Nouvelle version</a></nav></body></html>"#;

const NEWS: &str = r#"<html><head><title>Actualités</title></head><body><h1>Actualités d'Acme</h1>
<article><a href="/actualites/salon-des-maires"><h2>Acme au salon des maires</h2></a><time datetime="2026-09-15T09:00:00+02:00">15 septembre 2026</time></article>
<article><h3><a href="/actualites/nouvelle-version">Sortie de la version 5</a></h3><span>Publié le 2 octobre 2026</span></article>
<article><a href="/actualites/recrutement">Nous recrutons</a></article>
</body></html>"#;

#[test]
fn blog_pages_are_found_from_home_links_and_their_items_read() {
    let base = Url::parse("https://acme.fr/").unwrap();
    let links: Vec<String> = blog_links(HOME, &base, &domain())
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(links, ["https://acme.fr/actualites"]);
    let page = Url::parse("https://acme.fr/actualites").unwrap();
    let items = blog_items(NEWS, &page, &domain());
    let summary: Vec<(&str, Option<&str>, Option<NaiveDate>)> = items
        .iter()
        .map(|item| (item.url.path(), item.title.as_deref(), item.date))
        .collect();
    assert_eq!(
        summary,
        [
            (
                "/actualites/salon-des-maires",
                Some("Acme au salon des maires"),
                NaiveDate::from_ymd_opt(2026, 9, 15)
            ),
            (
                "/actualites/nouvelle-version",
                Some("Sortie de la version 5"),
                NaiveDate::from_ymd_opt(2026, 10, 2)
            ),
            ("/actualites/recrutement", Some("Nous recrutons"), None),
        ]
    );
}

#[test]
fn a_listing_without_articles_falls_back_on_its_child_links() {
    let page = Url::parse("https://acme.fr/blog").unwrap();
    let listing = r#"<ul><li><a href="/blog/premier-article">Premier article de blog</a></li><li><a href="/contact">Contact</a></li><li><a href="/blog/">Blog</a></li></ul>"#;
    let items = blog_items(listing, &page, &domain());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].url.path(), "/blog/premier-article");
}

fn article(title: &str) -> String {
    format!(
        "<html><head><title>{title}</title></head><body><h1>{title}</h1><p>Un article complet sur la vie de l'entreprise et de ses clients, numéro {}.</p></body></html>",
        title.len()
    )
}

fn module(fetcher: FakeFetcher) -> RecentModule<FakeFetcher> {
    RecentModule::new(
        Arc::new(fetcher),
        WebsiteSettings {
            max_rate_limit_wait: Duration::ZERO,
            origin: None,
        },
    )
}

#[tokio::test]
async fn the_module_merges_sitemap_and_news_items_and_confirms_them() {
    let fetcher = FakeFetcher::new(vec![
        (
            "https://acme.fr/robots.txt",
            Ok((
                200,
                vec![("content-type", "text/plain")],
                "User-agent: *\nSitemap: https://acme.fr/sitemap_index.xml\nSitemap: https://elsewhere.example/sitemap.xml\n".to_owned(),
            )),
        ),
        ("https://acme.fr/sitemap_index.xml", xml(INDEX)),
        ("https://acme.fr/post-sitemap.xml", xml(POSTS)),
        ("https://acme.fr/", html(HOME)),
        ("https://acme.fr/actualites", html(NEWS)),
        (
            "https://acme.fr/actualites/salon-des-maires",
            html(&article("Acme au salon des maires")),
        ),
        (
            "https://acme.fr/actualites/nouvelle-version",
            html(&article("Sortie de la version 5")),
        ),
        (
            "https://acme.fr/actualites/recrutement",
            html("<html><head><title>Page introuvable</title></head><body><p>Erreur 404</p></body></html>"),
        ),
        (
            "https://acme.fr/blog/nouvelle-version",
            html(&article("Nouvelle version d'Acme")),
        ),
        ("https://acme.fr/blog/", html(&article("Le blog d'Acme"))),
    ]);
    let module = module(fetcher);
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
                "latest_news",
                "Sortie de la version 5 (2026-10-02)",
                "https://acme.fr/actualites/nouvelle-version"
            ),
            (
                "freshness",
                "mis à jour il y a 6 jours (2026-10-02)",
                "https://acme.fr/sitemap_index.xml"
            ),
            ("recent_pages", "3", "https://acme.fr/sitemap_index.xml"),
            (
                "blog_page",
                "https://acme.fr/actualites",
                "https://acme.fr/actualites"
            ),
            (
                "recent_item",
                "Sortie de la version 5",
                "https://acme.fr/actualites/nouvelle-version"
            ),
            (
                "recent_item",
                "Nouvelle version d'Acme",
                "https://acme.fr/blog/nouvelle-version"
            ),
            (
                "recent_item",
                "Acme au salon des maires",
                "https://acme.fr/actualites/salon-des-maires"
            ),
        ]
    );
    assert!(
        findings
            .iter()
            .all(|found| found.module == EcosystemModule::RecentActivity)
    );
    assert_eq!(findings[1].details["days"], 6);
    assert_eq!(findings[4].details["date"], "2026-10-02");
    assert_eq!(
        findings[4].document_line(),
        "- Publication (2026-10-02): Sortie de la version 5"
    );
    assert_eq!(
        findings[0].document_line(),
        "- Dernière actualité: Sortie de la version 5 (2026-10-02)"
    );
}

#[tokio::test]
async fn a_site_without_sitemap_nor_news_gives_nothing() {
    let module = module(FakeFetcher::new(vec![(
        "https://acme.fr/",
        html(
            "<html><head><title>Acme</title></head><body><h1>Acme, logiciels pour les collectivités</h1><p>Depuis 2004.</p></body></html>",
        ),
    )]));
    let settings = InspectionSettings::default();
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    assert!(findings.is_empty());
}

#[tokio::test]
async fn a_rate_limited_sitemap_pauses_the_module() {
    let module = module(FakeFetcher::new(vec![(
        "https://acme.fr/robots.txt",
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

#[tokio::test]
async fn a_rate_limit_while_confirming_items_keeps_the_confirmed_ones() {
    let posts = "<urlset><url><loc>https://acme.fr/blog/a</loc><lastmod>2026-10-05</lastmod></url><url><loc>https://acme.fr/blog/b</loc><lastmod>2026-10-01</lastmod></url></urlset>";
    let module = module(FakeFetcher::new(vec![
        ("https://acme.fr/sitemap.xml", xml(posts)),
        ("https://acme.fr/blog/a", html(&article("Premier article"))),
        (
            "https://acme.fr/blog/b",
            Err(FetchError::RateLimited {
                retry_after: Duration::from_secs(30),
            }),
        ),
    ]));
    let settings = InspectionSettings::default();
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    let items: Vec<&str> = findings
        .iter()
        .filter(|found| found.kind == "recent_item")
        .map(|found| found.value.as_str())
        .collect();
    assert_eq!(items, ["Premier article"]);
}

#[test]
fn sitemap_publications_are_preferred_over_other_pages() {
    let entry = |url: &str| SitemapEntry {
        url: url.to_owned(),
        lastmod: None,
        title: None,
    };
    let entries = [
        entry("https://acme.fr/produits/gestion"),
        entry("https://acme.fr/en/news/partenariat"),
        entry("https://acme.fr/blog/"),
        entry("https://acme.fr/actualites/salon"),
    ];
    let recent: Vec<&SitemapEntry> = entries.iter().collect();
    let chosen: Vec<&str> = publications(&recent)
        .into_iter()
        .map(|entry| entry.url.as_str())
        .collect();
    assert_eq!(
        chosen,
        [
            "https://acme.fr/en/news/partenariat",
            "https://acme.fr/actualites/salon"
        ]
    );
    let pages = [entry("https://acme.fr/produits/gestion")];
    let only_pages: Vec<&SitemapEntry> = pages.iter().collect();
    assert_eq!(publications(&only_pages).len(), 1);
}

#[test]
fn the_site_name_is_dropped_from_page_titles() {
    assert_eq!(
        without_site_suffix("Startup Program for NVIDIA Inception Members | Scaleway"),
        "Startup Program for NVIDIA Inception Members"
    );
    assert_eq!(
        without_site_suffix("Acme - Sortie de la version 5 - Acme"),
        "Acme - Sortie de la version 5"
    );
    assert_eq!(without_site_suffix("Accueil | Acme"), "Accueil | Acme");
    assert_eq!(
        without_site_suffix("Sortie de la version 5"),
        "Sortie de la version 5"
    );
}
