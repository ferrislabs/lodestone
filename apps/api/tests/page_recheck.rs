mod support;

use std::{num::NonZeroU32, sync::Arc, time::Duration};

use api::{
    jobs::{CompanyPages, Intervals, JobRouter, WiredSources, schedule_page_recheck},
    providers::CatalogAccess,
};
use chrono::{DateTime, Duration as Delta, Utc};
use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{Method::GET, MockServer};
use lodestone_core::{
    JobId, JobQueue, NormalizedDomain,
    company::PAGE_RECHECK_JOB_KIND,
    infrastructure::{contact_target::postgres::PgAtsTargets, job::postgres::PgJobQueue},
    jobs::{JobHandler, JobOutcome},
};
use serde_json::json;
use sources::{
    ats::{AtsSettings, AtsSource},
    boamp::{BoampSettings, BoampSource},
    company::{CompanyResearch, CompanySettings},
    decp::{DecpSettings, DecpSource},
    france_travail::{FranceTravailSettings, FranceTravailSource},
    hacker_news::{HackerNewsSettings, HackerNewsSource},
    ted::{TedSettings, TedSource},
};
use sqlx::PgPool;
use support::Harness;
use url::Url;
use uuid::Uuid;

const MISSING: &str = "<html><head><title>Notion</title></head><body><h1>Cette page est introuvable</h1><p>Retournez à l'accueil de cet espace de travail.</p></body></html>";
const HOME: &str =
    "<html><body><h1>Jawagg</h1><p>Le studio qui conçoit vos jeux de société.</p></body></html>";

fn polite() -> Arc<PoliteFetcher> {
    Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact: "bot@example.com".to_owned(),
            requests_per_host_per_minute: NonZeroU32::new(10_000).unwrap(),
            request_timeout: Duration::from_secs(5),
            max_body_bytes: 1024 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(60),
            source_quotas: Vec::new(),
            allow_private_targets: true,
        })
        .unwrap(),
    )
}

fn router(harness: &Harness, pool: &PgPool, site: &MockServer) -> JobRouter {
    let fetcher = polite();
    let url = |path: &str| Url::parse(&site.url(path)).unwrap();
    let pages: CompanyPages = CompanyResearch::new(
        Arc::clone(&fetcher),
        CompanySettings {
            entreprises_url: url("/company").into(),
            max_pages: 5,
            max_rate_limit_wait: Duration::from_secs(1),
            origin: Some(url("/")),
        },
    );
    JobRouter::new(
        harness.state.usecase.clone(),
        WiredSources {
            boamp: BoampSource::new(
                Arc::clone(&fetcher),
                BoampSettings {
                    base_url: url("/boamp").into(),
                    lookback_days: 7,
                    max_pages: 1,
                },
            ),
            decp: DecpSource::new(
                Arc::clone(&fetcher),
                DecpSettings {
                    dataset_api_url: url("/decp").into(),
                    annuaire_url: url("/annuaire"),
                    entreprises_url: url("/company").into(),
                    lookback_days: 7,
                    notified_within_days: 7,
                    max_body_bytes: 1024 * 1024,
                    max_rate_limit_wait: Duration::from_secs(1),
                },
            ),
            ted: TedSource::new(
                Arc::clone(&fetcher),
                TedSettings {
                    base_url: url("/ted").into(),
                    lookback_days: 7,
                    max_pages: 1,
                    buyer_countries: Vec::new(),
                },
            ),
            france_travail: FranceTravailSource::new(
                Arc::clone(&fetcher),
                CatalogAccess::new(
                    harness.state.usecase.clone(),
                    Arc::clone(&harness.state.providers),
                    Arc::clone(&harness.state.cipher),
                ),
                FranceTravailSettings {
                    published_since_days: 7,
                    max_pages: 1,
                },
            ),
            hacker_news: HackerNewsSource::new(
                Arc::clone(&fetcher),
                HackerNewsSettings {
                    base_url: url("/hn/").into(),
                    max_pages: 1,
                },
            ),
            ats: AtsSource::new(
                fetcher,
                PgAtsTargets::new(pool.clone()),
                AtsSettings {
                    batch_size: 1,
                    revisit_interval: Duration::from_secs(60),
                    max_career_paths: 1,
                    max_rate_limit_wait: Duration::from_secs(1),
                },
            ),
        },
        Intervals {
            other_sources: Duration::from_secs(6 * 3600),
            decp: Duration::from_secs(6 * 3600),
            hacker_news: Duration::from_secs(24 * 3600),
        },
    )
    .with_page_verifier(pages)
}

async fn store_legacy_dossier(pool: &PgPool, site: &MockServer, built_at: DateTime<Utc>) {
    let pages = json!([
        {"url": site.url("/"), "text": "Jawagg"},
        {"url": site.url("/mentions-legales"), "text": "Cette page est introuvable"},
        {"url": site.url("/a-propos"), "text": "Cette page est introuvable"},
    ]);
    sqlx::query(
        "INSERT INTO company_dossiers (domain, state, pages, content_hash, content_at, built_at, next_attempt_at) VALUES ('jawagg.fr', 'partial', $1, 'legacy', $2, $2, $2)",
    )
    .bind(pages)
    .bind(built_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn recheck_jobs(pool: &PgPool) -> Vec<Uuid> {
    sqlx::query_scalar("SELECT id FROM jobs WHERE kind = $1")
        .bind(PAGE_RECHECK_JOB_KIND)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_startup_recheck_drops_unconfirmed_dossier_pages_once(pool: PgPool) {
    let harness = Harness::new(pool.clone()).await;
    let site = MockServer::start_async().await;
    site.mock(|when, then| {
        when.method(GET).path("/");
        then.status(200)
            .header("content-type", "text/html")
            .body(HOME);
    });
    for path in ["/mentions-legales", "/a-propos"] {
        site.mock(|when, then| {
            when.method(GET).path(path);
            then.status(200)
                .header("content-type", "text/html")
                .body(MISSING);
        });
    }
    let built_at = Utc::now() - Delta::days(3);
    store_legacy_dossier(&pool, &site, built_at).await;
    let queue = Arc::new(PgJobQueue::new(pool.clone()));

    schedule_page_recheck(&queue, &harness.state.usecase)
        .await
        .unwrap();
    let ids = recheck_jobs(&pool).await;
    assert_eq!(ids.len(), 1);
    let job = queue.find(JobId(ids[0])).await.unwrap().unwrap();

    let outcome = router(&harness, &pool, &site).handle(&job).await.unwrap();

    assert_eq!(outcome, JobOutcome::Done);
    let dossier = harness
        .state
        .usecase
        .company_dossier(&NormalizedDomain::from_host("jawagg.fr").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        dossier
            .pages
            .iter()
            .map(|page| page.url.clone())
            .collect::<Vec<_>>(),
        [site.url("/")]
    );
    assert_eq!(dossier.ignored_links, 2);
    assert!(dossier.content_at.unwrap() > built_at);
    assert_ne!(dossier.content_hash.as_deref(), Some("legacy"));
    assert!(dossier.pages_checked_at.is_some());

    schedule_page_recheck(&queue, &harness.state.usecase)
        .await
        .unwrap();
    assert_eq!(recheck_jobs(&pool).await.len(), 1);
}
