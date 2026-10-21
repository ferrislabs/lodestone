use std::time::Duration;

use serde_json::{Map, Value, json};
use tokio::time::Instant;
use url::Url;

use super::*;

const ENV: &str = "window.env = {\"PUBLIC_ALGOLIA_API_KEY_CLIENT\":\"search-only-key\",\"PUBLIC_WTTJ_ALERTS_API_URL\":\"https://alerts.example\",\"PUBLIC_ALGOLIA_APPLICATION_ID\":\"CSEKTEST\",\"PUBLIC_ALGOLIA_ARTICLES_INDEX\":\"wk_cms_articles_production\",\"PUBLIC_ALGOLIA_ORGANIZATIONS_INDEX\":\"wk_cms_organizations_production\",\"PUBLIC_WTTJ_URL\":\"https://www.welcometothejungle.com\"};";

fn settings(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn configured(changes: Value) -> Map<String, Value> {
    let mut map = settings(json!({
        "authorised": true,
        "authorised_at": "2026-10-09T08:00:00+00:00",
        "authorisation_reference": "Courriel du 9 octobre 2026",
        "referer": "https://www.welcometothejungle.com/",
        "requests_per_minute": 10.0,
        "max_pages": 3.0,
    }));
    map.extend(settings(changes));
    map
}

fn response(status: u16, headers: &[(&str, &str)], body: &str) -> FetchResponse {
    FetchResponse {
        status,
        headers: headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        body: body.as_bytes().to_vec(),
        final_url: Url::parse("https://csektest-dsn.algolia.net/1/indexes/x").unwrap(),
        truncated: false,
    }
}

#[test]
fn the_env_script_gives_the_application_the_search_key_and_the_indexes() {
    let env = parse_env(ENV.as_bytes()).unwrap();
    assert_eq!(env.application_id, "CSEKTEST");
    assert_eq!(env.search_key.expose(), "search-only-key");
    assert_eq!(
        env.organizations_index.as_deref(),
        Some("wk_cms_organizations_production")
    );
    assert_eq!(env.jobs_index, None);
    let with_jobs = ENV.replace(
        "\"PUBLIC_WTTJ_URL\"",
        "\"PUBLIC_ALGOLIA_JOBS_INDEX\":\"jobs_live\",\"PUBLIC_WTTJ_URL\"",
    );
    assert_eq!(
        parse_env(with_jobs.as_bytes())
            .unwrap()
            .jobs_index
            .as_deref(),
        Some("jobs_live")
    );
}

#[test]
fn an_env_without_key_or_with_a_strange_application_is_refused() {
    assert!(parse_env(b"window.env = {};").is_none());
    assert!(parse_env(b"<html>maintenance</html>").is_none());
    let odd = ENV.replace("CSEKTEST", "evil.example/x");
    assert!(parse_env(odd.as_bytes()).is_none());
}

#[test]
fn the_search_host_is_the_dsn_of_the_application() {
    assert_eq!(
        search_base("CSEKTEST").unwrap().as_str(),
        "https://csektest-dsn.algolia.net/"
    );
    assert!(search_base("a.b").is_none());
    assert!(search_base("").is_none());
}

#[test]
fn script_urls_are_listed_once_from_the_last() {
    let site = Url::parse("https://www.welcometothejungle.com/").unwrap();
    let html = "<script src=\"/_next/static/chunks/a1.js\"></script><script src=\"/_next/static/chunks/b-2_.js\"></script>[\"/_next/static/chunks/a1.js\"]<link href=\"/_next/static/chunks/style.css\"/>";
    let urls: Vec<String> = script_urls(html, &site)
        .iter()
        .map(Url::to_string)
        .collect();
    assert_eq!(
        urls,
        [
            "https://www.welcometothejungle.com/_next/static/chunks/b-2_.js",
            "https://www.welcometothejungle.com/_next/static/chunks/a1.js",
        ]
    );
}

#[test]
fn the_jobs_index_is_read_from_a_string_literal() {
    assert_eq!(
        jobs_index_in(
            "x=\"wttj_jobs_production_fr_published_at_desc\",y='wttj_jobs_production_fr'",
            "fr"
        )
        .as_deref(),
        Some("wttj_jobs_production_fr")
    );
    assert_eq!(
        jobs_index_in("i=`wttj_jobs_production_`+lang", "fr").as_deref(),
        Some("wttj_jobs_production_fr")
    );
    assert_eq!(
        jobs_index_in("\"wk_cms_organizations_production\" \"jobs\"", "fr"),
        None
    );
}

#[test]
fn the_referer_comes_from_the_settings_and_is_required() {
    let access = access_of(&configured(json!({"jobs_index": "jobs_live"}))).unwrap();
    assert_eq!(access.referer, "https://www.welcometothejungle.com/");
    assert_eq!(access.jobs_index.as_deref(), Some("jobs_live"));
    assert_eq!(access.requests_per_minute, 10);
    assert_eq!(access.max_pages, 3);
    let mut missing = configured(json!({}));
    missing.remove("referer");
    assert_eq!(access_of(&missing), Err(NotReady::MissingReferer));
    assert_eq!(
        access_of(&configured(json!({"referer": "https://example.com/"}))),
        Err(NotReady::InvalidReferer)
    );
    assert_eq!(
        access_of(&configured(json!({"authorised": false}))),
        Err(NotReady::NotAuthorised)
    );
    assert_eq!(
        access_of(&configured(json!({"authorisation_reference": " "}))),
        Err(NotReady::MissingReference)
    );
    assert!(NotReady::MissingReferer.explanation().contains("Referer"));
}

#[test]
fn the_rate_and_pages_settings_are_bounded() {
    let access = access_of(&configured(
        json!({"requests_per_minute": 300, "max_pages": 9}),
    ))
    .unwrap();
    assert_eq!(access.requests_per_minute, MAX_REQUESTS_PER_MINUTE);
    assert_eq!(access.max_pages, MAX_PAGES);
    let access = access_of(&configured(json!({"requests_per_minute": 1}))).unwrap();
    assert_eq!(access.requests_per_minute, 1);
    assert_eq!(spacing(10), Duration::from_secs(6));
    assert_eq!(spacing(30), Duration::from_secs(2));
    assert_eq!(spacing(0), Duration::from_secs(60));
}

#[tokio::test(start_paused = true)]
async fn the_pacer_spaces_requests_per_host() {
    let pacer = Pacer::default();
    let started = Instant::now();
    pacer.wait("www.welcometothejungle.com", 10).await;
    pacer.wait("csektest-dsn.algolia.net", 10).await;
    assert!(started.elapsed() < Duration::from_secs(1));
    pacer.wait("www.welcometothejungle.com", 10).await;
    pacer.wait("www.welcometothejungle.com", 10).await;
    assert!(started.elapsed() >= Duration::from_secs(12));
    assert!(started.elapsed() < Duration::from_secs(13));
}

#[test]
fn a_referer_refusal_is_told_apart_from_a_block() {
    let refused = classify(
        response(
            403,
            &[],
            "{\"message\":\"Method not allowed with this referer\",\"status\":403}",
        ),
        Some("jobs"),
    )
    .unwrap_err();
    assert_eq!(
        refused,
        Failure::RefererRefused("Method not allowed with this referer".to_owned())
    );
    assert!(refused.explanation().contains("Referer"));
    assert_eq!(
        classify(response(403, &[("cf-mitigated", "challenge")], ""), None).unwrap_err(),
        Failure::Blocked {
            status: 403,
            challenge: true
        }
    );
    assert_eq!(
        classify(
            response(503, &[], "<p>Please complete the captcha</p>"),
            None
        )
        .unwrap_err(),
        Failure::Blocked {
            status: 503,
            challenge: true
        }
    );
    assert_eq!(
        classify(response(503, &[], "down"), None).unwrap_err(),
        Failure::Status(503)
    );
    assert_eq!(
        classify(response(429, &[("retry-after", "12")], ""), None).unwrap_err(),
        Failure::RateLimited(Duration::from_secs(12))
    );
    assert_eq!(
        classify(response(404, &[], "{}"), Some("jobs")).unwrap_err(),
        Failure::Missing("jobs".to_owned())
    );
}

#[test]
fn contract_and_remote_values_get_french_labels_the_post_filters_read() {
    assert_eq!(contract_label("full_time"), Some("CDI"));
    assert_eq!(contract_label("temporary"), Some("CDD"));
    assert_eq!(contract_label("freelance"), Some("Freelance"));
    assert_eq!(contract_label("unknown"), None);
    assert_eq!(remote_label("fulltime"), Some("Télétravail total"));
    assert_eq!(remote_label("no"), Some("Pas de télétravail"));
    assert_eq!(remote_label("unknown"), None);
}

#[test]
fn offer_urls_must_stay_on_the_jungle() {
    assert!(is_offer_url(
        &Url::parse("https://www.welcometothejungle.com/fr/companies/acme/jobs/x").unwrap()
    ));
    assert!(!is_offer_url(
        &Url::parse("https://welcometothejungle.com.example.com/x").unwrap()
    ));
    assert!(!is_offer_url(
        &Url::parse("http://www.welcometothejungle.com/x").unwrap()
    ));
}
