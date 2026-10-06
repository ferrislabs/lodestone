mod support;

use lodestone_core::{
    CandidateHost, CandidateHosts, HostOrigin, Hostname, KnownHostRepository, NormalizedDomain,
    SourceName,
    infrastructure::{
        candidate_host::{PgCandidateHosts, PgKnownHostRepository},
        postgres::with_tx,
    },
};
use sqlx::PgPool;
use support::at;

fn host(name: &str, origin: HostOrigin, first: i64, last: i64) -> CandidateHost {
    let mut candidate =
        CandidateHost::discovered(Hostname::parse(name).unwrap(), origin, at(first)).unwrap();
    candidate.last_seen_at = at(last);
    candidate
}

fn names(hosts: &[CandidateHost]) -> Vec<&str> {
    hosts.iter().map(|h| h.host.as_str()).collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn upsert_is_idempotent_widens_and_keeps_origin(pool: PgPool) {
    let repo = PgCandidateHosts::new(pool);
    let first = host("sso.acme.com", HostOrigin::Import, 10, 20);
    assert_eq!(repo.upsert(std::slice::from_ref(&first)).await.unwrap(), 1);
    assert_eq!(repo.upsert(std::slice::from_ref(&first)).await.unwrap(), 1);

    let wider = host("sso.acme.com", HostOrigin::CertificateTransparency, 5, 30);
    repo.upsert(&[wider]).await.unwrap();
    let narrower = host("sso.acme.com", HostOrigin::Import, 12, 15);
    repo.upsert(&[narrower]).await.unwrap();

    let due = repo
        .list_due(SourceName::DnsCname, at(1000), 10)
        .await
        .unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].origin, HostOrigin::Import);
    assert_eq!(due[0].first_seen_at, at(5));
    assert_eq!(due[0].last_seen_at, at(30));
    assert_eq!(due[0].domain.as_str(), "acme.com");
}

#[sqlx::test(migrations = "../../migrations")]
async fn upsert_empty_batch_is_a_noop(pool: PgPool) {
    let repo = PgCandidateHosts::new(pool);
    assert_eq!(repo.upsert(&[]).await.unwrap(), 0);
    repo.mark_checked(&[], SourceName::DnsCname, at(0))
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_due_orders_never_checked_first_then_oldest_and_limits(pool: PgPool) {
    let repo = PgCandidateHosts::new(pool);
    let all = [
        host("c.acme.com", HostOrigin::Import, 0, 0),
        host("a.acme.com", HostOrigin::Import, 0, 0),
        host("b.acme.com", HostOrigin::Import, 0, 0),
        host("d.acme.com", HostOrigin::Import, 0, 0),
    ];
    repo.upsert(&all).await.unwrap();
    repo.mark_checked(&[all[0].host.clone()], SourceName::DnsCname, at(50))
        .await
        .unwrap();
    repo.mark_checked(&[all[1].host.clone()], SourceName::DnsCname, at(40))
        .await
        .unwrap();

    let due = repo
        .list_due(SourceName::DnsCname, at(100), 10)
        .await
        .unwrap();
    assert_eq!(
        names(&due),
        vec!["b.acme.com", "d.acme.com", "a.acme.com", "c.acme.com"]
    );
    let limited = repo
        .list_due(SourceName::DnsCname, at(100), 3)
        .await
        .unwrap();
    assert_eq!(
        names(&limited),
        vec!["b.acme.com", "d.acme.com", "a.acme.com"]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn marking_removes_from_due_until_window_passes(pool: PgPool) {
    let repo = PgCandidateHosts::new(pool);
    let one = host("sso.acme.com", HostOrigin::Import, 0, 0);
    repo.upsert(std::slice::from_ref(&one)).await.unwrap();
    repo.mark_checked(
        std::slice::from_ref(&one.host),
        SourceName::DnsCname,
        at(100),
    )
    .await
    .unwrap();

    assert!(
        repo.list_due(SourceName::DnsCname, at(100), 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        repo.list_due(SourceName::DnsCname, at(50), 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repo.list_due(SourceName::DnsCname, at(101), 10)
            .await
            .unwrap()
            .len(),
        1
    );

    repo.mark_checked(
        std::slice::from_ref(&one.host),
        SourceName::DnsCname,
        at(200),
    )
    .await
    .unwrap();
    assert!(
        repo.list_due(SourceName::DnsCname, at(150), 10)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn sources_are_independent(pool: PgPool) {
    let repo = PgCandidateHosts::new(pool);
    let one = host("sso.acme.com", HostOrigin::Import, 0, 0);
    repo.upsert(std::slice::from_ref(&one)).await.unwrap();
    repo.mark_checked(
        std::slice::from_ref(&one.host),
        SourceName::DnsCname,
        at(100),
    )
    .await
    .unwrap();

    assert!(
        repo.list_due(SourceName::DnsCname, at(100), 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repo.list_due(SourceName::HttpProbe, at(100), 10)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn count_due_matches_the_due_listing(pool: PgPool) {
    let repo = PgCandidateHosts::new(pool);
    repo.upsert(&[
        host("a.acme.com", HostOrigin::Import, 1, 1),
        host("b.acme.com", HostOrigin::Import, 1, 1),
    ])
    .await
    .unwrap();
    assert_eq!(
        repo.count_due(SourceName::DnsCname, at(100)).await.unwrap(),
        2
    );
    let checked = Hostname::parse("a.acme.com").unwrap();
    repo.mark_checked(&[checked], SourceName::DnsCname, at(200))
        .await
        .unwrap();
    assert_eq!(
        repo.count_due(SourceName::DnsCname, at(100)).await.unwrap(),
        1
    );
    assert_eq!(
        repo.count_due(SourceName::HttpProbe, at(100))
            .await
            .unwrap(),
        2
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_hosts_wait_for_the_revisit_and_new_hosts_come_first(pool: PgPool) {
    let repo = PgCandidateHosts::new(pool);
    let revisit = std::time::Duration::from_secs(720 * 3600);
    let revisit_delta = chrono::Duration::from_std(revisit).unwrap();
    repo.upsert(&[host("a-failing.acme.com", HostOrigin::Import, 1, 1)])
        .await
        .unwrap();
    let now = at(10_000);
    repo.mark_checked(
        &[Hostname::parse("a-failing.acme.com").unwrap()],
        SourceName::DnsCname,
        now,
    )
    .await
    .unwrap();
    repo.upsert(&[host("z-new.acme.com", HostOrigin::Import, 5, 5)])
        .await
        .unwrap();

    let due = repo
        .list_due(SourceName::DnsCname, now - revisit_delta, 10)
        .await
        .unwrap();
    assert_eq!(names(&due), vec!["z-new.acme.com"]);

    let transient = lodestone_core::transient_checked_at(now, revisit);
    repo.mark_checked(
        &[Hostname::parse("z-new.acme.com").unwrap()],
        SourceName::DnsCname,
        transient,
    )
    .await
    .unwrap();
    let in_two_hours = now + chrono::Duration::hours(2);
    let due = repo
        .list_due(SourceName::DnsCname, in_two_hours - revisit_delta, 10)
        .await
        .unwrap();
    assert_eq!(names(&due), vec!["z-new.acme.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_known_hosts_of_a_domain_are_listed_by_name_and_only_for_that_domain(pool: PgPool) {
    let hosts = PgCandidateHosts::new(pool.clone());
    hosts
        .upsert(&[
            host("sso.acme.com", HostOrigin::Import, 0, 0),
            host("dev.acme.com", HostOrigin::CertificateTransparency, 1, 2),
            host("sso.other.com", HostOrigin::Import, 0, 0),
        ])
        .await
        .unwrap();
    let acme = NormalizedDomain::from_host("acme.com").unwrap();

    let listed = with_tx(&pool, async |shared| {
        PgKnownHostRepository::new(&shared)
            .list_for_domain(&acme)
            .await
    })
    .await
    .unwrap();

    assert_eq!(names(&listed), vec!["dev.acme.com", "sso.acme.com"]);
    assert_eq!(listed[0].origin, HostOrigin::CertificateTransparency);
    assert_eq!(listed[0].last_seen_at, at(2));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_domain_without_hosts_lists_nothing(pool: PgPool) {
    let acme = NormalizedDomain::from_host("acme.com").unwrap();

    let listed = with_tx(&pool, async |shared| {
        PgKnownHostRepository::new(&shared)
            .list_for_domain(&acme)
            .await
    })
    .await
    .unwrap();

    assert!(listed.is_empty());
}
