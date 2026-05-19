use chrono::{NaiveDateTime, Utc};
use diesel_async::{AsyncConnection, AsyncPgConnection};
use serde_json::json;
use std::time::{Duration as StdDuration, Instant};

use crate::api::ApiError;
use crate::models::{
    schedule_interval_seconds, ConnectorWorker, ConnectorWorkerHeartbeat, MaintenanceRun,
    NewMaintenanceRun,
};
use crate::repositories::{
    AuditLogRepository, ConnectorConfigRepository, ConnectorRepository, ConnectorRunRepository,
    ConnectorWorkerRepository, MaintenanceRunRepository, ServiceHealthCheckRepository,
};
use crate::rocket_routes::audit_logs::record_system_audit_log;
use crate::rocket_routes::connectors::runtime::{create_queued_run, execute_claimed_connector_run};
use crate::rocket_routes::connectors::shared::count_as_i32;

const DEFAULT_HEALTH_RETENTION_DAYS: i64 = 30;
const DEFAULT_RUN_RETENTION_DAYS: i64 = 90;
const DEFAULT_AUDIT_LOG_RETENTION_DAYS: i64 = 365;
const DEFAULT_RETENTION_CLEANUP_INTERVAL_SECONDS: u64 = 60 * 60;
const DEFAULT_WORKER_HEARTBEAT_INTERVAL_SECONDS: u64 = 15;
pub(crate) const DEFAULT_WORKER_STALE_AFTER_SECONDS: i64 = 45;

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
}

#[derive(Clone, Copy)]
struct WorkerHeartbeatContext<'a> {
    worker_id: &'a str,
    started_at: NaiveDateTime,
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
        self.health_retention_days.is_some()
            || self.run_retention_days.is_some()
            || self.audit_log_retention_days.is_some()
    }
}

pub fn spawn_connector_background_worker() {
    tokio::spawn(run_connector_worker_forever());
}

pub async fn run_connector_worker_forever() {
    if !env_flag("CONNECTOR_WORKER_ENABLED", true) {
        rocket::info!("connector background worker is disabled");
        return;
    }

    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        rocket::warn!("connector background worker disabled: DATABASE_URL is not set");
        return;
    };

    let poll_ms = env_u64("CONNECTOR_WORKER_POLL_MS", 500);
    let heartbeat_interval_seconds = env_u64(
        "CONNECTOR_WORKER_HEARTBEAT_INTERVAL_SECONDS",
        DEFAULT_WORKER_HEARTBEAT_INTERVAL_SECONDS,
    );
    let scheduler_enabled = env_flag("CONNECTOR_SCHEDULER_ENABLED", true);
    let retention_policy = ConnectorRetentionPolicy::from_env();
    let worker_id = format!("connector-worker-{}", uuid::Uuid::new_v4());
    let worker_started_at = Utc::now().naive_utc();

    rocket::info!("connector background worker started: {}", worker_id);
    if retention_policy.enabled() {
        rocket::info!(
            "connector retention enabled: health={:?} days, runs={:?} days, audit_logs={:?} days, interval={}s",
            retention_policy.health_retention_days,
            retention_policy.run_retention_days,
            retention_policy.audit_log_retention_days,
            retention_policy.cleanup_interval.as_secs()
        );
    }
    run_connector_worker_loop(
        database_url,
        poll_ms,
        heartbeat_interval_seconds,
        scheduler_enabled,
        retention_policy,
        worker_id,
        worker_started_at,
    )
    .await;
}

async fn run_connector_worker_loop(
    database_url: String,
    poll_ms: u64,
    heartbeat_interval_seconds: u64,
    scheduler_enabled: bool,
    retention_policy: ConnectorRetentionPolicy,
    worker_id: String,
    worker_started_at: NaiveDateTime,
) {
    let poll = StdDuration::from_millis(poll_ms);
    let heartbeat_interval = StdDuration::from_secs(heartbeat_interval_seconds);
    let mut last_retention_cleanup_at: Option<Instant> = None;

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
                    let mut reconnect = false;

                    if retention_cleanup_due(&retention_policy, last_retention_cleanup_at) {
                        let cleanup_started_at = Utc::now().naive_utc();
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
                            Ok(cleanup) => {
                                last_retention_cleanup_at = Some(Instant::now());
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
                                {
                                    rocket::info!(
                                        "connector retention cleanup removed {} health checks, {} runs, and {} audit logs",
                                        cleanup.health_checks_deleted,
                                        cleanup.runs_deleted,
                                        cleanup.audit_logs_deleted
                                    );
                                }
                            }
                            Err(error) => {
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
                                if let Err(heartbeat_error) = record_worker_heartbeat(
                                    &mut db,
                                    heartbeat,
                                    "error",
                                    None,
                                    Some(format!("{:?}", error)),
                                )
                                .await
                                {
                                    rocket::warn!(
                                        "connector worker heartbeat after retention failure failed: {:?}",
                                        heartbeat_error
                                    );
                                }
                                reconnect = true;
                            }
                        }
                    }

                    if !reconnect && scheduler_enabled {
                        if let Err(error) = enqueue_due_scheduled_runs(&mut db).await {
                            rocket::error!("connector scheduler failed: {:?}", error);
                            break;
                        }
                    }

                    if !reconnect {
                        for _ in 0..5 {
                            match process_one_queued_run(&mut db, heartbeat).await {
                                Ok(true) => {
                                    last_heartbeat_at = Some(Instant::now());
                                }
                                Ok(false) => break,
                                Err(error) => {
                                    rocket::error!("connector worker failed: {:?}", error);
                                    reconnect = true;
                                    break;
                                }
                            }
                        }
                    }

                    if reconnect {
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
) -> Result<bool, ApiError> {
    let Some(run) = ConnectorRunRepository::claim_next_queued(db, heartbeat.worker_id).await?
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

    if let Err(error) = execute_claimed_connector_run(db, run).await {
        let error_message = format!("{:?}", error);
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

fn retention_cleanup_due(
    policy: &ConnectorRetentionPolicy,
    last_cleanup_at: Option<Instant>,
) -> bool {
    policy.enabled()
        && last_cleanup_at
            .map(|last_cleanup_at| last_cleanup_at.elapsed() >= policy.cleanup_interval)
            .unwrap_or(true)
}

async fn cleanup_connector_retention(
    db: &mut AsyncPgConnection,
    policy: &ConnectorRetentionPolicy,
    worker_id: &str,
    started_at: NaiveDateTime,
) -> Result<ConnectorRetentionCleanup, ApiError> {
    let policy = *policy;
    let worker_id = worker_id.to_owned();

    db.transaction::<ConnectorRetentionCleanup, ApiError, _>(|conn| {
        Box::pin(async move {
            let now = Utc::now().naive_utc();
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
            let finished_at = Utc::now().naive_utc();
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

            Ok(ConnectorRetentionCleanup {
                health_checks_deleted,
                runs_deleted,
                audit_logs_deleted,
            })
        })
    })
    .await
}

async fn record_retention_cleanup_failure(
    db: &mut AsyncPgConnection,
    worker_id: &str,
    started_at: NaiveDateTime,
    error: &ApiError,
) -> Result<MaintenanceRun, ApiError> {
    let finished_at = Utc::now().naive_utc();
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
    let now = Utc::now().naive_utc();
    let configs = ConnectorConfigRepository::find_due_for_schedule(db, now, 10).await?;
    let mut enqueued = 0;

    for config in configs {
        let Some(schedule_cron) = config.schedule_cron.as_deref() else {
            continue;
        };
        let Some(interval_seconds) = schedule_interval_seconds(schedule_cron) else {
            continue;
        };

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ConnectorRun, NewAuditLog, NewConnectorRun, NewMaintainer, NewService,
        NewServiceHealthCheck,
    };
    use crate::repositories::{MaintainerRepository, MaintenanceRunRepository, ServiceRepository};
    use crate::schema::{audit_logs, connector_runs, maintenance_runs};
    use diesel::prelude::*;
    use diesel_async::RunQueryDsl;

    #[tokio::test]
    async fn retention_cleanup_deletes_old_health_checks_finished_runs_and_audit_logs() {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for retention test");
        let mut db = AsyncPgConnection::establish(&database_url)
            .await
            .expect("test database should be reachable");
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let source = format!("retention_{suffix}");
        let stale_at = Utc::now().naive_utc() - chrono::Duration::days(120);
        let old_at = Utc::now().naive_utc() - chrono::Duration::days(45);
        let fresh_at = Utc::now().naive_utc();
        let maintainer = MaintainerRepository::create(
            &mut db,
            NewMaintainer {
                display_name: format!("Retention {suffix}"),
                email: format!("retention-{suffix}@example.test"),
            },
        )
        .await
        .expect("maintainer should be created");
        let service = ServiceRepository::create(
            &mut db,
            NewService {
                source: source.clone(),
                external_id: Some("retention-service".to_owned()),
                maintainer_id: maintainer.id,
                slug: format!("retention-{suffix}"),
                name: "Retention Service".to_owned(),
                lifecycle_status: "active".to_owned(),
                health_status: "healthy".to_owned(),
                description: None,
                repository_url: None,
                dashboard_url: None,
                runbook_url: None,
                last_checked_at: Some(fresh_at),
            },
        )
        .await
        .expect("service should be created");
        let old_run = create_finished_retention_run(&mut db, &source, old_at).await;
        let stale_run = create_finished_retention_run(&mut db, &source, stale_at).await;
        let fresh_run = create_finished_retention_run(&mut db, &source, fresh_at).await;
        ServiceHealthCheckRepository::create(
            &mut db,
            NewServiceHealthCheck {
                service_id: service.id,
                connector_run_id: Some(old_run.id),
                source: source.clone(),
                external_id: Some("retention-service".to_owned()),
                health_status: "healthy".to_owned(),
                previous_health_status: None,
                checked_at: old_at,
                response_time_ms: None,
                message: None,
                raw_payload: None,
            },
        )
        .await
        .expect("old health check should be created");
        let fresh_check = ServiceHealthCheckRepository::create(
            &mut db,
            NewServiceHealthCheck {
                service_id: service.id,
                connector_run_id: Some(fresh_run.id),
                source: source.clone(),
                external_id: Some("retention-service".to_owned()),
                health_status: "healthy".to_owned(),
                previous_health_status: None,
                checked_at: fresh_at,
                response_time_ms: None,
                message: None,
                raw_payload: None,
            },
        )
        .await
        .expect("fresh health check should be created");
        let old_audit_log = AuditLogRepository::create(
            &mut db,
            NewAuditLog {
                actor_user_id: None,
                action: "retention_old".to_owned(),
                resource_type: "retention".to_owned(),
                resource_id: Some(format!("old-{suffix}")),
                metadata: None,
            },
        )
        .await
        .expect("old audit log should be created");
        diesel::update(audit_logs::table.find(old_audit_log.id))
            .set(audit_logs::created_at.eq(old_at))
            .execute(&mut db)
            .await
            .expect("old audit log should be aged");
        let fresh_audit_log = AuditLogRepository::create(
            &mut db,
            NewAuditLog {
                actor_user_id: None,
                action: "retention_fresh".to_owned(),
                resource_type: "retention".to_owned(),
                resource_id: Some(format!("fresh-{suffix}")),
                metadata: None,
            },
        )
        .await
        .expect("fresh audit log should be created");
        let policy = ConnectorRetentionPolicy {
            health_retention_days: Some(30),
            run_retention_days: Some(90),
            audit_log_retention_days: Some(30),
            cleanup_interval: StdDuration::from_secs(3600),
        };

        let cleanup = cleanup_connector_retention(
            &mut db,
            &policy,
            "retention-test-worker",
            Utc::now().naive_utc(),
        )
        .await
        .expect("retention cleanup should run");

        assert_eq!(cleanup.health_checks_deleted, 1);
        assert_eq!(cleanup.runs_deleted, 1);
        assert_eq!(cleanup.audit_logs_deleted, 1);
        let maintenance_run =
            MaintenanceRunRepository::find_recent(&mut db, 5, Some("retention_cleanup"))
                .await
                .expect("maintenance run history should load")
                .into_iter()
                .find(|run| run.worker_id.as_deref() == Some("retention-test-worker"))
                .expect("retention cleanup should write maintenance history");
        assert_eq!(maintenance_run.status, "success");
        assert_eq!(maintenance_run.health_checks_deleted, 1);
        assert_eq!(maintenance_run.connector_runs_deleted, 1);
        assert_eq!(maintenance_run.audit_logs_deleted, 1);
        assert!(ConnectorRunRepository::find(&mut db, old_run.id)
            .await
            .is_ok());
        assert!(matches!(
            ConnectorRunRepository::find(&mut db, stale_run.id).await,
            Err(diesel::result::Error::NotFound)
        ));
        assert!(ConnectorRunRepository::find(&mut db, fresh_run.id)
            .await
            .is_ok());
        assert!(matches!(
            audit_logs::table
                .find(old_audit_log.id)
                .select(audit_logs::id)
                .first::<i32>(&mut db)
                .await,
            Err(diesel::result::Error::NotFound)
        ));
        assert!(audit_logs::table
            .find(fresh_audit_log.id)
            .select(audit_logs::id)
            .first::<i32>(&mut db)
            .await
            .is_ok());
        let fresh_checks = ServiceHealthCheckRepository::find_by_run(&mut db, fresh_run.id)
            .await
            .expect("fresh run checks should load");
        assert!(fresh_checks.iter().any(|check| check.id == fresh_check.id));

        ServiceRepository::delete(&mut db, service.id)
            .await
            .expect("service should be cleaned up");
        diesel::delete(connector_runs::table.find(old_run.id))
            .execute(&mut db)
            .await
            .expect("old run should be cleaned up");
        diesel::delete(connector_runs::table.find(fresh_run.id))
            .execute(&mut db)
            .await
            .expect("fresh run should be cleaned up");
        diesel::delete(audit_logs::table.find(fresh_audit_log.id))
            .execute(&mut db)
            .await
            .expect("fresh audit log should be cleaned up");
        diesel::delete(maintenance_runs::table.find(maintenance_run.id))
            .execute(&mut db)
            .await
            .expect("maintenance run should be cleaned up");
        MaintainerRepository::delete(&mut db, maintainer.id)
            .await
            .expect("maintainer should be cleaned up");
    }

    async fn create_finished_retention_run(
        db: &mut AsyncPgConnection,
        source: &str,
        finished_at: NaiveDateTime,
    ) -> ConnectorRun {
        ConnectorRunRepository::create(
            db,
            NewConnectorRun {
                source: source.to_owned(),
                target: "service_health".to_owned(),
                status: "success".to_owned(),
                success_count: 1,
                failure_count: 0,
                duration_ms: 1,
                error_message: None,
                started_at: finished_at,
                finished_at: Some(finished_at),
                trigger: "manual".to_owned(),
                payload: None,
                claimed_at: None,
                worker_id: None,
            },
        )
        .await
        .expect("retention test run should be created")
    }
}
