use std::{collections::HashMap, sync::Mutex};

use chrono::TimeZone;
use common::CoreError;
use lodestone_core::{
    Profile, ProfileName, ProfileRevision, RevisionNumber, Route, ValidatedProfile,
    definition::{AgentId, SearchAgent},
    route,
};
use serde_json::{from_str, json};
use uuid::Uuid;

use super::{
    discover::{Board, Vendor, careers_links, extract_boards},
    jobs::{html_to_text, parse_board},
    *,
};

const LINEAR: &str = include_str!("../../tests/fixtures/ats/linear-careers.html");
const QONTO: &str = include_str!("../../tests/fixtures/ats/qonto-careers.html");
const MISTRAL: &str = include_str!("../../tests/fixtures/ats/mistral-careers.html");
const CONTENTSQUARE: &str = include_str!("../../tests/fixtures/ats/contentsquare-careers.html");
const ALAN: &str = include_str!("../../tests/fixtures/ats/alan-careers.html");
const GREENHOUSE: &str = include_str!("../../tests/fixtures/ats/greenhouse-gitlab.json");
const LEVER: &str = include_str!("../../tests/fixtures/ats/lever-qonto.json");
const ASHBY: &str = include_str!("../../tests/fixtures/ats/ashby-linear.json");

fn board(vendor: Vendor, slug: &str) -> Board {
    Board {
        vendor,
        slug: slug.to_owned(),
    }
}

fn domain(host: &str) -> NormalizedDomain {
    NormalizedDomain::from_host(host).unwrap()
}

fn now() -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000, 0).unwrap()
}

fn json(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap()
}

fn rules(keywords: &[&str]) -> SearchAgent {
    SearchAgent {
        accept_min_keywords: 1,
        ..SearchAgent::new(AgentId::new("jobs"), "Jobs", &["ats"], keywords, &[])
    }
}

fn iam_rules() -> SearchAgent {
    rules(&["Keycloak", "IAM", "ingénieur IAM", "gestion des identités"])
}

#[test]
fn recorded_careers_pages_expose_their_board() {
    assert_eq!(extract_boards(LINEAR), [board(Vendor::Ashby, "Linear")]);
    assert_eq!(extract_boards(QONTO), [board(Vendor::Lever, "qonto")]);
    assert_eq!(
        extract_boards(MISTRAL),
        [board(Vendor::Ashby, "mistral.ai")]
    );
    assert_eq!(
        extract_boards(CONTENTSQUARE),
        [board(Vendor::Lever, "contentsquare")]
    );
    assert_eq!(extract_boards(ALAN), [board(Vendor::Ashby, "alan")]);
}

#[test]
fn every_documented_link_pattern_is_recognised() {
    let page = r#"
        <a href="https://boards.greenhouse.io/acme">a</a>
        <a href="https://job-boards.greenhouse.io/beta/jobs/12">b</a>
        <a href="https://job-boards.eu.greenhouse.io/gamma">c</a>
        <iframe src="https://boards.greenhouse.io/embed/job_board?for=delta&amp;b=1"></iframe>
        <script src="https://boards-api.greenhouse.io/v1/boards/epsilon/jobs"></script>
        <a href="https://jobs.lever.co/zeta/abc">f</a>
        <a href="https://jobs.eu.lever.co/eta">g</a>
        <a href="https://jobs.ashbyhq.com/Theta">h</a>
        <a href="https://api.ashbyhq.com/posting-api/job-board/iota?x=1">i</a>
    "#;
    assert_eq!(
        extract_boards(page),
        [
            board(Vendor::Greenhouse, "acme"),
            board(Vendor::Greenhouse, "beta"),
            board(Vendor::Greenhouse, "gamma"),
            board(Vendor::Greenhouse, "delta"),
            board(Vendor::Greenhouse, "epsilon"),
            board(Vendor::Lever, "zeta"),
            board(Vendor::LeverEu, "eta"),
            board(Vendor::Ashby, "Theta"),
            board(Vendor::Ashby, "iota"),
        ]
    );
}

#[test]
fn the_greenhouse_embed_script_is_not_a_board() {
    let page = r#"<script src="https://boards.greenhouse.io/embed/job_board/js?for=acme"></script>
        <a href="https://boards.greenhouse.io/embed/job_board?foo=1">x</a>
        <a href="https://boards.greenhouse.io/embed/">y</a>"#;
    assert_eq!(extract_boards(page), [board(Vendor::Greenhouse, "acme")]);
    let bare = r#"<a href="https://boards.greenhouse.io/embed/job_board?foo=1">x</a>"#;
    assert!(extract_boards(bare).is_empty());
}

#[test]
fn escaped_urls_in_inline_json_are_found() {
    let page =
        r#"{"jobUrl":"https:\/\/jobs.lever.co\/acme\/1","x":"https://jobs.ashbyhq.com/beta"}"#;
    assert_eq!(
        extract_boards(page),
        [board(Vendor::Lever, "acme"), board(Vendor::Ashby, "beta")]
    );
}

#[test]
fn a_page_without_a_link_yields_no_board_even_when_it_names_the_company() {
    let page = "<html><body><h1>Acme careers</h1><p>Work at lever acme, greenhouse acme, ashby acme. See jobs.lever.co</p></body></html>";
    assert!(extract_boards(page).is_empty());
}

#[test]
fn board_urls_point_at_the_public_apis() {
    assert_eq!(
        board(Vendor::Greenhouse, "gitlab")
            .api_url()
            .unwrap()
            .as_str(),
        "https://boards-api.greenhouse.io/v1/boards/gitlab/jobs?content=true"
    );
    assert_eq!(
        board(Vendor::Lever, "qonto").api_url().unwrap().as_str(),
        "https://api.lever.co/v0/postings/qonto?mode=json"
    );
    assert_eq!(
        board(Vendor::LeverEu, "eu").api_url().unwrap().as_str(),
        "https://api.eu.lever.co/v0/postings/eu?mode=json"
    );
    assert_eq!(
        board(Vendor::Ashby, "mistral.ai")
            .api_url()
            .unwrap()
            .as_str(),
        "https://api.ashbyhq.com/posting-api/job-board/mistral.ai?includeCompensation=false"
    );
}

#[test]
fn careers_links_stay_on_the_lead_registrable_domain() {
    let base = Url::parse("https://www.acme.fr/").unwrap();
    let html = r##"
        <a href="/fr/carrieres">c</a>
        <a href="https://careers.acme.fr/">d</a>
        <a href="https://other.com/careers">e</a>
        <a href="/blog">f</a>
        <a href="/jobs#top">g</a>
        <a href="mailto:jobs@acme.fr">h</a>
    "##;
    let links: Vec<String> = careers_links(html, &base, &domain("acme.fr"))
        .iter()
        .map(Url::to_string)
        .collect();
    assert_eq!(
        links,
        [
            "https://www.acme.fr/fr/carrieres",
            "https://careers.acme.fr/"
        ]
    );
}

#[test]
fn greenhouse_jobs_parse_with_stripped_html() {
    let candidates = parse_board(
        &board(Vendor::Greenhouse, "gitlab"),
        &json(GREENHOUSE),
        &domain("gitlab.com"),
        now(),
    );
    assert_eq!(candidates.len(), 5);
    let job = candidates
        .iter()
        .find(|candidate| candidate.details["job_id"] == "8614945002")
        .unwrap();
    assert_eq!(
        job.title,
        "Senior Software Engineer (RoR/Go), SSCS: Authentication"
    );
    assert_eq!(job.kind.as_str(), "job_offer");
    assert_eq!(job.source, SourceName::Ats);
    assert_eq!(job.domain, domain("gitlab.com"));
    assert!(
        job.provenance
            .as_str()
            .starts_with("https://job-boards.greenhouse.io/gitlab/jobs/")
    );
    assert_eq!(job.details["ats"], "greenhouse");
    assert_eq!(job.details["board"], "gitlab");
    assert!(job.details["location"].is_string());
    assert!(job.details["team"].is_string());
    assert!(!job.text.contains('<'));
    assert!(!job.text.contains("&lt;"));
    assert!(!job.text.contains("&amp;"));
    assert!(job.text.chars().count() <= 4000);
    assert!(job.observed_at < now());
    let content = format!("{}\n\n{}", job.title, job.text);
    let rules = rules(&["identity and access management"]);
    assert!(matches!(route(&content, &rules), Route::Accept { .. }));
    assert_eq!(route(&content, &iam_rules()), Route::Reject);
}

#[test]
fn lever_jobs_parse_with_lists_and_creation_date() {
    let candidates = parse_board(
        &board(Vendor::Lever, "qonto"),
        &json(LEVER),
        &domain("qonto.com"),
        now(),
    );
    assert_eq!(candidates.len(), 5);
    let job = &candidates[0];
    assert_eq!(job.details["ats"], "lever");
    assert_eq!(job.details["board"], "qonto");
    assert_eq!(
        job.details["job_id"],
        "ee6d81ef-6212-4c6f-b13c-b97fa8cff981"
    );
    assert_eq!(job.details["location"], "Paris");
    assert_eq!(
        job.provenance.as_str(),
        "https://jobs.lever.co/qonto/ee6d81ef-6212-4c6f-b13c-b97fa8cff981"
    );
    assert_eq!(job.observed_at.timestamp_millis(), 1_652_790_736_245);
    assert!(job.text.contains("Our mission and customers"));
    assert!(candidates.iter().all(|c| !c.text.contains('<')));
}

#[test]
fn ashby_jobs_parse_and_unlisted_ones_are_dropped() {
    let mut body = json(ASHBY);
    let candidates = parse_board(
        &board(Vendor::Ashby, "Linear"),
        &body,
        &domain("linear.app"),
        now(),
    );
    assert_eq!(candidates.len(), 5);
    let job = &candidates[0];
    assert_eq!(job.title, "Senior / Staff Fullstack Engineer");
    assert_eq!(job.details["ats"], "ashby");
    assert_eq!(job.details["board"], "Linear");
    assert_eq!(job.details["team"], "Engineering");
    assert_eq!(job.details["location"], "Europe");
    assert_eq!(
        job.observed_at.to_rfc3339(),
        "2021-04-27T20:13:45.158+00:00"
    );
    body["jobs"][0]["isListed"] = Value::Bool(false);
    let listed = parse_board(
        &board(Vendor::Ashby, "Linear"),
        &body,
        &domain("linear.app"),
        now(),
    );
    assert_eq!(listed.len(), 4);
}

#[test]
fn synthetic_keycloak_postings_are_routed_by_keyword() {
    let body = json(
        r#"{"jobs":[
            {"id":"syn-1","title":"Senior IAM Engineer (SYNTHETIC)","jobUrl":"https://jobs.ashbyhq.com/acme/syn-1",
             "descriptionPlain":"Run our self-hosted Keycloak cluster and migrate from Okta.","publishedAt":"2026-09-01T00:00:00.000+00:00","isListed":true},
            {"id":"syn-1","title":"duplicate id","jobUrl":"https://jobs.ashbyhq.com/acme/syn-1","descriptionPlain":"x"},
            {"id":"syn-2","title":"Office manager","jobUrl":"https://jobs.ashbyhq.com/acme/syn-2","descriptionPlain":"Keep the kitchen tidy."},
            {"title":"no id","jobUrl":"https://jobs.ashbyhq.com/acme/syn-3"}
        ]}"#,
    );
    let candidates = parse_board(
        &board(Vendor::Ashby, "acme"),
        &body,
        &domain("acme.io"),
        now(),
    );
    assert_eq!(candidates.len(), 2);
    let rules = iam_rules();
    let routes: Vec<bool> = candidates
        .iter()
        .map(|c| {
            matches!(
                route(&format!("{}\n\n{}", c.title, c.text), &rules),
                Route::Accept { .. }
            )
        })
        .collect();
    assert_eq!(routes, [true, false]);
}

#[test]
fn missing_publication_date_falls_back_to_now_and_text_is_bounded() {
    let long = "a ".repeat(5000);
    let body = json(&format!(
        r#"[{{"id":"1","text":"Role","hostedUrl":"https://jobs.lever.co/acme/1","descriptionPlain":"{long}"}}]"#
    ));
    let candidates = parse_board(
        &board(Vendor::Lever, "acme"),
        &body,
        &domain("acme.io"),
        now(),
    );
    assert_eq!(candidates[0].observed_at, now());
    assert_eq!(candidates[0].text.chars().count(), 4000);
}

#[test]
fn html_is_stripped_and_entities_unescaped() {
    let html = "<style>p{}</style><div><p>Join&nbsp;us &amp; build</p><ul><li>Keycloak</li><li>5&#39;000 users</li></ul><script>x()</script><!-- c --></div><br/>It&rsquo;s fine";
    assert_eq!(
        html_to_text(html),
        "Join us & build\nKeycloak\n5'000 users\n\nIt\u{2019}s fine"
    );
}

type Reply = Result<(u16, &'static str, String), FetchError>;

struct FakeFetcher {
    script: HashMap<String, Reply>,
    requests: Mutex<Vec<(String, FetchPurpose, SourceName)>>,
}

impl FakeFetcher {
    fn new(script: Vec<(&str, Reply)>) -> Self {
        Self {
            script: script
                .into_iter()
                .map(|(url, reply)| (url.to_owned(), reply))
                .collect(),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn urls(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|(url, _, _)| url.clone())
            .collect()
    }

    fn purpose_of(&self, url: &str) -> Option<FetchPurpose> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .find(|(requested, _, _)| requested == url)
            .map(|(_, purpose, _)| *purpose)
    }
}

impl Fetcher for FakeFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let key = request.url.to_string();
        self.requests
            .lock()
            .unwrap()
            .push((key.clone(), request.purpose, request.source));
        match self.script.get(&key) {
            Some(Ok((status, content_type, body))) => Ok(FetchResponse {
                status: *status,
                headers: vec![("content-type".to_owned(), (*content_type).to_owned())],
                body: body.clone().into_bytes(),
                final_url: request.url.clone(),
                truncated: false,
            }),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(FetchResponse {
                status: 404,
                headers: Vec::new(),
                body: Vec::new(),
                final_url: request.url.clone(),
                truncated: false,
            }),
        }
    }
}

fn html(body: &str) -> Reply {
    Ok((200, "text/html", body.to_owned()))
}

fn api(body: &str) -> Reply {
    Ok((200, "application/json", body.to_owned()))
}

const LEVER_API: &str = "https://api.lever.co/v0/postings/acme?mode=json";
const LEVER_JOBS: &str = r#"[
    {"id":"a1","text":"IAM engineer","hostedUrl":"https://jobs.lever.co/acme/a1","descriptionPlain":"Keycloak","createdAt":1700000000000},
    {"id":"a1","text":"IAM engineer again","hostedUrl":"https://jobs.lever.co/acme/a1","descriptionPlain":"Keycloak"}
]"#;

fn scanner(fetcher: &Arc<FakeFetcher>, max_paths: usize) -> AtsScanner<FakeFetcher> {
    AtsScanner::new(Arc::clone(fetcher), max_paths, Duration::from_secs(60))
}

#[tokio::test]
async fn the_board_linked_from_a_career_path_is_read_through_the_api() {
    let fetcher = Arc::new(FakeFetcher::new(vec![
        ("https://acme.io/", html("<html>home</html>")),
        (
            "https://acme.io/careers",
            html(r#"<a href="https://jobs.lever.co/acme">Jobs</a>"#),
        ),
        (LEVER_API, api(LEVER_JOBS)),
    ]));
    let scan = scanner(&fetcher, 6)
        .scan(&domain("acme.io"), now())
        .await
        .unwrap();
    assert_eq!(scan.boards.len(), 1);
    assert_eq!(scan.boards[0].board, board(Vendor::Lever, "acme"));
    assert_eq!(scan.boards[0].candidates.len(), 1);
    assert_eq!(
        fetcher.purpose_of("https://acme.io/careers"),
        Some(FetchPurpose::Crawl)
    );
    assert_eq!(fetcher.purpose_of(LEVER_API), Some(FetchPurpose::Api));
    assert_eq!(
        fetcher.urls(),
        ["https://acme.io/", "https://acme.io/careers", LEVER_API]
    );
    assert!(
        fetcher
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|(_, _, s)| *s == SourceName::Ats)
    );
}

#[tokio::test]
async fn a_homepage_link_to_a_careers_page_is_followed_one_level() {
    let fetcher = Arc::new(FakeFetcher::new(vec![
        (
            "https://acme.io/",
            html(
                r#"<a href="/work/careers-home">Careers</a><a href="https://elsewhere.com/careers">x</a>"#,
            ),
        ),
        (
            "https://acme.io/work/careers-home",
            html(r#"<a href="https://jobs.lever.co/acme">Jobs</a>"#),
        ),
        (LEVER_API, api(LEVER_JOBS)),
    ]));
    let scan = scanner(&fetcher, 6)
        .scan(&domain("acme.io"), now())
        .await
        .unwrap();
    assert_eq!(scan.boards.len(), 1);
    assert_eq!(
        fetcher.urls(),
        [
            "https://acme.io/",
            "https://acme.io/work/careers-home",
            LEVER_API
        ]
    );
}

#[tokio::test]
async fn a_board_on_the_homepage_needs_no_further_page() {
    let fetcher = Arc::new(FakeFetcher::new(vec![
        (
            "https://acme.io/",
            html(r#"<a href="https://jobs.lever.co/acme">Jobs</a>"#),
        ),
        (LEVER_API, api(LEVER_JOBS)),
    ]));
    let scan = scanner(&fetcher, 6)
        .scan(&domain("acme.io"), now())
        .await
        .unwrap();
    assert_eq!(scan.boards.len(), 1);
    assert_eq!(fetcher.urls().len(), 2);
}

#[tokio::test]
async fn no_link_means_no_board_and_no_guessed_api_call() {
    let fetcher = Arc::new(FakeFetcher::new(vec![(
        "https://acme.io/",
        html("<p>acme careers on lever and greenhouse</p>"),
    )]));
    let scan = scanner(&fetcher, 6)
        .scan(&domain("acme.io"), now())
        .await
        .unwrap();
    assert!(scan.boards.is_empty());
    let urls = fetcher.urls();
    assert_eq!(urls.len(), 1 + CAREER_PATHS.len());
    assert!(urls.iter().all(|url| url.starts_with("https://acme.io/")));
    assert_eq!(
        fetcher
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, purpose, _)| *purpose == FetchPurpose::Api)
            .count(),
        0
    );
}

#[tokio::test]
async fn the_career_path_budget_is_respected() {
    let fetcher = Arc::new(FakeFetcher::new(Vec::new()));
    scanner(&fetcher, 2)
        .scan(&domain("acme.io"), now())
        .await
        .unwrap();
    assert_eq!(
        fetcher.urls(),
        [
            "https://acme.io/",
            "https://acme.io/careers",
            "https://acme.io/jobs"
        ]
    );
}

#[tokio::test]
async fn an_api_failure_or_bad_json_yields_no_candidates() {
    let fetcher = Arc::new(FakeFetcher::new(vec![
        (
            "https://acme.io/",
            html(
                r#"<a href="https://jobs.lever.co/acme">a</a><a href="https://jobs.ashbyhq.com/Acme">b</a>"#,
            ),
        ),
        (LEVER_API, Ok((500, "text/plain", String::new()))),
        (
            "https://api.ashbyhq.com/posting-api/job-board/Acme?includeCompensation=false",
            api("not json"),
        ),
    ]));
    let scan = scanner(&fetcher, 6)
        .scan(&domain("acme.io"), now())
        .await
        .unwrap();
    assert!(scan.boards.is_empty());
}

#[tokio::test]
async fn a_rate_limit_longer_than_the_wait_ends_the_scan() {
    let fetcher = Arc::new(FakeFetcher::new(vec![(
        "https://acme.io/",
        Err(FetchError::RateLimited {
            retry_after: Duration::from_secs(600),
        }),
    )]));
    let result = scanner(&fetcher, 6).scan(&domain("acme.io"), now()).await;
    assert!(matches!(result, Err(SourceError::RateLimited { .. })));
}

type Asked = (Vec<ProfileId>, DateTime<Utc>, u32);

struct FakeTargets {
    due: Vec<NormalizedDomain>,
    checked: Mutex<Vec<NormalizedDomain>>,
    asked: Mutex<Vec<Asked>>,
}

impl AtsTargets for FakeTargets {
    async fn list_due(
        &self,
        profiles: &[ProfileId],
        checked_before: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<NormalizedDomain>, CoreError> {
        self.asked
            .lock()
            .unwrap()
            .push((profiles.to_vec(), checked_before, limit));
        Ok(self.due.clone())
    }

    async fn mark_checked(
        &self,
        domains: &[NormalizedDomain],
        _at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        self.checked.lock().unwrap().extend_from_slice(domains);
        Ok(())
    }
}

fn settings() -> AtsSettings {
    AtsSettings {
        batch_size: 20,
        revisit_interval: Duration::from_secs(7 * 86_400),
        max_career_paths: 1,
        max_rate_limit_wait: Duration::from_secs(60),
    }
}

#[tokio::test]
async fn collect_marks_every_scanned_domain_checked_and_returns_candidates() {
    let fetcher = Arc::new(FakeFetcher::new(vec![
        (
            "https://acme.io/",
            html(r#"<a href="https://jobs.lever.co/acme">Jobs</a>"#),
        ),
        (LEVER_API, api(LEVER_JOBS)),
    ]));
    let targets = FakeTargets {
        due: vec![domain("acme.io"), domain("empty.fr")],
        checked: Mutex::new(Vec::new()),
        asked: Mutex::new(Vec::new()),
    };
    let source = AtsSource::new(Arc::clone(&fetcher), targets, settings());
    let candidates = source.collect(&[], now()).await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].domain, domain("acme.io"));
    assert_eq!(
        *source.targets.checked.lock().unwrap(),
        [domain("acme.io"), domain("empty.fr")]
    );
    assert_eq!(
        *source.targets.asked.lock().unwrap(),
        [(Vec::new(), now() - chrono::Duration::days(7), 20)]
    );
    assert_eq!(source.name(), SourceName::Ats);
}

fn active(id: u128, company_jobs: bool) -> ActiveProfile {
    let mut value = ValidatedProfile::parse(
        from_str(include_str!(
            "../../../../docs/examples/profiles/iam-ferriskey.json"
        ))
        .unwrap(),
    )
    .unwrap()
    .to_json();
    value["enrichment"]["company_jobs"]["enabled"] = json!(company_jobs);
    let profile_id = ProfileId(Uuid::from_u128(id));
    ActiveProfile {
        profile: Profile {
            id: profile_id,
            name: ProfileName::parse("p").unwrap(),
            active: true,
            created_at: now(),
            updated_at: now(),
        },
        revision: ProfileRevision {
            profile_id,
            number: RevisionNumber::FIRST,
            definition: ValidatedProfile::parse(value).unwrap(),
            created_by: None,
            created_at: now(),
        },
    }
}

#[tokio::test]
async fn collect_reads_the_leads_of_the_profiles_that_enrich_with_company_jobs() {
    let fetcher = Arc::new(FakeFetcher::new(Vec::new()));
    let targets = FakeTargets {
        due: Vec::new(),
        checked: Mutex::new(Vec::new()),
        asked: Mutex::new(Vec::new()),
    };
    let source = AtsSource::new(fetcher, targets, settings());
    source
        .collect(&[active(1, true), active(2, false)], now())
        .await
        .unwrap();
    assert_eq!(
        *source.targets.asked.lock().unwrap(),
        [(
            vec![ProfileId(Uuid::from_u128(1))],
            now() - chrono::Duration::days(7),
            20
        )]
    );
}

#[tokio::test]
async fn collect_stops_on_a_rate_limit_without_marking_the_domain() {
    let fetcher = Arc::new(FakeFetcher::new(vec![(
        "https://later.fr/",
        Err(FetchError::RateLimited {
            retry_after: Duration::from_secs(600),
        }),
    )]));
    let targets = FakeTargets {
        due: vec![domain("empty.fr"), domain("later.fr"), domain("never.fr")],
        checked: Mutex::new(Vec::new()),
        asked: Mutex::new(Vec::new()),
    };
    let source = AtsSource::new(fetcher, targets, settings());
    let candidates = source.collect(&[], now()).await.unwrap();
    assert!(candidates.is_empty());
    assert_eq!(
        *source.targets.checked.lock().unwrap(),
        [domain("empty.fr")]
    );
}
