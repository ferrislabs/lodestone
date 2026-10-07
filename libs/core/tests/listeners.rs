mod support;

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_core::{
    ActiveProfile, CandidateHost, CandidateHosts, Clock, HostOrigin, Hostname, ListenerState,
    LodestoneUseCase, Observation, Source, SourceError, SourceName, StepOutcome,
    definition::Agent,
    infrastructure::{candidate_host::PgCandidateHosts, listener::postgres::PgListenerLock},
    listeners::{Listener, ListenerSettings, ListenerStep},
    lock_key,
    sources::SourceRunError,
};
use sqlx::PgPool;
use support::{at, create_profile};
use tokio::{sync::watch, time::sleep};

fn settings(idle: Duration) -> ListenerSettings {
    ListenerSettings {
        idle,
        lock_retry: Duration::from_millis(40),
        yield_pause: Duration::from_millis(1),
        backoff_base: Duration::from_millis(30),
        backoff_max: Duration::from_millis(120),
        shutdown_grace: Duration::from_secs(2),
    }
}

struct ScriptedStep {
    name: &'static str,
    active: AtomicBool,
    blocked_by: Option<&'static str>,
    runs: Arc<AtomicU64>,
    script: Mutex<VecDeque<Result<StepOutcome, String>>>,
}

impl ScriptedStep {
    fn new(name: &'static str, script: Vec<Result<StepOutcome, String>>) -> Self {
        Self {
            name,
            active: AtomicBool::new(true),
            blocked_by: None,
            runs: Arc::new(AtomicU64::new(0)),
            script: Mutex::new(script.into()),
        }
    }
}

impl ListenerStep for ScriptedStep {
    fn name(&self) -> &str {
        self.name
    }

    async fn is_active(&self) -> Result<bool, CoreError> {
        Ok(self.active.load(Ordering::SeqCst))
    }

    async fn blocking_provider(&self) -> Result<Option<&'static str>, CoreError> {
        Ok(self.blocked_by)
    }

    async fn run_once(&self) -> Result<StepOutcome, SourceRunError> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        let next = self.script.lock().unwrap().pop_front();
        match next {
            Some(Ok(outcome)) => Ok(outcome),
            Some(Err(message)) => Err(SourceError::Upstream(message).into()),
            None => Ok(StepOutcome {
                backlog: Some(0),
                ..StepOutcome::default()
            }),
        }
    }
}

fn spawn<S: ListenerStep + 'static>(
    listener: Listener<S>,
    shutdown: &watch::Sender<bool>,
) -> tokio::task::JoinHandle<()> {
    let receiver = shutdown.subscribe();
    tokio::spawn(async move { listener.run(receiver).await })
}

async fn eventually<F, Fut>(mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    for _ in 0..200 {
        if check().await {
            return;
        }
        sleep(Duration::from_millis(25)).await;
    }
    panic!("condition not reached in time");
}

async fn status(pool: &PgPool, name: &str) -> Option<lodestone_core::ListenerStatus> {
    LodestoneUseCase::new(pool.clone())
        .list_listener_statuses()
        .await
        .unwrap()
        .into_iter()
        .find(|status| status.name == name)
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_pass_updates_the_status_row(pool: PgPool) {
    let step = ScriptedStep::new(
        "dns_cname",
        vec![
            Ok(StepOutcome {
                processed: 3,
                signals: 2,
                backlog: Some(7),
            }),
            Ok(StepOutcome {
                processed: 2,
                signals: 1,
                backlog: Some(0),
            }),
        ],
    );
    let (shutdown, _) = watch::channel(false);
    let handle = spawn(
        Listener::new(
            step,
            pool.clone(),
            "node-a".to_owned(),
            settings(Duration::from_millis(40)),
        ),
        &shutdown,
    );

    eventually(|| async {
        status(&pool, "dns_cname")
            .await
            .is_some_and(|s| s.items_total == 5 && s.state == ListenerState::Idle)
    })
    .await;
    let row = status(&pool, "dns_cname").await.unwrap();
    assert_eq!(row.holder.as_deref(), Some("node-a"));
    assert_eq!(row.signals_total, 3);
    assert_eq!(row.backlog, Some(0));
    assert!(row.last_activity_at.is_some());
    assert_eq!(row.last_error, None);

    shutdown.send_replace(true);
    handle.await.unwrap();
    assert_eq!(
        status(&pool, "dns_cname").await.unwrap().state,
        ListenerState::Stopped
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_busy_pass_loops_again_without_waiting_for_the_idle_sleep(pool: PgPool) {
    let script = (0..5)
        .map(|_| {
            Ok(StepOutcome {
                processed: 1,
                signals: 0,
                backlog: None,
            })
        })
        .collect();
    let step = ScriptedStep::new("http_probe", script);
    let runs = Arc::clone(&step.runs);
    let (shutdown, _) = watch::channel(false);
    let handle = spawn(
        Listener::new(
            step,
            pool.clone(),
            "node-a".to_owned(),
            settings(Duration::from_secs(3600)),
        ),
        &shutdown,
    );

    eventually(|| async { runs.load(Ordering::SeqCst) >= 6 }).await;
    assert_eq!(status(&pool, "http_probe").await.unwrap().items_total, 5);

    shutdown.send_replace(true);
    handle.await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failing_step_reports_the_error_and_backs_off(pool: PgPool) {
    let step = ScriptedStep::new(
        "security_txt",
        vec![Err("boom".to_owned()), Err("boom again".to_owned())],
    );
    let runs = Arc::clone(&step.runs);
    let (shutdown, _) = watch::channel(false);
    let handle = spawn(
        Listener::new(
            step,
            pool.clone(),
            "node-a".to_owned(),
            settings(Duration::from_millis(40)),
        ),
        &shutdown,
    );

    eventually(|| async {
        status(&pool, "security_txt")
            .await
            .is_some_and(|s| s.state == ListenerState::Error)
    })
    .await;
    let row = status(&pool, "security_txt").await.unwrap();
    assert!(row.last_error.unwrap().contains("boom"));

    eventually(|| async { runs.load(Ordering::SeqCst) >= 3 }).await;
    eventually(|| async {
        status(&pool, "security_txt")
            .await
            .is_some_and(|s| s.state == ListenerState::Idle)
    })
    .await;
    assert!(
        status(&pool, "security_txt")
            .await
            .unwrap()
            .last_error
            .is_some()
    );

    shutdown.send_replace(true);
    handle.await.unwrap();
}

#[test]
fn backoff_doubles_and_is_capped() {
    let settings = ListenerSettings {
        backoff_base: Duration::from_secs(5),
        backoff_max: Duration::from_secs(300),
        ..settings(Duration::from_secs(15))
    };
    assert_eq!(settings.backoff(1), Duration::from_secs(5));
    assert_eq!(settings.backoff(2), Duration::from_secs(10));
    assert_eq!(settings.backoff(3), Duration::from_secs(20));
    assert_eq!(settings.backoff(10), Duration::from_secs(300));
    assert_eq!(settings.backoff(u32::MAX), Duration::from_secs(300));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_listener_without_work_to_listen_for_is_idle_no_agent(pool: PgPool) {
    let step = ScriptedStep::new("certificate_transparency", Vec::new());
    step.active.store(false, Ordering::SeqCst);
    let runs = Arc::clone(&step.runs);
    let (shutdown, _) = watch::channel(false);
    let handle = spawn(
        Listener::new(
            step,
            pool.clone(),
            "node-a".to_owned(),
            settings(Duration::from_millis(40)),
        ),
        &shutdown,
    );

    eventually(|| async {
        status(&pool, "certificate_transparency")
            .await
            .is_some_and(|s| s.state == ListenerState::IdleNoAgent)
    })
    .await;
    assert_eq!(runs.load(Ordering::SeqCst), 0);

    shutdown.send_replace(true);
    handle.await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_listener_whose_provider_is_not_ready_never_runs_its_step(pool: PgPool) {
    let mut step = ScriptedStep::new("dns_cname", Vec::new());
    step.blocked_by = Some("dns_cname");
    let runs = Arc::clone(&step.runs);
    let (shutdown, _) = watch::channel(false);
    let handle = spawn(
        Listener::new(
            step,
            pool.clone(),
            "node-a".to_owned(),
            settings(Duration::from_millis(40)),
        ),
        &shutdown,
    );

    eventually(|| async {
        status(&pool, "dns_cname")
            .await
            .is_some_and(|s| s.state == ListenerState::Idle)
    })
    .await;
    sleep(Duration::from_millis(150)).await;
    assert_eq!(runs.load(Ordering::SeqCst), 0);

    shutdown.send_replace(true);
    handle.await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_replica_holds_a_listener_and_another_takes_over_when_it_stops(pool: PgPool) {
    let first = ScriptedStep::new("role_guess", Vec::new());
    let second = ScriptedStep::new("role_guess", Vec::new());
    let first_runs = Arc::clone(&first.runs);
    let second_runs = Arc::clone(&second.runs);
    let (first_stop, _) = watch::channel(false);
    let (second_stop, _) = watch::channel(false);

    let first_handle = spawn(
        Listener::new(
            first,
            pool.clone(),
            "node-a".to_owned(),
            settings(Duration::from_millis(40)),
        ),
        &first_stop,
    );
    eventually(|| async { first_runs.load(Ordering::SeqCst) > 0 }).await;
    let second_handle = spawn(
        Listener::new(
            second,
            pool.clone(),
            "node-b".to_owned(),
            settings(Duration::from_millis(40)),
        ),
        &second_stop,
    );

    sleep(Duration::from_millis(300)).await;
    assert_eq!(second_runs.load(Ordering::SeqCst), 0);
    assert_eq!(
        status(&pool, "role_guess").await.unwrap().holder.as_deref(),
        Some("node-a")
    );

    first_stop.send_replace(true);
    first_handle.await.unwrap();
    eventually(|| async { second_runs.load(Ordering::SeqCst) > 0 }).await;
    assert_eq!(
        status(&pool, "role_guess").await.unwrap().holder.as_deref(),
        Some("node-b")
    );

    second_stop.send_replace(true);
    second_handle.await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn shutdown_releases_the_advisory_lock(pool: PgPool) {
    let step = ScriptedStep::new("public_page", Vec::new());
    let runs = Arc::clone(&step.runs);
    let (shutdown, _) = watch::channel(false);
    let handle = spawn(
        Listener::new(
            step,
            pool.clone(),
            "node-a".to_owned(),
            settings(Duration::from_millis(40)),
        ),
        &shutdown,
    );
    eventually(|| async { runs.load(Ordering::SeqCst) > 0 }).await;

    let mut contender = PgListenerLock::connect(&pool, lock_key("public_page"))
        .await
        .unwrap();
    assert!(!contender.try_acquire().await.unwrap());

    shutdown.send_replace(true);
    handle.await.unwrap();

    assert!(contender.try_acquire().await.unwrap());
    contender.release().await;
}

struct DueHostsStep {
    hosts: PgCandidateHosts,
    handled: Arc<AtomicU64>,
}

impl ListenerStep for DueHostsStep {
    fn name(&self) -> &str {
        "dns_cname"
    }

    async fn is_active(&self) -> Result<bool, CoreError> {
        Ok(true)
    }

    async fn run_once(&self) -> Result<StepOutcome, SourceRunError> {
        let due = self.hosts.list_due(SourceName::DnsCname, at(0), 1).await?;
        let names: Vec<Hostname> = due.iter().map(|host| host.host.clone()).collect();
        self.hosts
            .mark_checked(&names, SourceName::DnsCname, at(10))
            .await?;
        self.handled.fetch_add(due.len() as u64, Ordering::SeqCst);
        Ok(StepOutcome {
            processed: due.len() as u64,
            signals: 0,
            backlog: None,
        })
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn newly_discovered_hosts_are_handled_without_waiting_for_an_interval(pool: PgPool) {
    let hosts = PgCandidateHosts::new(pool.clone());
    let handled = Arc::new(AtomicU64::new(0));
    let (shutdown, _) = watch::channel(false);
    let handle = spawn(
        Listener::new(
            DueHostsStep {
                hosts: hosts.clone(),
                handled: Arc::clone(&handled),
            },
            pool.clone(),
            "node-a".to_owned(),
            settings(Duration::from_millis(60)),
        ),
        &shutdown,
    );
    sleep(Duration::from_millis(150)).await;
    assert_eq!(handled.load(Ordering::SeqCst), 0);

    let fresh: Vec<CandidateHost> = ["sso.acme.com", "auth.beta.io", "id.gamma.fr"]
        .into_iter()
        .map(|name| {
            CandidateHost::discovered(
                Hostname::parse(name).unwrap(),
                HostOrigin::CertificateTransparency,
                at(5),
            )
            .unwrap()
        })
        .collect();
    hosts.upsert(&fresh).await.unwrap();

    eventually(|| async { handled.load(Ordering::SeqCst) == 3 }).await;
    eventually(|| async {
        status(&pool, "dns_cname")
            .await
            .is_some_and(|row| row.items_total == 3)
    })
    .await;

    shutdown.send_replace(true);
    handle.await.unwrap();
}

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

struct BatchSource {
    hosts: Mutex<Vec<&'static str>>,
}

impl Source for BatchSource {
    type Item = &'static str;

    fn name(&self) -> SourceName {
        SourceName::HttpProbe
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Self::Item>, SourceError> {
        Ok(std::mem::take(&mut *self.hosts.lock().unwrap()))
    }

    fn evaluate(&self, _: &Self::Item, _: &ActiveProfile, _: &Agent) -> Vec<Observation> {
        Vec::new()
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_empty_listening_pass_records_no_run(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let usecase = LodestoneUseCase::new(pool.clone());
    usecase.set_profile_active(profile, true).await.unwrap();
    let source = BatchSource {
        hosts: Mutex::new(Vec::new()),
    };

    let idle = usecase.listen_source(&source, &FixedClock).await.unwrap();
    assert!(idle.runs.is_empty());
    assert!(
        usecase
            .list_source_schedule()
            .await
            .unwrap()
            .runs
            .is_empty()
    );

    source.hosts.lock().unwrap().push("sso.acme.com");
    let busy = usecase.listen_source(&source, &FixedClock).await.unwrap();
    assert_eq!(busy.items, 1);
    assert_eq!(busy.runs.len(), 1);
    let runs = usecase.list_source_schedule().await.unwrap().runs;
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].volume, 1);

    let idle_again = usecase.listen_source(&source, &FixedClock).await.unwrap();
    assert!(idle_again.runs.is_empty());
    assert_eq!(usecase.list_source_schedule().await.unwrap().runs.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_source_listens_only_while_an_enabled_agent_exists(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    assert!(
        !usecase
            .has_enabled_agent(SourceName::HttpProbe)
            .await
            .unwrap()
    );
    let profile = create_profile(&pool, "alpha").await;
    assert!(
        !usecase
            .has_enabled_agent(SourceName::HttpProbe)
            .await
            .unwrap()
    );
    usecase.set_profile_active(profile, true).await.unwrap();
    assert!(
        usecase
            .has_enabled_agent(SourceName::HttpProbe)
            .await
            .unwrap()
    );
    assert!(
        !usecase
            .has_enabled_agent(SourceName::CertificateTransparency)
            .await
            .unwrap()
    );
}

struct SlowStep {
    finish: Arc<tokio::sync::Notify>,
}

impl ListenerStep for SlowStep {
    fn name(&self) -> &str {
        "certificate_transparency"
    }

    async fn is_active(&self) -> Result<bool, CoreError> {
        Ok(true)
    }

    async fn run_once(&self) -> Result<StepOutcome, SourceRunError> {
        self.finish.notified().await;
        Ok(StepOutcome::default())
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_long_step_keeps_the_status_fresh_with_a_heartbeat(pool: PgPool) {
    let finish = Arc::new(tokio::sync::Notify::new());
    let (shutdown, _) = watch::channel(false);
    let handle = spawn(
        Listener::new(
            SlowStep {
                finish: Arc::clone(&finish),
            },
            pool.clone(),
            "node-a".to_owned(),
            settings(Duration::from_millis(40)),
        ),
        &shutdown,
    );
    let usecase = LodestoneUseCase::new(pool.clone());
    sqlx::query(
        "INSERT INTO listeners (name, holder, state, updated_at)
         VALUES ('certificate_transparency', 'node-a', 'listening', now() - interval '1 hour')",
    )
    .execute(&pool)
    .await
    .unwrap();

    eventually(|| async {
        let row = status(&pool, "certificate_transparency").await.unwrap();
        Utc::now() - row.updated_at < chrono::Duration::seconds(5)
    })
    .await;
    assert_eq!(
        usecase.list_listener_statuses().await.unwrap()[0].state,
        ListenerState::Listening
    );

    finish.notify_one();
    shutdown.send_replace(true);
    handle.await.unwrap();
}

struct BlockingStep {
    name: &'static str,
    release: Arc<tokio::sync::Notify>,
}

impl ListenerStep for BlockingStep {
    fn name(&self) -> &str {
        self.name
    }

    async fn is_active(&self) -> Result<bool, CoreError> {
        Ok(true)
    }

    async fn run_once(&self) -> Result<StepOutcome, SourceRunError> {
        self.release.notified().await;
        Ok(StepOutcome {
            processed: 1,
            signals: 0,
            backlog: Some(0),
        })
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_listener_is_listed_while_its_first_pass_is_still_running(pool: PgPool) {
    let release = Arc::new(tokio::sync::Notify::new());
    let step = BlockingStep {
        name: "public_page",
        release: Arc::clone(&release),
    };
    let (shutdown, _) = watch::channel(false);
    let handle = spawn(
        Listener::new(
            step,
            pool.clone(),
            "node-a".to_owned(),
            settings(Duration::from_secs(3600)),
        ),
        &shutdown,
    );

    eventually(|| async {
        status(&pool, "public_page")
            .await
            .is_some_and(|s| s.state == ListenerState::Listening && s.items_total == 0)
    })
    .await;
    let row = status(&pool, "public_page").await.unwrap();
    assert_eq!(row.holder.as_deref(), Some("node-a"));

    release.notify_one();
    eventually(|| async {
        status(&pool, "public_page")
            .await
            .is_some_and(|s| s.items_total == 1)
    })
    .await;

    shutdown.send_replace(true);
    release.notify_one();
    handle.await.unwrap();
}
