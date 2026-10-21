mod support;

use lodestone_core::{
    LeadQuery, LeadRepository, LeadSort, LeadStatus, LodestoneUseCase, ProfileId, Provenance,
    Score, ScoreBreakdown, Signal, SignalId, SignalKind, SignalRepository, SourceName,
    SuppressionTarget, UnitInterval, VerdictFilter,
    infrastructure::{
        lead::postgres::PgLeadRepository, postgres::with_tx, signal::postgres::PgSignalRepository,
    },
};
use sqlx::PgPool;
use support::{at, create_profile, domain};

async fn seed(pool: &PgPool, profile: ProfileId, host: &str, score: f64, seen: i64) {
    with_tx(pool, async |shared| {
        let mut repo = PgLeadRepository::new(&shared);
        let lead = repo.record_seen(profile, &domain(host), at(seen)).await?;
        repo.update_score(
            lead.id,
            &ScoreBreakdown {
                total: Score::new(score).unwrap(),
                contributions: Vec::new(),
            },
            at(seen),
        )
        .await?;
        Ok(())
    })
    .await
    .unwrap();
}

async fn set_status(pool: &PgPool, profile: ProfileId, host: &str, to: LeadStatus) {
    with_tx(pool, async |shared| {
        let mut repo = PgLeadRepository::new(&shared);
        let lead = repo.find_by_domain(profile, &domain(host)).await?.unwrap();
        repo.update_status(lead.id, lead.status, to, at(50)).await?;
        Ok(())
    })
    .await
    .unwrap();
}

async fn add_signal(pool: &PgPool, profile: ProfileId, host: &str, kind: &str) {
    with_tx(pool, async |shared| {
        let lead = PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain(host))
            .await?
            .unwrap();
        PgSignalRepository::new(&shared)
            .record(&Signal {
                id: SignalId(common::generate_uuid_v7()),
                lead_id: lead.id,
                kind: SignalKind::parse(kind).unwrap(),
                value: None,
                source: SourceName::HttpProbe,
                raw: serde_json::json!({}),
                provenance: Provenance::parse("https://example.com/x").unwrap(),
                strength: UnitInterval::new(0.5).unwrap(),
                observed_at: at(1),
                agent: None,
            })
            .await?;
        Ok(())
    })
    .await
    .unwrap();
}

fn query() -> LeadQuery {
    LeadQuery {
        limit: 50,
        ..LeadQuery::default()
    }
}

async fn domains(pool: &PgPool, profile: ProfileId, query: LeadQuery) -> (Vec<String>, u64) {
    let page = with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .list_page(profile, &query)
            .await
    })
    .await
    .unwrap();
    (
        page.items
            .iter()
            .map(|lead| lead.domain.as_str().to_owned())
            .collect(),
        page.total,
    )
}

async fn fixture(pool: &PgPool) -> ProfileId {
    let profile = create_profile(pool, "alpha").await;
    seed(pool, profile, "alpha-one.com", 10.0, 30).await;
    seed(pool, profile, "beta-two.com", 30.0, 20).await;
    seed(pool, profile, "gamma-three.com", 30.0, 10).await;
    set_status(pool, profile, "beta-two.com", LeadStatus::Qualified).await;
    add_signal(pool, profile, "alpha-one.com", "product_detected").await;
    profile
}

#[sqlx::test(migrations = "../../migrations")]
async fn sorts_by_score_descending_with_id_as_tie_breaker(pool: PgPool) {
    let profile = fixture(&pool).await;

    let (first, total) = domains(&pool, profile, query()).await;
    let (second, _) = domains(&pool, profile, query()).await;

    assert_eq!(total, 3);
    assert_eq!(first, second);
    assert_eq!(first[2], "alpha-one.com");
    let top: std::collections::BTreeSet<_> = first[..2].iter().cloned().collect();
    assert_eq!(
        top,
        ["beta-two.com".to_owned(), "gamma-three.com".to_owned()].into()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_sort_orders_as_named(pool: PgPool) {
    let profile = fixture(&pool).await;
    let sorted = |sort| LeadQuery { sort, ..query() };

    let (asc, _) = domains(&pool, profile, sorted(LeadSort::ScoreAsc)).await;
    assert_eq!(asc[0], "alpha-one.com");

    let (last_seen, _) = domains(&pool, profile, sorted(LeadSort::LastSeenDesc)).await;
    assert_eq!(
        last_seen,
        ["alpha-one.com", "beta-two.com", "gamma-three.com"]
    );

    let (first_seen, _) = domains(&pool, profile, sorted(LeadSort::FirstSeenDesc)).await;
    assert_eq!(
        first_seen,
        ["alpha-one.com", "beta-two.com", "gamma-three.com"]
    );

    let (by_domain, _) = domains(&pool, profile, sorted(LeadSort::DomainAsc)).await;
    assert_eq!(
        by_domain,
        ["alpha-one.com", "beta-two.com", "gamma-three.com"]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn filters_narrow_the_page_and_the_total(pool: PgPool) {
    let profile = fixture(&pool).await;

    let (by_status, total) = domains(
        &pool,
        profile,
        LeadQuery {
            status: Some(LeadStatus::Qualified),
            ..query()
        },
    )
    .await;
    assert_eq!((by_status, total), (vec!["beta-two.com".to_owned()], 1));

    let (_, total) = domains(
        &pool,
        profile,
        LeadQuery {
            min_score: Some(30.0),
            ..query()
        },
    )
    .await;
    assert_eq!(total, 2);

    let (by_domain, _) = domains(
        &pool,
        profile,
        LeadQuery {
            domain_contains: Some("THREE".to_owned()),
            ..query()
        },
    )
    .await;
    assert_eq!(by_domain, ["gamma-three.com"]);

    let (by_signal, _) = domains(
        &pool,
        profile,
        LeadQuery {
            signal_kind: Some(SignalKind::parse("product_detected").unwrap()),
            ..query()
        },
    )
    .await;
    assert_eq!(by_signal, ["alpha-one.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn like_metacharacters_in_the_domain_filter_match_literally(pool: PgPool) {
    let profile = fixture(&pool).await;

    for needle in ["%", "_", "\\"] {
        let (found, total) = domains(
            &pool,
            profile,
            LeadQuery {
                domain_contains: Some(needle.to_owned()),
                ..query()
            },
        )
        .await;
        assert!(found.is_empty(), "{needle}");
        assert_eq!(total, 0);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn limit_and_offset_page_through_a_stable_order(pool: PgPool) {
    let profile = fixture(&pool).await;
    let (all, _) = domains(&pool, profile, query()).await;

    let (page_one, total) = domains(
        &pool,
        profile,
        LeadQuery {
            limit: 2,
            ..query()
        },
    )
    .await;
    let (page_two, _) = domains(
        &pool,
        profile,
        LeadQuery {
            limit: 2,
            offset: 2,
            ..query()
        },
    )
    .await;
    let (beyond, beyond_total) = domains(
        &pool,
        profile,
        LeadQuery {
            limit: 2,
            offset: 10,
            ..query()
        },
    )
    .await;

    assert_eq!(total, 3);
    assert_eq!([page_one, page_two].concat(), all);
    assert!(beyond.is_empty());
    assert_eq!(beyond_total, 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn other_profiles_leads_are_not_listed(pool: PgPool) {
    let profile = fixture(&pool).await;
    let other = create_profile(&pool, "beta").await;
    seed(&pool, other, "elsewhere.com", 99.0, 5).await;

    let (found, total) = domains(&pool, profile, query()).await;

    assert_eq!(total, 3);
    assert!(!found.contains(&"elsewhere.com".to_owned()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn suppressed_domains_are_hidden_including_through_subdomains(pool: PgPool) {
    let profile = fixture(&pool).await;
    seed(&pool, profile, "optout.io", 80.0, 5).await;
    LodestoneUseCase::new(pool.clone())
        .add_suppression(
            SuppressionTarget::Domain(domain("www.optout.io")),
            None,
            None,
        )
        .await
        .unwrap();

    let (found, total) = domains(&pool, profile, query()).await;

    assert_eq!(total, 3);
    assert!(!found.contains(&"optout.io".to_owned()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn profile_summaries_count_visible_leads_and_latest_revision(pool: PgPool) {
    let profile = fixture(&pool).await;
    seed(&pool, profile, "optout.io", 80.0, 5).await;
    LodestoneUseCase::new(pool.clone())
        .add_suppression(SuppressionTarget::Domain(domain("optout.io")), None, None)
        .await
        .unwrap();

    let summaries = LodestoneUseCase::new(pool.clone())
        .list_profiles_summary()
        .await
        .unwrap();

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, profile);
    assert_eq!(summaries[0].name.as_str(), "alpha");
    assert_eq!(summaries[0].lead_count, 3);
    assert_eq!(summaries[0].revision.map(|r| r.get()), Some(1));
}

async fn judge(pool: &PgPool, host: &str, confidence: Option<f64>, verdict: Option<&str>) {
    sqlx::query("UPDATE leads SET confidence = $2, verdict = $3 WHERE domain = $1")
        .bind(host)
        .bind(confidence)
        .bind(verdict)
        .execute(pool)
        .await
        .unwrap();
}

async fn judged_fixture(pool: &PgPool) -> ProfileId {
    let profile = create_profile(pool, "alpha").await;
    seed(pool, profile, "sure.com", 5.0, 10).await;
    seed(pool, profile, "maybe.com", 50.0, 20).await;
    seed(pool, profile, "maybe-too.com", 20.0, 30).await;
    seed(pool, profile, "out.com", 90.0, 40).await;
    seed(pool, profile, "waiting.com", 70.0, 50).await;
    judge(pool, "sure.com", Some(92.5), Some("fiable")).await;
    judge(pool, "maybe.com", Some(55.0), Some("a_verifier")).await;
    judge(pool, "maybe-too.com", Some(55.0), Some("a_verifier")).await;
    judge(pool, "out.com", Some(8.0), Some("ecarte")).await;
    profile
}

#[sqlx::test(migrations = "../../migrations")]
async fn confidence_sorts_with_pending_leads_last_and_ties_broken_by_score(pool: PgPool) {
    let profile = judged_fixture(&pool).await;
    assert_eq!(LeadSort::default(), LeadSort::ConfidenceDesc);

    let (default, total) = domains(&pool, profile, query()).await;
    let (again, _) = domains(&pool, profile, query()).await;

    assert_eq!(total, 5);
    assert_eq!(
        default,
        [
            "sure.com",
            "maybe.com",
            "maybe-too.com",
            "out.com",
            "waiting.com"
        ]
    );
    assert_eq!(default, again);

    let (ascending, _) = domains(
        &pool,
        profile,
        LeadQuery {
            sort: LeadSort::ConfidenceAsc,
            ..query()
        },
    )
    .await;
    assert_eq!(
        ascending,
        [
            "out.com",
            "maybe.com",
            "maybe-too.com",
            "sure.com",
            "waiting.com"
        ]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_verdict_filter_keeps_the_chosen_verdicts_and_pending_ones(pool: PgPool) {
    let profile = judged_fixture(&pool).await;
    let with = |verdicts: &[VerdictFilter]| LeadQuery {
        verdicts: verdicts.to_vec(),
        ..query()
    };

    let (hidden_discarded, total) = domains(
        &pool,
        profile,
        with(&[
            VerdictFilter::Reliable,
            VerdictFilter::ToCheck,
            VerdictFilter::Pending,
        ]),
    )
    .await;
    assert_eq!(total, 4);
    assert_eq!(
        hidden_discarded,
        ["sure.com", "maybe.com", "maybe-too.com", "waiting.com"]
    );

    let (only_discarded, _) = domains(&pool, profile, with(&[VerdictFilter::Discarded])).await;
    assert_eq!(only_discarded, ["out.com"]);

    let (pending, _) = domains(&pool, profile, with(&[VerdictFilter::Pending])).await;
    assert_eq!(pending, ["waiting.com"]);

    let (everything, total) = domains(&pool, profile, with(&[])).await;
    assert_eq!(total, 5);
    assert_eq!(everything.len(), 5);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_minimum_confidence_leaves_out_lower_and_pending_leads(pool: PgPool) {
    let profile = judged_fixture(&pool).await;

    let (kept, total) = domains(
        &pool,
        profile,
        LeadQuery {
            min_confidence: Some(55.0),
            ..query()
        },
    )
    .await;

    assert_eq!(total, 3);
    assert_eq!(kept, ["sure.com", "maybe.com", "maybe-too.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_listing_carries_the_latest_summary_of_each_lead(pool: PgPool) {
    let profile = judged_fixture(&pool).await;
    let sure: uuid::Uuid = sqlx::query_scalar("SELECT id FROM leads WHERE domain = 'sure.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    for (age, summary) in [(20, "older"), (5, "latest")] {
        sqlx::query(
            "INSERT INTO assessments (id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, created_at)
             VALUES ($1, $2, 1, $3, 'angle', '[]'::jsonb, 'm', 'aggregator@x', now() - make_interval(secs => $4))",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(sure)
        .bind(summary)
        .bind(f64::from(age))
        .execute(&pool)
        .await
        .unwrap();
    }

    let listing = LodestoneUseCase::new(pool)
        .list_leads(profile, query())
        .await
        .unwrap();

    assert_eq!(listing.summaries.len(), 1);
    let id = listing
        .page
        .items
        .iter()
        .find(|lead| lead.domain.as_str() == "sure.com")
        .unwrap()
        .id;
    assert_eq!(
        listing.summaries.get(&id).map(String::as_str),
        Some("latest")
    );
}
