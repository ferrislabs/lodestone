mod support;

use lodestone_core::{
    Decision, LeadId, LeadQuery, LeadRepository, LeadSort, ProfileId, Score, ScoringRepository,
    Verdict, VerdictProbabilities, VerdictReason,
    infrastructure::{
        lead::postgres::PgLeadRepository, postgres::with_tx,
        scoring_agent::postgres::PgScoringRepository,
    },
};
use sqlx::PgPool;
use support::{at, create_profile, domain};

async fn seed(pool: &PgPool, profile: ProfileId, host: &str, score: f64) -> LeadId {
    with_tx(pool, async |shared| {
        let mut repo = PgLeadRepository::new(&shared);
        let lead = repo.record_seen(profile, &domain(host), at(10)).await?;
        repo.update_score(lead.id, Score::new(score).unwrap(), at(10))
            .await?;
        Ok(lead.id)
    })
    .await
    .unwrap()
}

fn judged(verdict: Verdict, confidence: f64, probabilities: (f64, f64, f64)) -> Decision {
    Decision {
        score: Some(50),
        verdict: Some(verdict),
        confidence: Some(confidence),
        probabilities: Some(VerdictProbabilities::new(
            probabilities.0,
            probabilities.1,
            probabilities.2,
        )),
        reason: None,
        exclusions: Vec::new(),
        contributors: 1,
    }
}

fn excluded() -> Decision {
    Decision {
        verdict: Some(Verdict::Discarded),
        confidence: Some(100.0),
        reason: Some(VerdictReason::Excluded),
        ..judged(Verdict::Discarded, 100.0, (0.95, 0.04, 0.01))
    }
}

fn awaiting() -> Decision {
    Decision {
        score: None,
        verdict: None,
        confidence: None,
        probabilities: None,
        reason: Some(VerdictReason::AwaitingScoring),
        exclusions: Vec::new(),
        contributors: 0,
    }
}

async fn decide(pool: &PgPool, lead: LeadId, decision: Decision) {
    with_tx(pool, async |shared| {
        PgScoringRepository::new(&shared)
            .apply_decision(lead, &decision)
            .await
    })
    .await
    .unwrap();
}

async fn stored(pool: &PgPool, lead: LeadId) -> Option<f64> {
    sqlx::query_scalar("SELECT reliable_probability FROM leads WHERE id = $1")
        .bind(lead.0)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn defaults() -> LeadQuery {
    LeadQuery {
        limit: 50,
        ..LeadQuery::default()
    }
}

fn query(sort: LeadSort) -> LeadQuery {
    LeadQuery { sort, ..defaults() }
}

async fn listed(pool: &PgPool, profile: ProfileId, query: LeadQuery) -> Vec<String> {
    with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .list_page(profile, &query)
            .await
    })
    .await
    .unwrap()
    .items
    .iter()
    .map(|lead| lead.domain.as_str().to_owned())
    .collect()
}

#[test]
fn the_default_lead_sort_is_the_relevance() {
    assert_eq!(LeadSort::default(), LeadSort::RelevanceDesc);
    assert_eq!(LeadSort::ALL[0], LeadSort::RelevanceDesc);
    assert_eq!(LeadSort::RelevanceDesc.as_str(), "relevance_desc");
    assert_eq!(
        LeadSort::parse("relevance_desc"),
        Some(LeadSort::RelevanceDesc)
    );
    assert_eq!(
        LeadSort::parse("confidence_desc"),
        Some(LeadSort::ConfidenceDesc)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_probably_reliable_comes_before_a_lead_surely_to_check(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let to_check = seed(&pool, profile, "to-check.com", 90.0).await;
    let reliable = seed(&pool, profile, "reliable.com", 10.0).await;
    decide(
        &pool,
        to_check,
        judged(Verdict::ToCheck, 90.0, (0.05, 0.9, 0.05)),
    )
    .await;
    decide(
        &pool,
        reliable,
        judged(Verdict::Reliable, 80.0, (0.8, 0.15, 0.05)),
    )
    .await;

    let by_default = listed(&pool, profile, defaults()).await;
    let by_relevance = listed(&pool, profile, query(LeadSort::RelevanceDesc)).await;
    let by_confidence = listed(&pool, profile, query(LeadSort::ConfidenceDesc)).await;

    assert_eq!(by_default, ["reliable.com", "to-check.com"]);
    assert_eq!(by_relevance, by_default);
    assert_eq!(by_confidence, ["to-check.com", "reliable.com"]);
    assert_eq!(stored(&pool, to_check).await, Some(5.0));
    assert_eq!(stored(&pool, reliable).await, Some(80.0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_excluded_lead_scores_zero_and_leads_without_a_verdict_come_last_by_score(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let barely = seed(&pool, profile, "barely.com", 5.0).await;
    let likely = seed(&pool, profile, "likely.com", 10.0).await;
    let out = seed(&pool, profile, "out.com", 99.0).await;
    let waiting_high = seed(&pool, profile, "waiting-high.com", 70.0).await;
    let waiting_low = seed(&pool, profile, "waiting-low.com", 20.0).await;
    decide(
        &pool,
        barely,
        judged(Verdict::Discarded, 85.0, (0.05, 0.1, 0.85)),
    )
    .await;
    decide(
        &pool,
        likely,
        judged(Verdict::ToCheck, 60.0, (0.3, 0.6, 0.1)),
    )
    .await;
    decide(&pool, out, excluded()).await;

    let found = listed(&pool, profile, query(LeadSort::RelevanceDesc)).await;

    assert_eq!(
        found,
        [
            "likely.com",
            "barely.com",
            "out.com",
            "waiting-high.com",
            "waiting-low.com"
        ]
    );
    assert_eq!(stored(&pool, out).await, Some(0.0));
    assert_eq!(stored(&pool, waiting_high).await, None);
    assert_eq!(stored(&pool, waiting_low).await, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn scoring_a_lead_again_replaces_its_reliable_probability(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let lead = seed(&pool, profile, "acme.com", 10.0).await;

    decide(
        &pool,
        lead,
        judged(Verdict::Reliable, 80.0, (0.8, 0.15, 0.05)),
    )
    .await;
    assert_eq!(stored(&pool, lead).await, Some(80.0));

    decide(
        &pool,
        lead,
        judged(Verdict::ToCheck, 62.5, (0.2, 0.625, 0.175)),
    )
    .await;
    assert_eq!(stored(&pool, lead).await, Some(20.0));

    decide(&pool, lead, excluded()).await;
    assert_eq!(stored(&pool, lead).await, Some(0.0));

    decide(&pool, lead, awaiting()).await;
    assert_eq!(stored(&pool, lead).await, None);
}

async fn link_company(pool: &PgPool, host: &str, siren: &str) {
    sqlx::query("INSERT INTO companies (siren, legal_name) VALUES ($1, 'ACME SAS') ON CONFLICT (siren) DO NOTHING")
        .bind(siren)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO company_dossiers (domain, state, siren, pages, built_at, next_attempt_at, company_siren)
         VALUES ($1, 'complete', $2, '[]'::jsonb, now(), now() + interval '90 days', $2)",
    )
    .bind(host)
    .bind(siren)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_grouped_list_stands_a_company_by_its_most_reliable_lead(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let confident = seed(&pool, profile, "acme.com", 30.0).await;
    let probable = seed(&pool, profile, "acme.fr", 10.0).await;
    let solo = seed(&pool, profile, "solo.io", 50.0).await;
    link_company(&pool, "acme.com", "552081317").await;
    link_company(&pool, "acme.fr", "552081317").await;
    decide(
        &pool,
        confident,
        judged(Verdict::ToCheck, 90.0, (0.05, 0.9, 0.05)),
    )
    .await;
    decide(
        &pool,
        probable,
        judged(Verdict::Reliable, 70.0, (0.7, 0.2, 0.1)),
    )
    .await;
    decide(&pool, solo, judged(Verdict::ToCheck, 50.0, (0.3, 0.5, 0.2))).await;
    let grouped = |sort| LeadQuery {
        group_by_company: true,
        ..query(sort)
    };

    let page = with_tx(&pool, async |shared| {
        PgLeadRepository::new(&shared)
            .list_page(profile, &grouped(LeadSort::RelevanceDesc))
            .await
    })
    .await
    .unwrap();
    let by_confidence = listed(&pool, profile, grouped(LeadSort::ConfidenceDesc)).await;

    let names: Vec<&str> = page.items.iter().map(|lead| lead.domain.as_str()).collect();
    assert_eq!(names, ["acme.fr", "solo.io"]);
    assert_eq!(page.total, 2);
    let siblings = page.members.get(&probable).expect("members");
    assert_eq!(siblings.len(), 1);
    assert_eq!(siblings[0].id, confident);
    assert_eq!(by_confidence, ["acme.fr", "solo.io"]);
}

async fn seed_state(
    pool: &PgPool,
    lead: LeadId,
    source: &str,
    reason: Option<&str>,
    probabilities: Option<serde_json::Value>,
) {
    sqlx::query(
        "INSERT INTO lead_qualification_state (lead_id, profile_revision, sources, completed_at, verdict_source, verdict_reason, verdict_probabilities)
         VALUES ($1, 1, ARRAY['http_probe'], now(), $2, $3, $4)",
    )
    .bind(lead.0)
    .bind(source)
    .bind(reason)
    .bind(probabilities)
    .execute(pool)
    .await
    .unwrap();
}

async fn rerun_migration(pool: &PgPool) {
    sqlx::raw_sql(include_str!(
        "../../../migrations/20261021000002_add_lead_reliable_probability.down.sql"
    ))
    .execute(pool)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!(
        "../../../migrations/20261021000002_add_lead_reliable_probability.up.sql"
    ))
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_migration_backfills_the_reliable_probability_of_scored_leads_only(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let excluded_lead = seed(&pool, profile, "excluded.com", 1.0).await;
    let scored = seed(&pool, profile, "scored.com", 1.0).await;
    let rounded_down = seed(&pool, profile, "down.com", 1.0).await;
    let rounded_up = seed(&pool, profile, "up.com", 1.0).await;
    let indicators = seed(&pool, profile, "indicators.com", 1.0).await;
    let awaiting_lead = seed(&pool, profile, "awaiting.com", 1.0).await;
    let bare = seed(&pool, profile, "bare.com", 1.0).await;
    let probabilities = |fiable: f64| serde_json::json!({"fiable": fiable, "a_verifier": 0.0, "ecarte": 1.0 - fiable});
    seed_state(
        &pool,
        excluded_lead,
        "scoring",
        Some("excluded"),
        Some(probabilities(0.9)),
    )
    .await;
    seed_state(&pool, scored, "scoring", None, Some(probabilities(0.8123))).await;
    seed_state(
        &pool,
        rounded_down,
        "scoring",
        None,
        Some(probabilities(0.0004)),
    )
    .await;
    seed_state(
        &pool,
        rounded_up,
        "scoring",
        None,
        Some(probabilities(0.9996)),
    )
    .await;
    seed_state(&pool, indicators, "indicators", None, None).await;
    seed_state(
        &pool,
        awaiting_lead,
        "scoring",
        Some("awaiting_scoring"),
        None,
    )
    .await;
    sqlx::query("UPDATE leads SET verdict = 'fiable', confidence = 90 WHERE id = $1")
        .bind(indicators.0)
        .execute(&pool)
        .await
        .unwrap();

    rerun_migration(&pool).await;

    assert_eq!(stored(&pool, excluded_lead).await, Some(0.0));
    assert_eq!(stored(&pool, scored).await, Some(81.2));
    assert_eq!(stored(&pool, rounded_down).await, Some(0.0));
    assert_eq!(stored(&pool, rounded_up).await, Some(100.0));
    assert_eq!(stored(&pool, indicators).await, None);
    assert_eq!(stored(&pool, awaiting_lead).await, None);
    assert_eq!(stored(&pool, bare).await, None);
}
