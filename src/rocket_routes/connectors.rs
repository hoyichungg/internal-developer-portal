use chrono::{DateTime, NaiveDateTime, Utc};
use diesel_async::{AsyncConnection, AsyncPgConnection};
use rocket::serde::json::Json;
use rocket::serde::{Deserialize, Serialize};
use rocket_db_pools::Connection;
use serde_json::{json, Value};
use std::time::{Duration as StdDuration, Instant};
use utoipa::ToSchema;

use crate::api::{created, ok, ApiError, ApiResult, CreatedApiResult};
use crate::auth::{require_admin, AuthenticatedUser};
use crate::connector_adapters::fetch_connector_payload;
use crate::crypto::{
    decrypt_connector_config, encrypt_connector_config, preserve_redacted_connector_config,
    redact_connector_config, sanitized_json_snapshot,
};
use crate::models::{
    schedule_interval_seconds, Connector, ConnectorConfig, ConnectorConfigUpdate, ConnectorRun,
    ConnectorRunItem, ConnectorRunItemError, ConnectorRunStateUpdate, ConnectorUpdate,
    ConnectorWorker, ConnectorWorkerHeartbeat, MaintenanceRun, NewConnector, NewConnectorRun,
    NewConnectorRunItem, NewConnectorRunItemError, NewMaintenanceRun, NewNotification, NewService,
    NewWorkCard, ServiceHealthCheck,
};
use crate::repositories::{
    AuditLogRepository, ConnectorConfigRepository, ConnectorRepository,
    ConnectorRunItemErrorRepository, ConnectorRunItemRepository, ConnectorRunRepository,
    ConnectorWorkerRepository, MaintenanceRunRepository, NotificationRepository,
    ServiceHealthCheckRepository, ServiceRepository, WorkCardRepository,
};
use crate::rocket_routes::audit_logs::{record_audit_log, record_system_audit_log};
use crate::rocket_routes::DbConn;
use crate::validation::{validate_request, FieldViolation, Validate};

fn default_run_mode() -> String {
    "execute".to_owned()
}

const DEFAULT_HEALTH_RETENTION_DAYS: i64 = 30;
const DEFAULT_RUN_RETENTION_DAYS: i64 = 90;
const DEFAULT_AUDIT_LOG_RETENTION_DAYS: i64 = 365;
const DEFAULT_RETENTION_CLEANUP_INTERVAL_SECONDS: u64 = 60 * 60;
const DEFAULT_WORKER_HEARTBEAT_INTERVAL_SECONDS: u64 = 15;
pub(crate) const DEFAULT_WORKER_STALE_AFTER_SECONDS: i64 = 45;

#[derive(Serialize, ToSchema)]
pub struct ConnectorRunExecutionResponse {
    pub source: String,
    pub target: String,
    pub imported: usize,
    pub failed: usize,
    pub run: ConnectorRun,
    pub data: Vec<Value>,
    pub items: Vec<ConnectorRunItem>,
    pub errors: Vec<ConnectorImportError>,
    pub item_errors: Vec<ConnectorRunItemError>,
}

#[derive(Serialize, ToSchema)]
pub struct ConnectorRunDetail {
    pub run: ConnectorRun,
    pub items: Vec<ConnectorRunItem>,
    pub item_errors: Vec<ConnectorRunItemError>,
    pub health_checks: Vec<ServiceHealthCheck>,
}

#[derive(Serialize, ToSchema)]
pub struct ConnectorOperationsResponse {
    pub stale_after_seconds: i64,
    pub workers: Vec<ConnectorWorkerStatus>,
    pub maintenance_runs: Vec<MaintenanceRun>,
}

#[derive(Serialize, ToSchema)]
pub struct ConnectorWorkerStatus {
    pub id: i32,
    pub worker_id: String,
    pub status: String,
    pub scheduler_enabled: bool,
    pub retention_enabled: bool,
    pub current_run_id: Option<i32>,
    pub last_error: Option<String>,
    pub started_at: NaiveDateTime,
    pub last_seen_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub seconds_since_last_seen: i64,
    pub is_stale: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ConnectorConfigResponse {
    pub id: i32,
    pub source: String,
    pub target: String,
    pub enabled: bool,
    pub schedule_cron: Option<String>,
    pub config: String,
    pub sample_payload: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub last_scheduled_at: Option<NaiveDateTime>,
    pub next_run_at: Option<NaiveDateTime>,
    pub last_scheduled_run_id: Option<i32>,
}

#[derive(Serialize, ToSchema)]
pub struct ConnectorImportError {
    pub external_id: Option<String>,
    pub message: String,
    pub raw_item: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct ManualConnectorRunRequest {
    #[serde(default = "default_run_mode")]
    pub mode: String,
    pub target: Option<String>,
    pub payload: Option<Value>,
}

impl Validate for ManualConnectorRunRequest {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        crate::validation::required(&mut errors, "mode", &self.mode);
        crate::validation::max_len(&mut errors, "mode", &self.mode, 32);
        crate::validation::one_of(&mut errors, "mode", &self.mode, &["execute", "queue"]);

        if let Some(target) = &self.target {
            crate::validation::max_len(&mut errors, "target", target, 64);
            crate::validation::one_of(
                &mut errors,
                "target",
                target,
                &["service_health", "work_cards", "notifications"],
            );
        }

        errors
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct WorkCardImportRequest {
    /// Work card records to import. Items are upserted by `(source, external_id)`.
    pub items: Vec<WorkCardImportItem>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct WorkCardImportItem {
    /// Stable id from the external work tracking system.
    pub external_id: String,
    /// Work item title displayed on the dashboard.
    pub title: String,
    /// Supported values: `todo`, `in_progress`, `blocked`, `done`.
    pub status: String,
    /// Supported values: `low`, `medium`, `high`, `urgent`.
    pub priority: String,
    /// Optional assignee display name or email.
    pub assignee: Option<String>,
    /// Optional due date/time in local NaiveDateTime JSON format.
    pub due_at: Option<NaiveDateTime>,
    /// Optional absolute URL back to the source system.
    pub url: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct NotificationImportRequest {
    /// Notification records to import. Items are upserted by `(source, external_id)`.
    pub items: Vec<NotificationImportItem>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct NotificationImportItem {
    /// Stable id from the external notification source.
    pub external_id: String,
    /// Notification title displayed in the portal.
    pub title: String,
    /// Optional notification body.
    pub body: Option<String>,
    /// Supported values: `info`, `warning`, `critical`.
    pub severity: String,
    /// Initial read state.
    pub is_read: bool,
    /// Optional absolute URL back to the source system.
    pub url: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct ServiceHealthImportRequest {
    /// Service health records to import. Each item upserts a service and appends a health check.
    pub items: Vec<ServiceHealthImportItem>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct ServiceHealthImportItem {
    /// Stable id from the external monitoring source.
    pub external_id: String,
    /// Existing maintainer/team id that owns the imported service.
    pub maintainer_id: i32,
    /// Stable portal service slug.
    pub slug: String,
    /// Human-friendly service name.
    pub name: String,
    /// Supported values: `active`, `deprecated`, `archived`.
    pub lifecycle_status: String,
    /// Supported values: `healthy`, `degraded`, `down`, `unknown`.
    pub health_status: String,
    /// Optional service description.
    pub description: Option<String>,
    /// Optional absolute repository URL.
    pub repository_url: Option<String>,
    /// Optional absolute dashboard URL.
    pub dashboard_url: Option<String>,
    /// Optional absolute runbook URL.
    pub runbook_url: Option<String>,
    /// Optional source check time. RFC3339 strings and `%Y-%m-%d %H:%M:%S` are accepted.
    pub last_checked_at: Option<String>,
}

struct ConnectorExecution {
    data: Vec<Value>,
    items: Vec<ConnectorRunItemDraft>,
    errors: Vec<ConnectorImportError>,
}

struct ConnectorRunItemDraft {
    external_id: Option<String>,
    record_id: Option<i32>,
    status: &'static str,
    snapshot: Option<String>,
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

impl From<ConnectorConfig> for ConnectorConfigResponse {
    fn from(config: ConnectorConfig) -> Self {
        Self {
            id: config.id,
            source: config.source,
            target: config.target,
            enabled: config.enabled,
            schedule_cron: config.schedule_cron,
            config: redact_connector_config(&config.config),
            sample_payload: config.sample_payload,
            created_at: config.created_at,
            updated_at: config.updated_at,
            last_scheduled_at: config.last_scheduled_at,
            next_run_at: config.next_run_at,
            last_scheduled_run_id: config.last_scheduled_run_id,
        }
    }
}

impl ConnectorWorkerStatus {
    fn from_worker(worker: ConnectorWorker, now: NaiveDateTime, stale_after_seconds: i64) -> Self {
        let seconds_since_last_seen = (now - worker.last_seen_at).num_seconds().max(0);

        Self {
            id: worker.id,
            worker_id: worker.worker_id,
            status: worker.status,
            scheduler_enabled: worker.scheduler_enabled,
            retention_enabled: worker.retention_enabled,
            current_run_id: worker.current_run_id,
            last_error: worker.last_error,
            started_at: worker.started_at,
            last_seen_at: worker.last_seen_at,
            updated_at: worker.updated_at,
            seconds_since_last_seen,
            is_stale: seconds_since_last_seen > stale_after_seconds,
        }
    }
}

#[rocket::get("/connectors")]
pub async fn get_connectors(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
) -> ApiResult<Vec<Connector>> {
    let connectors = ConnectorRepository::find_multiple(&mut db, 100).await?;

    ok(connectors)
}

#[rocket::get("/connectors/operations")]
pub async fn get_connector_operations(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
) -> ApiResult<ConnectorOperationsResponse> {
    require_admin(&auth)?;
    let now = Utc::now().naive_utc();
    let stale_after_seconds = connector_worker_stale_after_seconds();
    let workers = ConnectorWorkerRepository::find_recent(&mut db, 20)
        .await?
        .into_iter()
        .map(|worker| ConnectorWorkerStatus::from_worker(worker, now, stale_after_seconds))
        .collect();
    let maintenance_runs =
        MaintenanceRunRepository::find_recent(&mut db, 20, Some("retention_cleanup")).await?;

    ok(ConnectorOperationsResponse {
        stale_after_seconds,
        workers,
        maintenance_runs,
    })
}

#[rocket::get("/connectors/<source>")]
pub async fn view_connector(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
    source: String,
) -> ApiResult<Connector> {
    let source = validate_source(source)?;
    let connector = ConnectorRepository::find_by_source(&mut db, &source).await?;

    ok(connector)
}

#[rocket::post("/connectors", format = "json", data = "<connector>")]
pub async fn create_connector(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    connector: Json<NewConnector>,
) -> CreatedApiResult<Connector> {
    require_admin(&auth)?;
    let connector = validate_request(connector.into_inner())?;
    let connector = ConnectorRepository::create(&mut db, connector).await?;
    record_audit_log(
        &mut db,
        &auth,
        "create",
        "connector",
        &connector.source,
        json!({
            "kind": &connector.kind,
            "status": &connector.status,
        }),
    )
    .await?;

    created(connector)
}

#[rocket::put("/connectors/<source>", format = "json", data = "<connector>")]
pub async fn update_connector(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    source: String,
    connector: Json<ConnectorUpdate>,
) -> ApiResult<Connector> {
    require_admin(&auth)?;
    let source = validate_source(source)?;
    let connector = validate_request(connector.into_inner())?;
    let connector = ConnectorRepository::update_by_source(&mut db, &source, connector).await?;
    record_audit_log(
        &mut db,
        &auth,
        "update",
        "connector",
        &connector.source,
        json!({
            "kind": &connector.kind,
            "status": &connector.status,
        }),
    )
    .await?;

    ok(connector)
}

#[rocket::delete("/connectors/<source>")]
pub async fn delete_connector(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    source: String,
) -> Result<rocket::response::status::NoContent, ApiError> {
    require_admin(&auth)?;
    let source = validate_source(source)?;
    ConnectorRepository::delete_by_source(&mut db, &source).await?;
    record_audit_log(&mut db, &auth, "delete", "connector", &source, json!({})).await?;

    Ok(rocket::response::status::NoContent)
}

#[rocket::get("/connectors/<source>/config", rank = 2)]
pub async fn get_connector_config(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    source: String,
) -> ApiResult<ConnectorConfigResponse> {
    require_admin(&auth)?;
    let source = validate_source(source)?;
    let config = ConnectorConfigRepository::find_by_source(&mut db, &source).await?;

    ok(ConnectorConfigResponse::from(config))
}

#[rocket::put("/connectors/<source>/config", format = "json", data = "<config>")]
pub async fn upsert_connector_config(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    source: String,
    config: Json<ConnectorConfigUpdate>,
) -> ApiResult<ConnectorConfigResponse> {
    require_admin(&auth)?;
    let source = validate_source(source)?;
    ConnectorRepository::find_by_source(&mut db, &source).await?;
    let existing_config = match ConnectorConfigRepository::find_by_source(&mut db, &source).await {
        Ok(config) => Some(config),
        Err(diesel::result::Error::NotFound) => None,
        Err(error) => return Err(error.into()),
    };
    let mut config = validate_request(config.into_inner())?;
    config.config = preserve_redacted_connector_config(
        &config.config,
        existing_config
            .as_ref()
            .map(|existing_config| existing_config.config.as_str()),
    )
    .map_err(|error| validation_error_dynamic("config", error))?;
    config.config = encrypt_connector_config(&config.config)
        .map_err(|error| validation_error_dynamic("config", error))?;
    let config = ConnectorConfigRepository::upsert_by_source(&mut db, &source, config).await?;
    record_audit_log(
        &mut db,
        &auth,
        "upsert_config",
        "connector",
        &source,
        json!({
            "target": &config.target,
            "enabled": config.enabled,
            "schedule_cron": &config.schedule_cron,
        }),
    )
    .await?;

    ok(ConnectorConfigResponse::from(config))
}

#[rocket::get("/connectors/runs?<source>&<target>")]
pub async fn get_connector_runs(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    source: Option<String>,
    target: Option<String>,
) -> ApiResult<Vec<ConnectorRun>> {
    require_admin(&auth)?;
    let source = match source {
        Some(source) => Some(validate_source(source)?),
        None => None,
    };
    let target = match target {
        Some(target) => Some(validate_target(target)?),
        None => None,
    };
    let runs =
        ConnectorRunRepository::find_multiple(&mut db, 100, source.as_deref(), target.as_deref())
            .await?;

    ok(runs)
}

#[rocket::get("/connectors/runs/<id>", rank = 1)]
pub async fn get_connector_run(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
) -> ApiResult<ConnectorRunDetail> {
    require_admin(&auth)?;
    let run = ConnectorRunRepository::find(&mut db, id).await?;
    let items = ConnectorRunItemRepository::find_by_run(&mut db, id).await?;
    let item_errors = ConnectorRunItemErrorRepository::find_by_run(&mut db, id).await?;
    let health_checks = ServiceHealthCheckRepository::find_by_run(&mut db, id).await?;

    ok(ConnectorRunDetail {
        run,
        items,
        item_errors,
        health_checks,
    })
}

#[rocket::post("/connectors/runs/<id>/retry")]
pub async fn retry_connector_run(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
) -> CreatedApiResult<ConnectorRunExecutionResponse> {
    require_admin(&auth)?;
    let original = ConnectorRunRepository::find(&mut db, id).await?;

    if !matches!(original.status.as_str(), "failed" | "partial_success") {
        return Err(validation_error(
            "status",
            "must be failed or partial_success",
        ));
    }

    let connector = ConnectorRepository::find_by_source(&mut db, &original.source).await?;
    if connector.status == "paused" {
        return Err(validation_error("status", "is paused"));
    }

    if original.payload.is_none() {
        let config = ConnectorConfigRepository::find_by_source(&mut db, &original.source).await?;
        if !config.enabled {
            return Err(validation_error("enabled", "must be true"));
        }
    }

    let run = create_queued_run(
        &mut db,
        &original.source,
        &original.target,
        "retry",
        original.payload.clone(),
    )
    .await?;
    record_audit_log(
        &mut db,
        &auth,
        "retry",
        "connector_run",
        run.id,
        json!({
            "source": &run.source,
            "target": &run.target,
            "original_run_id": original.id,
        }),
    )
    .await?;

    created(ConnectorRunExecutionResponse {
        source: run.source.clone(),
        target: run.target.clone(),
        imported: 0,
        failed: 0,
        run,
        data: Vec::new(),
        items: Vec::new(),
        errors: Vec::new(),
        item_errors: Vec::new(),
    })
}

#[rocket::post("/connectors/<source>/runs", format = "json", data = "<request>")]
pub async fn run_connector(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    source: String,
    request: Json<ManualConnectorRunRequest>,
) -> CreatedApiResult<ConnectorRunExecutionResponse> {
    require_admin(&auth)?;
    let source = validate_source(source)?;
    let connector = ConnectorRepository::find_by_source(&mut db, &source).await?;
    let config = ConnectorConfigRepository::find_by_source(&mut db, &source).await?;
    let request = validate_request(request.into_inner())?;
    let target = match request.target {
        Some(target) => validate_target(target)?,
        None => config.target.clone(),
    };

    if connector.status == "paused" {
        return Err(validation_error("status", "is paused"));
    }

    if !config.enabled {
        return Err(validation_error("enabled", "must be true"));
    }

    if request.mode == "queue" {
        let run = create_queued_run(
            &mut db,
            &source,
            &target,
            "manual",
            request.payload.map(|payload| payload.to_string()),
        )
        .await?;
        record_audit_log(
            &mut db,
            &auth,
            "queue_run",
            "connector_run",
            run.id,
            json!({
                "source": &source,
                "target": &target,
                "trigger": "manual",
            }),
        )
        .await?;
        return created(ConnectorRunExecutionResponse {
            source,
            target,
            imported: 0,
            failed: 0,
            run,
            data: Vec::new(),
            items: Vec::new(),
            errors: Vec::new(),
            item_errors: Vec::new(),
        });
    }

    let run = create_running_run(
        &mut db,
        &source,
        &target,
        "manual",
        request.payload.map(|payload| payload.to_string()),
    )
    .await?;
    let response = execute_claimed_connector_run(&mut db, run).await?;
    record_audit_log(
        &mut db,
        &auth,
        "run",
        "connector_run",
        response.run.id,
        json!({
            "source": &response.source,
            "target": &response.target,
            "status": &response.run.status,
            "success_count": response.run.success_count,
            "failure_count": response.run.failure_count,
        }),
    )
    .await?;

    created(response)
}

#[rocket::post(
    "/connectors/<source>/work-cards/import",
    format = "json",
    data = "<request>"
)]
pub async fn import_work_cards(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    source: String,
    request: Json<WorkCardImportRequest>,
) -> CreatedApiResult<ConnectorRunExecutionResponse> {
    require_admin(&auth)?;
    let source = validate_source(source)?;
    let run = create_running_run(&mut db, &source, "work_cards", "import", None).await?;
    let execution = execute_work_card_items(&mut db, &source, request.into_inner().items).await?;
    let response = finish_connector_run(&mut db, &source, "work_cards", run, execution).await?;
    record_audit_log(
        &mut db,
        &auth,
        "import",
        "connector_run",
        response.run.id,
        json!({
            "source": &response.source,
            "target": &response.target,
            "status": &response.run.status,
            "success_count": response.run.success_count,
            "failure_count": response.run.failure_count,
        }),
    )
    .await?;

    created(response)
}

#[rocket::post(
    "/connectors/<source>/notifications/import",
    format = "json",
    data = "<request>"
)]
pub async fn import_notifications(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    source: String,
    request: Json<NotificationImportRequest>,
) -> CreatedApiResult<ConnectorRunExecutionResponse> {
    require_admin(&auth)?;
    let source = validate_source(source)?;
    let run = create_running_run(&mut db, &source, "notifications", "import", None).await?;
    let execution =
        execute_notification_items(&mut db, &source, request.into_inner().items).await?;
    let response = finish_connector_run(&mut db, &source, "notifications", run, execution).await?;
    record_audit_log(
        &mut db,
        &auth,
        "import",
        "connector_run",
        response.run.id,
        json!({
            "source": &response.source,
            "target": &response.target,
            "status": &response.run.status,
            "success_count": response.run.success_count,
            "failure_count": response.run.failure_count,
        }),
    )
    .await?;

    created(response)
}

#[rocket::post(
    "/connectors/<source>/service-health/import",
    format = "json",
    data = "<request>"
)]
pub async fn import_service_health(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    source: String,
    request: Json<ServiceHealthImportRequest>,
) -> CreatedApiResult<ConnectorRunExecutionResponse> {
    require_admin(&auth)?;
    let source = validate_source(source)?;
    let run = create_running_run(&mut db, &source, "service_health", "import", None).await?;
    let execution =
        execute_service_health_items(&mut db, &source, run.id, request.into_inner().items).await?;
    let response = finish_connector_run(&mut db, &source, "service_health", run, execution).await?;
    record_audit_log(
        &mut db,
        &auth,
        "import",
        "connector_run",
        response.run.id,
        json!({
            "source": &response.source,
            "target": &response.target,
            "status": &response.run.status,
            "success_count": response.run.success_count,
            "failure_count": response.run.failure_count,
        }),
    )
    .await?;

    created(response)
}

fn validate_source(source: String) -> Result<String, ApiError> {
    struct ConnectorSource {
        source: String,
    }

    impl Validate for ConnectorSource {
        fn validate(&self) -> Vec<FieldViolation> {
            let mut errors = Vec::new();

            crate::validation::required(&mut errors, "source", &self.source);
            crate::validation::max_len(&mut errors, "source", &self.source, 64);

            errors
        }
    }

    validate_request(ConnectorSource { source }).map(|request| request.source)
}

fn validate_target(target: String) -> Result<String, ApiError> {
    struct ConnectorTarget {
        target: String,
    }

    impl Validate for ConnectorTarget {
        fn validate(&self) -> Vec<FieldViolation> {
            let mut errors = Vec::new();

            crate::validation::required(&mut errors, "target", &self.target);
            crate::validation::max_len(&mut errors, "target", &self.target, 64);
            crate::validation::one_of(
                &mut errors,
                "target",
                &self.target,
                &["service_health", "work_cards", "notifications"],
            );

            errors
        }
    }

    validate_request(ConnectorTarget { target }).map(|request| request.target)
}

pub(crate) async fn execute_claimed_connector_run(
    db: &mut AsyncPgConnection,
    run: ConnectorRun,
) -> Result<ConnectorRunExecutionResponse, ApiError> {
    let source = run.source.clone();
    let target = run.target.clone();
    let execution = match load_payload_for_claimed_run(db, &run).await {
        Ok(payload) => execute_payload_for_target(db, &source, &target, run.id, payload).await?,
        Err(error) => ConnectorExecution {
            data: Vec::new(),
            items: Vec::new(),
            errors: vec![connector_error(None, api_error_message(&error), None)],
        },
    };

    finish_connector_run(db, &source, &target, run, execution).await
}

async fn load_payload_for_claimed_run(
    db: &mut AsyncPgConnection,
    run: &ConnectorRun,
) -> Result<Value, ApiError> {
    let connector = ConnectorRepository::find_by_source(db, &run.source).await?;
    let config = ConnectorConfigRepository::find_by_source(db, &run.source).await?;

    if connector.status == "paused" {
        return Err(validation_error("status", "is paused"));
    }

    if !config.enabled {
        return Err(validation_error("enabled", "must be true"));
    }

    if let Some(payload) = run.payload.as_deref() {
        return Ok(parse_json_payload(payload));
    }

    let config_json = decrypt_connector_config(&config.config)
        .map_err(|error| validation_error_dynamic("config", error))?;

    match fetch_connector_payload(&run.target, &config_json).await {
        Ok(Some(payload)) => Ok(payload),
        Ok(None) => Ok(parse_json_payload(&config.sample_payload)),
        Err(error) => Err(validation_error_dynamic("config", error)),
    }
}

pub(crate) async fn create_queued_run(
    db: &mut AsyncPgConnection,
    source: &str,
    target: &str,
    trigger: &str,
    payload: Option<String>,
) -> Result<ConnectorRun, ApiError> {
    let started_at = Utc::now().naive_utc();

    ConnectorRunRepository::create(
        db,
        NewConnectorRun {
            source: source.to_owned(),
            target: target.to_owned(),
            status: "queued".to_owned(),
            success_count: 0,
            failure_count: 0,
            duration_ms: 0,
            error_message: None,
            started_at,
            finished_at: None,
            trigger: trigger.to_owned(),
            payload,
            claimed_at: None,
            worker_id: None,
        },
    )
    .await
    .map_err(ApiError::from)
}

async fn create_running_run(
    db: &mut AsyncPgConnection,
    source: &str,
    target: &str,
    trigger: &str,
    payload: Option<String>,
) -> Result<ConnectorRun, ApiError> {
    let run = create_queued_run(db, source, target, trigger, payload).await?;

    ConnectorRunRepository::update_state(
        db,
        run.id,
        ConnectorRunStateUpdate {
            status: "running".to_owned(),
            success_count: 0,
            failure_count: 0,
            duration_ms: 0,
            error_message: None,
            finished_at: None,
        },
    )
    .await
    .map_err(ApiError::from)
}

async fn finish_connector_run(
    db: &mut AsyncPgConnection,
    source: &str,
    target: &str,
    run: ConnectorRun,
    execution: ConnectorExecution,
) -> Result<ConnectorRunExecutionResponse, ApiError> {
    let finished_at = Utc::now().naive_utc();
    let started_at = run.claimed_at.unwrap_or(run.started_at);
    let duration_ms = (finished_at - started_at).num_milliseconds().max(0);
    let imported = execution.data.len();
    let failed = execution.errors.len();
    let status = match (imported, failed) {
        (_, 0) => "success",
        (0, _) => "failed",
        (_, _) => "partial_success",
    };
    let error_message = connector_run_error_message(&execution.errors);
    let run = ConnectorRunRepository::update_state(
        db,
        run.id,
        ConnectorRunStateUpdate {
            status: status.to_owned(),
            success_count: count_as_i32(imported),
            failure_count: count_as_i32(failed),
            duration_ms,
            error_message,
            finished_at: Some(finished_at),
        },
    )
    .await
    .map_err(ApiError::from)?;

    let item_errors = ConnectorRunItemErrorRepository::create_many(
        db,
        execution
            .errors
            .iter()
            .map(|error| NewConnectorRunItemError {
                connector_run_id: run.id,
                source: source.to_owned(),
                target: target.to_owned(),
                external_id: error.external_id.clone(),
                message: error.message.clone(),
                raw_item: error.raw_item.clone(),
            })
            .collect(),
    )
    .await
    .map_err(ApiError::from)?;

    let mut run_item_rows = execution
        .items
        .iter()
        .map(|item| NewConnectorRunItem {
            connector_run_id: run.id,
            source: source.to_owned(),
            target: target.to_owned(),
            record_id: item.record_id,
            external_id: item.external_id.clone(),
            status: item.status.to_owned(),
            snapshot: item.snapshot.clone(),
        })
        .collect::<Vec<_>>();
    run_item_rows.extend(execution.errors.iter().map(|error| NewConnectorRunItem {
        connector_run_id: run.id,
        source: source.to_owned(),
        target: target.to_owned(),
        record_id: None,
        external_id: error.external_id.clone(),
        status: "failed".to_owned(),
        snapshot: error.raw_item.clone(),
    }));
    let items = ConnectorRunItemRepository::create_many(db, run_item_rows)
        .await
        .map_err(ApiError::from)?;

    ConnectorRepository::touch_run_state(
        db,
        source,
        default_connector_kind(source, target),
        &run.status,
        finished_at,
    )
    .await?;

    Ok(ConnectorRunExecutionResponse {
        source: source.to_owned(),
        target: target.to_owned(),
        imported,
        failed,
        run,
        data: execution.data,
        items,
        errors: execution.errors,
        item_errors,
    })
}

async fn execute_payload_for_target(
    db: &mut AsyncPgConnection,
    source: &str,
    target: &str,
    run_id: i32,
    payload: Value,
) -> Result<ConnectorExecution, ApiError> {
    match target {
        "work_cards" => match serde_json::from_value::<WorkCardImportRequest>(payload) {
            Ok(request) => execute_work_card_items(db, source, request.items).await,
            Err(error) => Ok(payload_error(error)),
        },
        "notifications" => match serde_json::from_value::<NotificationImportRequest>(payload) {
            Ok(request) => execute_notification_items(db, source, request.items).await,
            Err(error) => Ok(payload_error(error)),
        },
        "service_health" => match serde_json::from_value::<ServiceHealthImportRequest>(payload) {
            Ok(request) => execute_service_health_items(db, source, run_id, request.items).await,
            Err(error) => Ok(payload_error(error)),
        },
        _ => Ok(ConnectorExecution {
            data: Vec::new(),
            items: Vec::new(),
            errors: vec![connector_error(
                None,
                "target is not supported".to_owned(),
                None,
            )],
        }),
    }
}

async fn execute_work_card_items(
    db: &mut AsyncPgConnection,
    source: &str,
    items: Vec<WorkCardImportItem>,
) -> Result<ConnectorExecution, ApiError> {
    let mut data = Vec::new();
    let mut run_items = Vec::new();
    let mut errors = Vec::new();

    for item in items {
        let external_id = item.external_id.clone();
        let raw_item = raw_item_json(&item);
        let work_card = validate_request(NewWorkCard {
            source: source.to_owned(),
            external_id: Some(item.external_id),
            title: item.title,
            status: item.status,
            priority: item.priority,
            assignee: item.assignee,
            due_at: item.due_at,
            url: item.url,
        });

        match work_card {
            Ok(work_card) => match WorkCardRepository::upsert_from_connector(db, work_card).await {
                Ok(work_card) => {
                    run_items.push(imported_run_item(
                        work_card.external_id.clone(),
                        Some(work_card.id),
                        raw_item,
                    ));
                    data.push(to_json_value(&work_card)?);
                }
                Err(error) => errors.push(connector_error(
                    Some(external_id),
                    error.to_string(),
                    raw_item,
                )),
            },
            Err(error) => errors.push(connector_error(
                Some(external_id),
                api_error_message(&error),
                raw_item,
            )),
        }
    }

    Ok(ConnectorExecution {
        data,
        items: run_items,
        errors,
    })
}

async fn execute_notification_items(
    db: &mut AsyncPgConnection,
    source: &str,
    items: Vec<NotificationImportItem>,
) -> Result<ConnectorExecution, ApiError> {
    let mut data = Vec::new();
    let mut run_items = Vec::new();
    let mut errors = Vec::new();

    for item in items {
        let external_id = item.external_id.clone();
        let raw_item = raw_item_json(&item);
        let notification = validate_request(NewNotification {
            source: source.to_owned(),
            external_id: Some(item.external_id),
            title: item.title,
            body: item.body,
            severity: item.severity,
            is_read: item.is_read,
            url: item.url,
        });

        match notification {
            Ok(notification) => {
                match NotificationRepository::upsert_from_connector(db, notification).await {
                    Ok(notification) => {
                        run_items.push(imported_run_item(
                            notification.external_id.clone(),
                            Some(notification.id),
                            raw_item,
                        ));
                        data.push(to_json_value(&notification)?);
                    }
                    Err(error) => errors.push(connector_error(
                        Some(external_id),
                        error.to_string(),
                        raw_item,
                    )),
                }
            }
            Err(error) => errors.push(connector_error(
                Some(external_id),
                api_error_message(&error),
                raw_item,
            )),
        }
    }

    Ok(ConnectorExecution {
        data,
        items: run_items,
        errors,
    })
}

async fn execute_service_health_items(
    db: &mut AsyncPgConnection,
    source: &str,
    run_id: i32,
    items: Vec<ServiceHealthImportItem>,
) -> Result<ConnectorExecution, ApiError> {
    let mut data = Vec::new();
    let mut run_items = Vec::new();
    let mut errors = Vec::new();

    for item in items {
        let external_id = item.external_id.clone();
        let raw_item = raw_item_json(&item);
        let last_checked_at = match parse_connector_datetime(item.last_checked_at.as_deref()) {
            Ok(value) => value,
            Err(error) => {
                errors.push(connector_error(Some(external_id), error, raw_item));
                continue;
            }
        };
        let service = validate_request(NewService {
            source: source.to_owned(),
            external_id: Some(item.external_id),
            maintainer_id: item.maintainer_id,
            slug: item.slug,
            name: item.name,
            lifecycle_status: item.lifecycle_status,
            health_status: item.health_status,
            description: item.description,
            repository_url: item.repository_url,
            dashboard_url: item.dashboard_url,
            runbook_url: item.runbook_url,
            last_checked_at,
        });

        match service {
            Ok(service) => match ServiceRepository::upsert_from_connector_with_health_check(
                db,
                service,
                run_id,
                raw_item.clone(),
            )
            .await
            {
                Ok(service) => {
                    run_items.push(imported_run_item(
                        service.external_id.clone(),
                        Some(service.id),
                        raw_item,
                    ));
                    data.push(to_json_value(&service)?);
                }
                Err(error) => errors.push(connector_error(
                    Some(external_id),
                    error.to_string(),
                    raw_item,
                )),
            },
            Err(error) => errors.push(connector_error(
                Some(external_id),
                api_error_message(&error),
                raw_item,
            )),
        }
    }

    Ok(ConnectorExecution {
        data,
        items: run_items,
        errors,
    })
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

fn connector_error(
    external_id: Option<String>,
    message: String,
    raw_item: Option<String>,
) -> ConnectorImportError {
    ConnectorImportError {
        external_id,
        message,
        raw_item,
    }
}

fn connector_run_error_message(errors: &[ConnectorImportError]) -> Option<String> {
    if errors.is_empty() {
        return None;
    }

    let message = errors
        .iter()
        .map(|error| {
            format!(
                "{}: {}",
                error.external_id.as_deref().unwrap_or("payload"),
                error.message
            )
        })
        .collect::<Vec<_>>()
        .join("; ");

    Some(message.chars().take(4096).collect())
}

fn parse_json_payload(payload: &str) -> Value {
    serde_json::from_str(payload).unwrap_or_else(|error| {
        serde_json::json!({
            "items": [],
            "_runtime_error": format!("sample_payload could not be parsed: {error}")
        })
    })
}

fn payload_error(error: serde_json::Error) -> ConnectorExecution {
    ConnectorExecution {
        data: Vec::new(),
        items: Vec::new(),
        errors: vec![connector_error(
            None,
            format!("payload could not be decoded: {error}"),
            None,
        )],
    }
}

fn imported_run_item(
    external_id: Option<String>,
    record_id: Option<i32>,
    snapshot: Option<String>,
) -> ConnectorRunItemDraft {
    ConnectorRunItemDraft {
        external_id,
        record_id,
        status: "imported",
        snapshot,
    }
}

fn raw_item_json<T: Serialize>(item: &T) -> Option<String> {
    const MAX_CONNECTOR_ITEM_SNAPSHOT_CHARS: usize = 8192;

    sanitized_json_snapshot(item, MAX_CONNECTOR_ITEM_SNAPSHOT_CHARS)
}

fn to_json_value<T: Serialize>(item: T) -> Result<Value, ApiError> {
    serde_json::to_value(item).map_err(|_| ApiError::Internal)
}

fn count_as_i32(count: usize) -> i32 {
    count.min(i32::MAX as usize) as i32
}

fn default_connector_kind(source: &str, target: &str) -> &'static str {
    let source = source.to_ascii_lowercase();

    if source.contains("azure") || source.contains("devops") {
        "azure_devops"
    } else if source.contains("outlook") {
        "outlook"
    } else if source.contains("erp") {
        "erp"
    } else if source.contains("monitor") || target == "service_health" {
        "monitoring"
    } else {
        "custom"
    }
}

fn parse_connector_datetime(value: Option<&str>) -> Result<Option<NaiveDateTime>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    if let Ok(datetime) = DateTime::parse_from_rfc3339(value) {
        return Ok(Some(datetime.naive_utc()));
    }

    for format in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(datetime) = NaiveDateTime::parse_from_str(value, format) {
            return Ok(Some(datetime));
        }
    }

    Err(format!(
        "last_checked_at is not a supported datetime: {value}"
    ))
}

fn api_error_message(error: &ApiError) -> String {
    match error {
        ApiError::BadRequest => "bad request".to_owned(),
        ApiError::Validation(errors) => errors
            .iter()
            .map(|error| format!("{} {}", error.field, error.message))
            .collect::<Vec<_>>()
            .join(", "),
        ApiError::Unauthorized => "authentication is required".to_owned(),
        ApiError::Forbidden => "permission denied".to_owned(),
        ApiError::NotFound => "resource was not found".to_owned(),
        ApiError::Database(error) => error.to_string(),
        ApiError::Internal => "internal server error".to_owned(),
    }
}

fn validation_error(field: &'static str, message: &'static str) -> ApiError {
    ApiError::Validation(vec![FieldViolation::new(field, message)])
}

fn validation_error_dynamic(field: &'static str, message: String) -> ApiError {
    ApiError::Validation(vec![FieldViolation::new(field, message)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NewAuditLog, NewMaintainer, NewService, NewServiceHealthCheck};
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
        let old_run = create_queued_run(&mut db, &source, "service_health", "manual", None)
            .await
            .expect("old run should be created");
        let stale_run = create_queued_run(&mut db, &source, "service_health", "manual", None)
            .await
            .expect("stale run should be created");
        let fresh_run = create_queued_run(&mut db, &source, "service_health", "manual", None)
            .await
            .expect("fresh run should be created");

        ConnectorRunRepository::update_state(
            &mut db,
            old_run.id,
            ConnectorRunStateUpdate {
                status: "success".to_owned(),
                success_count: 1,
                failure_count: 0,
                duration_ms: 1,
                error_message: None,
                finished_at: Some(old_at),
            },
        )
        .await
        .expect("old run should be finished");
        ConnectorRunRepository::update_state(
            &mut db,
            stale_run.id,
            ConnectorRunStateUpdate {
                status: "success".to_owned(),
                success_count: 1,
                failure_count: 0,
                duration_ms: 1,
                error_message: None,
                finished_at: Some(stale_at),
            },
        )
        .await
        .expect("stale run should be finished");
        ConnectorRunRepository::update_state(
            &mut db,
            fresh_run.id,
            ConnectorRunStateUpdate {
                status: "success".to_owned(),
                success_count: 1,
                failure_count: 0,
                duration_ms: 1,
                error_message: None,
                finished_at: Some(fresh_at),
            },
        )
        .await
        .expect("fresh run should be finished");
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
}
