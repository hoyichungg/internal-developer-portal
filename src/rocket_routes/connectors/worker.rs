use chrono::{DateTime, Utc};
use diesel::sql_types::{BigInt, Bool, Text};
use diesel::{sql_query, QueryableByName};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};
use tokio::sync::watch;

use crate::api::ApiError;
use crate::models::{
    effective_schedule_interval_seconds, ConnectorWorker, ConnectorWorkerHeartbeat, MaintenanceRun,
    NewMaintenanceRun,
};
use crate::repositories::{
    bounded_retry_backoff_seconds, AuditLogRepository, ConnectorConfigRepository,
    ConnectorRepository, ConnectorRunRepository, ConnectorWorkerRepository,
    LoginThrottleRepository, MaintenanceRunRepository, OidcLoginTransactionRepository,
    ServiceHealthCheckRepository, SessionRepository,
};
use crate::rocket_routes::audit_logs::record_system_audit_log;
use crate::rocket_routes::connectors::runtime::{
    configured_connector_run_max_attempts, create_queued_run, execute_leased_connector_run,
};
use crate::rocket_routes::connectors::shared::count_as_i32;

const DEFAULT_HEALTH_RETENTION_DAYS: i64 = 30;
const DEFAULT_RUN_RETENTION_DAYS: i64 = 90;
const DEFAULT_AUDIT_LOG_RETENTION_DAYS: i64 = 365;
const DEFAULT_LOGIN_THROTTLE_RETENTION_DAYS: i64 = 30;
const DEFAULT_RETENTION_CLEANUP_INTERVAL_SECONDS: u64 = 60 * 60;
const RETENTION_FAILURE_RETRY_BASE_SECONDS: i64 = 30;
const RETENTION_FAILURE_RETRY_MAX_SECONDS: i64 = 5 * 60;
const DEFAULT_WORKER_HEARTBEAT_INTERVAL_SECONDS: u64 = 15;
const DEFAULT_RUN_LEASE_SECONDS: u64 = 60;
const DEFAULT_RUN_LEASE_RENEW_INTERVAL_SECONDS: u64 = 15;
const DEFAULT_RUN_RETRY_BASE_SECONDS: u64 = 5;
const DEFAULT_RUN_RETRY_MAX_SECONDS: u64 = 300;
pub(crate) const DEFAULT_WORKER_STALE_AFTER_SECONDS: i64 = 45;

/// Stable PostgreSQL advisory-lock key for retention ownership across workers.
///
/// The value encodes `IDPRET` plus a version byte. Keep it stable so workers
/// running adjacent application versions still serialize the same task.
#[doc(hidden)]
pub const RETENTION_CLEANUP_ADVISORY_LOCK_KEY: i64 = 0x4944_5052_4554_0001;

#[doc(hidden)]
#[derive(Debug, Eq, PartialEq)]
pub enum GuardedRetentionCleanupResult {
    Completed((usize, usize, usize, usize, usize, usize)),
    SkippedLockBusy,
    SkippedRecentlyCompleted,
}

#[derive(Clone, Copy)]
struct ConnectorRetentionPolicy {
    health_retention_days: Option<i64>,
    run_retention_days: Option<i64>,
    audit_log_retention_days: Option<i64>,
    cleanup_interval: StdDuration,
}

struct ConnectorRetentionCleanup {
    health_checks_deleted: usize,
    runs_deleted: usize,
    audit_logs_deleted: usize,
    oidc_transactions_deleted: usize,
    expired_sessions_deleted: usize,
    login_throttle_buckets_deleted: usize,
}

enum ConnectorRetentionCleanupAttempt {
    Completed(ConnectorRetentionCleanup),
    SkippedLockBusy,
    SkippedRecentlyCompleted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RetentionAttemptOutcome {
    Completed,
    SkippedLockBusy,
    SkippedRecentlyCompleted,
    Failed,
}

struct RetentionAttemptSchedule {
    next_attempt_at: Instant,
    consecutive_failures: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkerCycleOperation {
    LeaseRecovery,
    RetentionCleanup,
    Scheduler,
    ConnectorQueue,
}

#[derive(Default)]
struct WorkerCycleState {
    reconnect_required: bool,
}

#[derive(Clone, Copy)]
struct ConnectorRunLeasePolicy {
    lease_seconds: i64,
    renew_interval: StdDuration,
    retry_base_seconds: i64,
    retry_max_seconds: i64,
}

struct RunLeaseRenewal {
    stop: watch::Sender<bool>,
    lease_lost: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

struct ConnectorWorkerLoopConfig {
    database_url: String,
    poll_ms: u64,
    heartbeat_interval_seconds: u64,
    scheduler_enabled: bool,
    retention_policy: ConnectorRetentionPolicy,
    lease_policy: ConnectorRunLeasePolicy,
    worker_id: String,
    worker_started_at: DateTime<Utc>,
}

#[derive(QueryableByName)]
struct CurrentDatabaseName {
    #[diesel(sql_type = Text)]
    database_name: String,
}

#[derive(QueryableByName)]
struct AdvisoryLockAttempt {
    #[diesel(sql_type = Bool)]
    acquired: bool,
}

#[derive(Clone, Copy)]
struct WorkerHeartbeatContext<'a> {
    worker_id: &'a str,
    started_at: DateTime<Utc>,
    scheduler_enabled: bool,
    retention_enabled: bool,
}

impl ConnectorRetentionPolicy {
    fn from_env() -> Self {
        Self {
            health_retention_days: env_retention_days(
                "CONNECTOR_HEALTH_RETENTION_DAYS",
                DEFAULT_HEALTH_RETENTION_DAYS,
            ),
            run_retention_days: env_retention_days(
                "CONNECTOR_RUN_RETENTION_DAYS",
                DEFAULT_RUN_RETENTION_DAYS,
            ),
            audit_log_retention_days: env_retention_days(
                "AUDIT_LOG_RETENTION_DAYS",
                DEFAULT_AUDIT_LOG_RETENTION_DAYS,
            ),
            cleanup_interval: StdDuration::from_secs(env_u64(
                "CONNECTOR_RETENTION_CLEANUP_INTERVAL_SECONDS",
                DEFAULT_RETENTION_CLEANUP_INTERVAL_SECONDS,
            )),
        }
    }

    fn enabled(&self) -> bool {
        // Authentication cleanup is intentionally non-optional. Setting every
        // configurable history retention value to zero disables only those
        // history paths; expired sessions and transient auth state must still
        // remain bounded.
        true
    }
}

impl RetentionAttemptSchedule {
    fn due_immediately(now: Instant) -> Self {
        Self {
            next_attempt_at: now,
            consecutive_failures: 0,
        }
    }

    fn is_due(&self, now: Instant) -> bool {
        now >= self.next_attempt_at
    }

    fn record_outcome(
        &mut self,
        outcome: RetentionAttemptOutcome,
        policy: &ConnectorRetentionPolicy,
        now: Instant,
    ) {
        let delay = match outcome {
            RetentionAttemptOutcome::Completed => {
                self.consecutive_failures = 0;
                policy.cleanup_interval
            }
            RetentionAttemptOutcome::SkippedLockBusy
            | RetentionAttemptOutcome::SkippedRecentlyCompleted => {
                // Another worker either owns or already completed this
                // interval's pass. Advancing a full interval prevents every
                // standby from taking turns running the same cleanup.
                self.consecutive_failures = 0;
                policy.cleanup_interval
            }
            RetentionAttemptOutcome::Failed => {
                self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                StdDuration::from_secs(bounded_retry_backoff_seconds(
                    self.consecutive_failures,
                    RETENTION_FAILURE_RETRY_BASE_SECONDS,
                    RETENTION_FAILURE_RETRY_MAX_SECONDS,
                ) as u64)
            }
        };
        self.next_attempt_at = now + delay;
    }
}

impl WorkerCycleState {
    fn record_failure(&mut self, operation: WorkerCycleOperation) {
        // Retention is an isolated maintenance path. Its transaction has
        // already rolled back, so let scheduler/queue operations prove whether
        // the connection itself is unhealthy before reconnecting.
        if operation != WorkerCycleOperation::RetentionCleanup {
            self.reconnect_required = true;
        }
    }

    fn can_continue_connector_work(&self) -> bool {
        !self.reconnect_required
    }
}

impl ConnectorRunLeasePolicy {
    fn from_env() -> Result<Self, String> {
        let lease_seconds =
            strict_positive_env_u64("CONNECTOR_RUN_LEASE_SECONDS", DEFAULT_RUN_LEASE_SECONDS)?;
        let renew_interval_seconds = strict_positive_env_u64(
            "CONNECTOR_RUN_LEASE_RENEW_INTERVAL_SECONDS",
            DEFAULT_RUN_LEASE_RENEW_INTERVAL_SECONDS,
        )?;
        let retry_base_seconds = strict_positive_env_u64(
            "CONNECTOR_RUN_RETRY_BASE_SECONDS",
            DEFAULT_RUN_RETRY_BASE_SECONDS,
        )?;
        let retry_max_seconds = strict_positive_env_u64(
            "CONNECTOR_RUN_RETRY_MAX_SECONDS",
            DEFAULT_RUN_RETRY_MAX_SECONDS,
        )?;
        let max_attempts = strict_positive_env_u64(
            "CONNECTOR_RUN_MAX_ATTEMPTS",
            configured_connector_run_max_attempts() as u64,
        )?;

        if renew_interval_seconds >= lease_seconds {
            return Err(
                "CONNECTOR_RUN_LEASE_RENEW_INTERVAL_SECONDS must be less than CONNECTOR_RUN_LEASE_SECONDS"
                    .to_owned(),
            );
        }
        if retry_max_seconds < retry_base_seconds {
            return Err(
                "CONNECTOR_RUN_RETRY_MAX_SECONDS must be greater than or equal to CONNECTOR_RUN_RETRY_BASE_SECONDS"
                    .to_owned(),
            );
        }
        if max_attempts > i32::MAX as u64 {
            return Err(
                "CONNECTOR_RUN_MAX_ATTEMPTS must fit in a signed 32-bit integer".to_owned(),
            );
        }

        Ok(Self {
            lease_seconds: lease_seconds as i64,
            renew_interval: StdDuration::from_secs(renew_interval_seconds),
            retry_base_seconds: retry_base_seconds as i64,
            retry_max_seconds: retry_max_seconds as i64,
        })
    }
}

pub fn spawn_connector_background_worker() {
    tokio::spawn(async {
        if let Err(error) = run_connector_worker_forever().await {
            rocket::error!("connector background worker stopped during startup: {error}");
        }
    });
}

pub async fn run_connector_worker_forever() -> Result<(), String> {
    if !strict_env_flag("CONNECTOR_WORKER_ENABLED", true)? {
        rocket::info!("connector background worker is disabled");
        return Ok(());
    }

    crate::config::AppConfig::from_env().map_err(|error| error.to_string())?;

    let database_url = std::env::var("DATABASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "connector worker requires DATABASE_URL when enabled".to_owned())?;

    let poll_ms = env_u64("CONNECTOR_WORKER_POLL_MS", 500);
    let heartbeat_interval_seconds = env_u64(
        "CONNECTOR_WORKER_HEARTBEAT_INTERVAL_SECONDS",
        DEFAULT_WORKER_HEARTBEAT_INTERVAL_SECONDS,
    );
    let scheduler_enabled = env_flag("CONNECTOR_SCHEDULER_ENABLED", true);
    let retention_policy = ConnectorRetentionPolicy::from_env();
    let lease_policy = ConnectorRunLeasePolicy::from_env()?;
    let worker_id = format!("connector-worker-{}", uuid::Uuid::new_v4());
    let worker_started_at = Utc::now();

    rocket::info!("connector background worker started: {}", worker_id);
    rocket::info!(
        "connector run leases enabled: lease={}s, renew={}s, max_attempts={}, retry_backoff={}..{}s",
        lease_policy.lease_seconds,
        lease_policy.renew_interval.as_secs(),
        configured_connector_run_max_attempts(),
        lease_policy.retry_base_seconds,
        lease_policy.retry_max_seconds,
    );
    if retention_policy.enabled() {
        rocket::info!(
            "connector retention enabled: health={:?} days, runs={:?} days, audit_logs={:?} days, interval={}s",
            retention_policy.health_retention_days,
            retention_policy.run_retention_days,
            retention_policy.audit_log_retention_days,
            retention_policy.cleanup_interval.as_secs()
        );
    }
    run_connector_worker_loop(ConnectorWorkerLoopConfig {
        database_url,
        poll_ms,
        heartbeat_interval_seconds,
        scheduler_enabled,
        retention_policy,
        lease_policy,
        worker_id,
        worker_started_at,
    })
    .await;

    Ok(())
}

async fn run_connector_worker_loop(config: ConnectorWorkerLoopConfig) {
    let ConnectorWorkerLoopConfig {
        database_url,
        poll_ms,
        heartbeat_interval_seconds,
        scheduler_enabled,
        retention_policy,
        lease_policy,
        worker_id,
        worker_started_at,
    } = config;
    let poll = StdDuration::from_millis(poll_ms);
    let heartbeat_interval = StdDuration::from_secs(heartbeat_interval_seconds);
    let mut retention_schedule = RetentionAttemptSchedule::due_immediately(Instant::now());

    loop {
        match AsyncPgConnection::establish(&database_url).await {
            Ok(mut db) => {
                rocket::info!("connector background worker connected: {}", worker_id);
                let heartbeat = WorkerHeartbeatContext {
                    worker_id: &worker_id,
                    started_at: worker_started_at,
                    scheduler_enabled,
                    retention_enabled: retention_policy.enabled(),
                };
                let mut last_heartbeat_at =
                    match record_worker_heartbeat(&mut db, heartbeat, "idle", None, None).await {
                        Ok(_) => Some(Instant::now()),
                        Err(error) => {
                            rocket::error!("connector worker heartbeat failed: {:?}", error);
                            None
                        }
                    };

                loop {
                    let mut cycle = WorkerCycleState::default();

                    match ConnectorRunRepository::recover_expired_leases(
                        &mut db,
                        lease_policy.retry_base_seconds,
                        lease_policy.retry_max_seconds,
                        100,
                    )
                    .await
                    {
                        Ok(stats)
                            if stats.requeued > 0 || stats.failed > 0 || stats.cancelled > 0 =>
                        {
                            rocket::warn!(
                                "connector lease recovery: requeued={}, failed={}, cancelled={}",
                                stats.requeued,
                                stats.failed,
                                stats.cancelled
                            );
                        }
                        Ok(_) => {}
                        Err(error) => {
                            rocket::error!("connector lease recovery failed: {:?}", error);
                            cycle.record_failure(WorkerCycleOperation::LeaseRecovery);
                        }
                    }

                    if cycle.can_continue_connector_work()
                        && retention_schedule.is_due(Instant::now())
                    {
                        let cleanup_started_at = Utc::now();
                        if let Err(error) = record_worker_heartbeat(
                            &mut db,
                            heartbeat,
                            "retention_cleanup",
                            None,
                            None,
                        )
                        .await
                        {
                            rocket::warn!(
                                "connector worker heartbeat before retention failed: {:?}",
                                error
                            );
                        }
                        match cleanup_connector_retention(
                            &mut db,
                            &retention_policy,
                            &worker_id,
                            cleanup_started_at,
                        )
                        .await
                        {
                            Ok(ConnectorRetentionCleanupAttempt::Completed(cleanup)) => {
                                retention_schedule.record_outcome(
                                    RetentionAttemptOutcome::Completed,
                                    &retention_policy,
                                    Instant::now(),
                                );
                                match record_worker_heartbeat(
                                    &mut db, heartbeat, "idle", None, None,
                                )
                                .await
                                {
                                    Ok(_) => {
                                        last_heartbeat_at = Some(Instant::now());
                                    }
                                    Err(error) => {
                                        rocket::warn!(
                                            "connector worker heartbeat after retention failed: {:?}",
                                            error
                                        );
                                    }
                                }
                                if cleanup.health_checks_deleted > 0
                                    || cleanup.runs_deleted > 0
                                    || cleanup.audit_logs_deleted > 0
                                    || cleanup.oidc_transactions_deleted > 0
                                    || cleanup.expired_sessions_deleted > 0
                                    || cleanup.login_throttle_buckets_deleted > 0
                                {
                                    rocket::info!(
                                        "connector retention cleanup removed {} health checks, {} runs, {} audit logs, {} expired OIDC transactions, {} expired sessions, and {} stale login-throttle buckets",
                                        cleanup.health_checks_deleted,
                                        cleanup.runs_deleted,
                                        cleanup.audit_logs_deleted,
                                        cleanup.oidc_transactions_deleted,
                                        cleanup.expired_sessions_deleted,
                                        cleanup.login_throttle_buckets_deleted,
                                    );
                                }
                            }
                            Ok(ConnectorRetentionCleanupAttempt::SkippedLockBusy) => {
                                retention_schedule.record_outcome(
                                    RetentionAttemptOutcome::SkippedLockBusy,
                                    &retention_policy,
                                    Instant::now(),
                                );
                                rocket::debug!(
                                    "connector retention cleanup skipped: another worker owns the shared advisory lock"
                                );
                                match record_worker_heartbeat(
                                    &mut db, heartbeat, "idle", None, None,
                                )
                                .await
                                {
                                    Ok(_) => {
                                        last_heartbeat_at = Some(Instant::now());
                                    }
                                    Err(error) => {
                                        rocket::warn!(
                                            "connector worker heartbeat after retention skip failed: {:?}",
                                            error
                                        );
                                    }
                                }
                            }
                            Ok(ConnectorRetentionCleanupAttempt::SkippedRecentlyCompleted) => {
                                retention_schedule.record_outcome(
                                    RetentionAttemptOutcome::SkippedRecentlyCompleted,
                                    &retention_policy,
                                    Instant::now(),
                                );
                                rocket::debug!(
                                    "connector retention cleanup skipped: the shared interval was already completed"
                                );
                                match record_worker_heartbeat(
                                    &mut db, heartbeat, "idle", None, None,
                                )
                                .await
                                {
                                    Ok(_) => {
                                        last_heartbeat_at = Some(Instant::now());
                                    }
                                    Err(error) => {
                                        rocket::warn!(
                                            "connector worker heartbeat after retention skip failed: {:?}",
                                            error
                                        );
                                    }
                                }
                            }
                            Err(error) => {
                                retention_schedule.record_outcome(
                                    RetentionAttemptOutcome::Failed,
                                    &retention_policy,
                                    Instant::now(),
                                );
                                rocket::error!("connector retention cleanup failed: {:?}", error);
                                if let Err(record_error) = record_retention_cleanup_failure(
                                    &mut db,
                                    &worker_id,
                                    cleanup_started_at,
                                    &error,
                                )
                                .await
                                {
                                    rocket::warn!(
                                        "connector retention failure history failed: {:?}",
                                        record_error
                                    );
                                }
                                match record_worker_heartbeat(
                                    &mut db,
                                    heartbeat,
                                    "error",
                                    None,
                                    Some(format!("{:?}", error)),
                                )
                                .await
                                {
                                    Ok(_) => {
                                        last_heartbeat_at = Some(Instant::now());
                                    }
                                    Err(heartbeat_error) => {
                                        rocket::warn!(
                                            "connector worker heartbeat after retention failure failed: {:?}",
                                            heartbeat_error
                                        );
                                    }
                                }
                                cycle.record_failure(WorkerCycleOperation::RetentionCleanup);
                            }
                        }
                    }

                    if cycle.can_continue_connector_work() && scheduler_enabled {
                        if let Err(error) = enqueue_due_scheduled_runs(&mut db).await {
                            rocket::error!("connector scheduler failed: {:?}", error);
                            cycle.record_failure(WorkerCycleOperation::Scheduler);
                        }
                    }

                    if cycle.can_continue_connector_work() {
                        for _ in 0..5 {
                            match process_one_queued_run(
                                &mut db,
                                heartbeat,
                                &database_url,
                                lease_policy,
                            )
                            .await
                            {
                                Ok(true) => {
                                    last_heartbeat_at = Some(Instant::now());
                                }
                                Ok(false) => break,
                                Err(error) => {
                                    rocket::error!("connector worker failed: {:?}", error);
                                    cycle.record_failure(WorkerCycleOperation::ConnectorQueue);
                                    break;
                                }
                            }
                        }
                    }

                    if !cycle.can_continue_connector_work() {
                        break;
                    }

                    if heartbeat_due(last_heartbeat_at, heartbeat_interval) {
                        match record_worker_heartbeat(&mut db, heartbeat, "idle", None, None).await
                        {
                            Ok(_) => {
                                last_heartbeat_at = Some(Instant::now());
                            }
                            Err(error) => {
                                rocket::error!("connector worker heartbeat failed: {:?}", error);
                                break;
                            }
                        }
                    }

                    tokio::time::sleep(poll).await;
                }
            }
            Err(error) => {
                rocket::error!(
                    "connector worker could not connect to database: {:?}",
                    error
                );
            }
        }

        tokio::time::sleep(poll).await;
    }
}

async fn process_one_queued_run(
    db: &mut AsyncPgConnection,
    heartbeat: WorkerHeartbeatContext<'_>,
    database_url: &str,
    lease_policy: ConnectorRunLeasePolicy,
) -> Result<bool, ApiError> {
    let Some(run) = ConnectorRunRepository::claim_next_queued(
        db,
        heartbeat.worker_id,
        lease_policy.lease_seconds,
    )
    .await?
    else {
        return Ok(false);
    };

    let run_id = run.id;
    if let Err(error) = record_worker_heartbeat(db, heartbeat, "running", Some(run_id), None).await
    {
        rocket::warn!(
            "connector worker heartbeat before run {} failed: {:?}",
            run_id,
            error
        );
    }

    let lease_renewal = RunLeaseRenewal::spawn(
        database_url.to_owned(),
        run_id,
        heartbeat.worker_id.to_owned(),
        lease_policy,
    );
    let execution = execute_leased_connector_run(db, run, heartbeat.worker_id).await;
    let lease_lost = lease_renewal.stop().await;

    if let Err(error) = execution {
        let error_message = format!("{:?}", error);
        if lease_lost {
            rocket::warn!(
                "connector worker {} lost lease ownership while processing run {}",
                heartbeat.worker_id,
                run_id
            );
        }
        if let Err(heartbeat_error) =
            record_worker_heartbeat(db, heartbeat, "error", Some(run_id), Some(error_message)).await
        {
            rocket::warn!(
                "connector worker heartbeat after run {} failure failed: {:?}",
                run_id,
                heartbeat_error
            );
        }
        return Err(error);
    }

    record_system_audit_log(
        db,
        "worker_run",
        "connector_run",
        run_id,
        json!({ "worker_id": heartbeat.worker_id }),
    )
    .await?;
    if let Err(error) = record_worker_heartbeat(db, heartbeat, "idle", None, None).await {
        rocket::warn!(
            "connector worker heartbeat after run {} failed: {:?}",
            run_id,
            error
        );
    }

    Ok(true)
}

impl RunLeaseRenewal {
    fn spawn(
        database_url: String,
        run_id: i32,
        worker_id: String,
        policy: ConnectorRunLeasePolicy,
    ) -> Self {
        let (stop, mut stop_rx) = watch::channel(false);
        let lease_lost = Arc::new(AtomicBool::new(false));
        let task_lease_lost = Arc::clone(&lease_lost);
        let task = tokio::spawn(async move {
            let mut last_successful_renewal = Instant::now();
            loop {
                tokio::select! {
                    changed = stop_rx.changed() => {
                        if changed.is_err() || *stop_rx.borrow() {
                            break;
                        }
                    }
                    _ = tokio::time::sleep(policy.renew_interval) => {
                        let renewed = match AsyncPgConnection::establish(&database_url).await {
                            Ok(mut lease_db) => ConnectorRunRepository::renew_lease(
                                &mut lease_db,
                                run_id,
                                &worker_id,
                                policy.lease_seconds,
                            )
                            .await,
                            Err(error) => {
                                rocket::warn!(
                                    "connector run {} lease heartbeat could not connect: {:?}",
                                    run_id,
                                    error
                                );
                                if last_successful_renewal.elapsed()
                                    >= StdDuration::from_secs(policy.lease_seconds as u64)
                                {
                                    task_lease_lost.store(true, Ordering::Release);
                                    break;
                                }
                                continue;
                            }
                        };

                        match renewed {
                            Ok(true) => last_successful_renewal = Instant::now(),
                            Ok(false) => {
                                task_lease_lost.store(true, Ordering::Release);
                                break;
                            }
                            Err(error) => {
                                rocket::warn!(
                                    "connector run {} lease heartbeat failed: {:?}",
                                    run_id,
                                    error
                                );
                                if last_successful_renewal.elapsed()
                                    >= StdDuration::from_secs(policy.lease_seconds as u64)
                                {
                                    task_lease_lost.store(true, Ordering::Release);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        });

        Self {
            stop,
            lease_lost,
            task,
        }
    }

    async fn stop(self) -> bool {
        let _ = self.stop.send(true);
        if self.task.await.is_err() {
            self.lease_lost.store(true, Ordering::Release);
        }
        AtomicBool::load(self.lease_lost.as_ref(), Ordering::Acquire)
    }
}

fn heartbeat_due(last_heartbeat_at: Option<Instant>, heartbeat_interval: StdDuration) -> bool {
    last_heartbeat_at
        .map(|last_heartbeat_at| last_heartbeat_at.elapsed() >= heartbeat_interval)
        .unwrap_or(true)
}

async fn record_worker_heartbeat(
    db: &mut AsyncPgConnection,
    heartbeat: WorkerHeartbeatContext<'_>,
    status: &str,
    current_run_id: Option<i32>,
    last_error: Option<String>,
) -> Result<ConnectorWorker, ApiError> {
    ConnectorWorkerRepository::upsert_heartbeat(
        db,
        ConnectorWorkerHeartbeat {
            worker_id: heartbeat.worker_id.to_owned(),
            status: status.to_owned(),
            scheduler_enabled: heartbeat.scheduler_enabled,
            retention_enabled: heartbeat.retention_enabled,
            current_run_id,
            last_error,
            started_at: heartbeat.started_at,
        },
    )
    .await
    .map_err(ApiError::from)
}

async fn cleanup_connector_retention(
    db: &mut AsyncPgConnection,
    policy: &ConnectorRetentionPolicy,
    worker_id: &str,
    started_at: DateTime<Utc>,
) -> Result<ConnectorRetentionCleanupAttempt, ApiError> {
    let policy = *policy;
    let worker_id = worker_id.to_owned();

    db.transaction::<ConnectorRetentionCleanupAttempt, ApiError, _>(|conn| {
        Box::pin(async move {
            // This must be the first statement in the cleanup transaction.
            // PostgreSQL advisory locks coordinate every worker session on the
            // portal database, while the xact form guarantees ownership is
            // released on both commit and rollback.
            let lock = sql_query("SELECT pg_try_advisory_xact_lock($1) AS acquired")
                .bind::<BigInt, _>(RETENTION_CLEANUP_ADVISORY_LOCK_KEY)
                .get_result::<AdvisoryLockAttempt>(conn)
                .await?;
            if !lock.acquired {
                // A skip is expected multi-worker contention, not maintenance.
                // Do not add a maintenance_runs row: only the owning worker
                // records success/failure, keeping operator history meaningful.
                return Ok(ConnectorRetentionCleanupAttempt::SkippedLockBusy);
            }

            let now = Utc::now();
            let cleanup_interval_seconds =
                i64::try_from(policy.cleanup_interval.as_secs()).unwrap_or(i64::MAX);
            let latest_success =
                MaintenanceRunRepository::find_latest_success(conn, "retention_cleanup").await?;
            if latest_success.is_some_and(|latest| {
                now.signed_duration_since(latest.finished_at).num_seconds()
                    < cleanup_interval_seconds
            }) {
                // The lock prevents concurrent work; this shared success check
                // prevents sequential duplicate passes from workers whose local
                // timers drift by a few milliseconds.
                return Ok(ConnectorRetentionCleanupAttempt::SkippedRecentlyCompleted);
            }

            let health_checks_deleted = match policy.health_retention_days {
                Some(days) => {
                    let cutoff = now - chrono::Duration::days(days);
                    ServiceHealthCheckRepository::delete_older_than(conn, cutoff).await?
                }
                None => 0,
            };
            let runs_deleted = match policy.run_retention_days {
                Some(days) => {
                    let cutoff = now - chrono::Duration::days(days);
                    ConnectorRunRepository::delete_finished_older_than(conn, cutoff).await?
                }
                None => 0,
            };
            let audit_logs_deleted = match policy.audit_log_retention_days {
                Some(days) => {
                    let cutoff = now - chrono::Duration::days(days);
                    AuditLogRepository::delete_older_than(conn, cutoff).await?
                }
                None => 0,
            };
            let oidc_transactions_deleted =
                OidcLoginTransactionRepository::delete_expired(conn, now).await?;
            let expired_sessions_deleted = SessionRepository::delete_expired(conn, now).await?;
            let login_throttle_buckets_deleted = LoginThrottleRepository::prune_before(
                conn,
                now - chrono::Duration::days(DEFAULT_LOGIN_THROTTLE_RETENTION_DAYS),
            )
            .await?;
            let finished_at = Utc::now();
            let duration_ms = (finished_at - started_at).num_milliseconds().max(0);

            MaintenanceRunRepository::create(
                conn,
                NewMaintenanceRun {
                    task: "retention_cleanup".to_owned(),
                    status: "success".to_owned(),
                    worker_id: Some(worker_id),
                    started_at,
                    finished_at,
                    duration_ms,
                    health_checks_deleted: count_as_i32(health_checks_deleted),
                    connector_runs_deleted: count_as_i32(runs_deleted),
                    audit_logs_deleted: count_as_i32(audit_logs_deleted),
                    error_message: None,
                },
            )
            .await?;

            Ok(ConnectorRetentionCleanupAttempt::Completed(
                ConnectorRetentionCleanup {
                    health_checks_deleted,
                    runs_deleted,
                    audit_logs_deleted,
                    oidc_transactions_deleted,
                    expired_sessions_deleted,
                    login_throttle_buckets_deleted,
                },
            ))
        })
    })
    .await
}

/// Runs one retention pass for database integration coverage.
///
/// This deliberately refuses to execute unless both `APP_ENV=test` and the
/// actual PostgreSQL `current_database()` name contain a standalone `test`
/// segment. Keeping the guard inside this function ensures it runs on the same
/// connection immediately before the destructive cleanup.
#[doc(hidden)]
pub async fn run_guarded_retention_cleanup_for_test(
    db: &mut AsyncPgConnection,
    health_retention_days: Option<i64>,
    run_retention_days: Option<i64>,
    audit_log_retention_days: Option<i64>,
    worker_id: &str,
) -> Result<GuardedRetentionCleanupResult, String> {
    ensure_safe_test_database(db, "retention test cleanup").await?;

    let policy = ConnectorRetentionPolicy {
        health_retention_days,
        run_retention_days,
        audit_log_retention_days,
        cleanup_interval: StdDuration::from_secs(DEFAULT_RETENTION_CLEANUP_INTERVAL_SECONDS),
    };
    let attempt = cleanup_connector_retention(db, &policy, worker_id, Utc::now())
        .await
        .map_err(|error| format!("retention test cleanup failed: {error:?}"))?;

    Ok(match attempt {
        ConnectorRetentionCleanupAttempt::Completed(cleanup) => {
            GuardedRetentionCleanupResult::Completed((
                cleanup.health_checks_deleted,
                cleanup.runs_deleted,
                cleanup.audit_logs_deleted,
                cleanup.oidc_transactions_deleted,
                cleanup.expired_sessions_deleted,
                cleanup.login_throttle_buckets_deleted,
            ))
        }
        ConnectorRetentionCleanupAttempt::SkippedLockBusy => {
            GuardedRetentionCleanupResult::SkippedLockBusy
        }
        ConnectorRetentionCleanupAttempt::SkippedRecentlyCompleted => {
            GuardedRetentionCleanupResult::SkippedRecentlyCompleted
        }
    })
}

/// Claims one queued run using the production repository semantics.
/// Available only for guarded database integration coverage.
#[doc(hidden)]
pub async fn claim_connector_run_for_test(
    db: &mut AsyncPgConnection,
    worker_id: &str,
    lease_seconds: i64,
) -> Result<Option<i32>, String> {
    ensure_safe_test_database(db, "connector lease claim test").await?;
    ConnectorRunRepository::claim_next_queued(db, worker_id, lease_seconds)
        .await
        .map(|run| run.map(|run| run.id))
        .map_err(|error| format!("connector lease claim test failed: {error}"))
}

/// Recovers expired leases using the production repository semantics.
/// Returns `(requeued, failed, cancelled)` for guarded integration coverage.
#[doc(hidden)]
pub async fn recover_connector_runs_for_test(
    db: &mut AsyncPgConnection,
    retry_base_seconds: i64,
    retry_max_seconds: i64,
) -> Result<(usize, usize, usize), String> {
    ensure_safe_test_database(db, "connector lease recovery test").await?;
    let stats = ConnectorRunRepository::recover_expired_leases(
        db,
        retry_base_seconds,
        retry_max_seconds,
        100,
    )
    .await
    .map_err(|error| format!("connector lease recovery test failed: {error}"))?;
    Ok((stats.requeued, stats.failed, stats.cancelled))
}

/// Requests cancellation using the production repository state transition.
/// Returns the resulting run status for guarded integration coverage.
#[doc(hidden)]
pub async fn request_connector_run_cancel_for_test(
    db: &mut AsyncPgConnection,
    run_id: i32,
) -> Result<Option<String>, String> {
    ensure_safe_test_database(db, "connector cancellation test").await?;
    ConnectorRunRepository::request_cancel(db, run_id)
        .await
        .map(|run| run.map(|run| run.status))
        .map_err(|error| format!("connector cancellation test failed: {error}"))
}

async fn ensure_safe_test_database(
    db: &mut AsyncPgConnection,
    operation: &str,
) -> Result<(), String> {
    if !matches!(std::env::var("APP_ENV").as_deref(), Ok("test")) {
        return Err(format!(
            "refusing {operation}: APP_ENV must be exactly 'test'"
        ));
    }

    let database_name = sql_query("SELECT current_database()::text AS database_name")
        .get_result::<CurrentDatabaseName>(db)
        .await
        .map_err(|error| format!("failed to read current_database(): {error}"))?
        .database_name;
    if !is_safe_test_database_name(&database_name) {
        return Err(format!(
            "refusing {operation}: database '{database_name}' must contain a standalone 'test' segment"
        ));
    }

    Ok(())
}

fn is_safe_test_database_name(database_name: &str) -> bool {
    database_name
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|segment| segment == "test")
}

async fn record_retention_cleanup_failure(
    db: &mut AsyncPgConnection,
    worker_id: &str,
    started_at: DateTime<Utc>,
    error: &ApiError,
) -> Result<MaintenanceRun, ApiError> {
    let finished_at = Utc::now();
    let duration_ms = (finished_at - started_at).num_milliseconds().max(0);

    MaintenanceRunRepository::create(
        db,
        NewMaintenanceRun {
            task: "retention_cleanup".to_owned(),
            status: "failed".to_owned(),
            worker_id: Some(worker_id.to_owned()),
            started_at,
            finished_at,
            duration_ms,
            health_checks_deleted: 0,
            connector_runs_deleted: 0,
            audit_logs_deleted: 0,
            error_message: Some(format!("{:?}", error)),
        },
    )
    .await
    .map_err(ApiError::from)
}

async fn enqueue_due_scheduled_runs(db: &mut AsyncPgConnection) -> Result<usize, ApiError> {
    db.transaction::<usize, ApiError, _>(|conn| {
        Box::pin(async move { enqueue_due_scheduled_runs_locked(conn).await })
    })
    .await
}

async fn enqueue_due_scheduled_runs_locked(db: &mut AsyncPgConnection) -> Result<usize, ApiError> {
    let now = Utc::now();
    let configs = ConnectorConfigRepository::find_due_for_schedule(db, now, 10).await?;
    let mut enqueued = 0;

    for config in configs {
        let Some(schedule_cron) = config.schedule_cron.as_deref() else {
            continue;
        };
        let Some(interval_seconds) = effective_schedule_interval_seconds(schedule_cron) else {
            continue;
        };
        // Older databases may contain sub-minute schedules created before the
        // API enforced a safe minimum. Clamp them here so an old config cannot
        // flood run history and audit logs while an operator updates it.

        match ConnectorRepository::find_by_source(db, &config.source).await {
            Ok(connector) if connector.status == "paused" || !config.enabled => {
                ConnectorConfigRepository::mark_scheduled(
                    db,
                    &config.source,
                    now,
                    interval_seconds,
                    None,
                )
                .await?;
            }
            Ok(_) => {
                if ConnectorRunRepository::has_pending(db, &config.source, &config.target).await? {
                    continue;
                }

                let run = create_queued_run(db, &config.source, &config.target, "scheduled", None)
                    .await?;

                ConnectorConfigRepository::mark_scheduled(
                    db,
                    &config.source,
                    now,
                    interval_seconds,
                    Some(run.id),
                )
                .await?;
                enqueued += 1;
            }
            Err(diesel::result::Error::NotFound) => {
                ConnectorConfigRepository::mark_scheduled(
                    db,
                    &config.source,
                    now,
                    interval_seconds,
                    None,
                )
                .await?;
            }
            Err(error) => return Err(ApiError::from(error)),
        }
    }

    Ok(enqueued)
}

fn env_flag(name: &str, default: bool) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn strict_positive_env_u64(name: &str, default: u64) -> Result<u64, String> {
    match std::env::var(name) {
        Ok(value) => value
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| format!("{name} must be a positive integer")),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!("{name} must contain valid Unicode")),
    }
}

pub(crate) fn connector_worker_stale_after_seconds() -> i64 {
    env_u64(
        "CONNECTOR_WORKER_STALE_AFTER_SECONDS",
        DEFAULT_WORKER_STALE_AFTER_SECONDS as u64,
    ) as i64
}

fn env_retention_days(name: &str, default: i64) -> Option<i64> {
    let days = std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value >= 0)
        .unwrap_or(default);

    if days == 0 {
        None
    } else {
        Some(days)
    }
}

fn strict_env_flag(name: &str, default: bool) -> Result<bool, String> {
    match std::env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Ok(true),
            "0" | "false" | "no" => Ok(false),
            _ => Err(format!("{name} must be one of: true, false, 1, 0, yes, no")),
        },
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!("{name} must contain valid Unicode")),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        is_safe_test_database_name, ConnectorRetentionPolicy, RetentionAttemptOutcome,
        RetentionAttemptSchedule, WorkerCycleOperation, WorkerCycleState,
    };
    use crate::repositories::bounded_retry_backoff_seconds;
    use std::time::{Duration as StdDuration, Instant};

    fn retention_policy() -> ConnectorRetentionPolicy {
        ConnectorRetentionPolicy {
            health_retention_days: Some(30),
            run_retention_days: Some(90),
            audit_log_retention_days: Some(365),
            cleanup_interval: StdDuration::from_secs(60 * 60),
        }
    }

    #[test]
    fn retention_test_database_name_requires_standalone_test_segment() {
        assert!(is_safe_test_database_name("portal_retention_test"));
        assert!(is_safe_test_database_name("test-portal"));
        assert!(is_safe_test_database_name("TEST"));
        assert!(!is_safe_test_database_name("app_db"));
        assert!(!is_safe_test_database_name("production"));
        assert!(!is_safe_test_database_name("contest"));
    }

    #[test]
    fn connector_retry_backoff_is_exponential_and_bounded() {
        assert_eq!(bounded_retry_backoff_seconds(1, 5, 300), 5);
        assert_eq!(bounded_retry_backoff_seconds(2, 5, 300), 10);
        assert_eq!(bounded_retry_backoff_seconds(3, 5, 300), 20);
        assert_eq!(bounded_retry_backoff_seconds(20, 5, 300), 300);
        assert_eq!(bounded_retry_backoff_seconds(0, 0, 0), 1);
    }

    #[test]
    fn retention_schedule_delays_lock_contention_without_marking_failure() {
        let policy = retention_policy();
        let now = Instant::now();
        let mut schedule = RetentionAttemptSchedule::due_immediately(now);

        assert!(schedule.is_due(now));
        schedule.record_outcome(RetentionAttemptOutcome::SkippedLockBusy, &policy, now);

        assert_eq!(schedule.consecutive_failures, 0);
        assert_eq!(
            schedule.next_attempt_at.duration_since(now),
            policy.cleanup_interval
        );
        assert!(!schedule.is_due(now + StdDuration::from_secs(1)));
        assert!(schedule.is_due(schedule.next_attempt_at));

        let next_attempt = schedule.next_attempt_at;
        schedule.record_outcome(
            RetentionAttemptOutcome::SkippedRecentlyCompleted,
            &policy,
            next_attempt,
        );
        assert_eq!(schedule.consecutive_failures, 0);
        assert_eq!(
            schedule.next_attempt_at.duration_since(next_attempt),
            policy.cleanup_interval
        );
    }

    #[test]
    fn retention_failure_retry_is_exponential_bounded_and_resets_on_success() {
        let policy = retention_policy();
        let mut now = Instant::now();
        let mut schedule = RetentionAttemptSchedule::due_immediately(now);
        let expected_delays = [30, 60, 120, 240, 300, 300];

        for expected_delay in expected_delays {
            schedule.record_outcome(RetentionAttemptOutcome::Failed, &policy, now);
            assert_eq!(
                schedule.next_attempt_at.duration_since(now),
                StdDuration::from_secs(expected_delay)
            );
            assert!(!schedule.is_due(now + StdDuration::from_secs(1)));
            now = schedule.next_attempt_at;
        }
        schedule.record_outcome(RetentionAttemptOutcome::Completed, &policy, now);
        assert_eq!(schedule.consecutive_failures, 0);
        assert_eq!(
            schedule.next_attempt_at.duration_since(now),
            policy.cleanup_interval
        );
    }

    #[test]
    fn retention_failure_does_not_block_scheduler_or_connector_queue() {
        let mut cycle = WorkerCycleState::default();
        cycle.record_failure(WorkerCycleOperation::RetentionCleanup);
        assert!(cycle.can_continue_connector_work());

        for operation in [
            WorkerCycleOperation::LeaseRecovery,
            WorkerCycleOperation::Scheduler,
            WorkerCycleOperation::ConnectorQueue,
        ] {
            let mut cycle = WorkerCycleState::default();
            cycle.record_failure(operation);
            assert!(!cycle.can_continue_connector_work());
        }
    }
}
