use chrono::Utc;
use diesel_async::AsyncConnection;
use rocket::serde::json::Json;
use rocket_db_pools::Connection;
use serde_json::json;

use crate::api::{created, ok, ApiError, ApiResult, CreatedApiResult};
use crate::auth::{record_access_scope, require_admin, AuthenticatedUser};
use crate::crypto::{encrypt_connector_config, preserve_redacted_connector_config};
use crate::models::{
    Connector, ConnectorConfigUpdate, ConnectorRun, ConnectorScopeUpdate, ConnectorUpdate,
    NewConnector,
};
use crate::repositories::{
    ConnectorConfigRepository, ConnectorRepository, ConnectorRunItemErrorRepository,
    ConnectorRunItemRepository, ConnectorRunRepository, ConnectorWorkerRepository,
    MaintainerRepository, MaintenanceRunRepository, ServiceHealthCheckRepository, UserRepository,
};
use crate::rocket_routes::audit_logs::record_audit_log;
use crate::rocket_routes::connectors::runtime::{
    create_queued_retry_run, create_queued_run, create_running_run, execute_calendar_event_items,
    execute_claimed_connector_run, execute_notification_items, execute_service_health_items,
    execute_work_card_items, finish_connector_run, settle_cancelled_connector_run,
};
use crate::rocket_routes::connectors::shared::{
    validate_source, validate_target, validation_error, validation_error_dynamic,
};
use crate::rocket_routes::connectors::types::{
    CalendarEventImportRequest, ConnectorConfigResponse, ConnectorOperationsResponse,
    ConnectorRunDetail, ConnectorRunExecutionResponse, ConnectorWorkerStatus,
    ManualConnectorRunRequest, NotificationImportRequest, ServiceHealthImportRequest,
    WorkCardImportRequest,
};
use crate::rocket_routes::connectors::worker::connector_worker_stale_after_seconds;
use crate::rocket_routes::DbConn;
use crate::validation::validate_request;

#[rocket::get("/connectors")]
pub async fn get_connectors(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
) -> ApiResult<Vec<Connector>> {
    let access = record_access_scope(&mut db, &auth).await?;
    let connectors = ConnectorRepository::find_multiple_for_access(&mut db, 100, &access).await?;

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
    auth: AuthenticatedUser,
    source: String,
) -> ApiResult<Connector> {
    let source = validate_source(source)?;
    let connector = ConnectorRepository::find_by_source(&mut db, &source).await?;
    let access = record_access_scope(&mut db, &auth).await?;
    if !access.allows(connector.owner_user_id, connector.maintainer_id) {
        return Err(ApiError::NotFound);
    }

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
    match connector.scope_type.as_str() {
        "user" => {
            UserRepository::find(
                &mut db,
                connector.owner_user_id.ok_or(ApiError::BadRequest)?,
            )
            .await?;
        }
        "maintainer" => {
            MaintainerRepository::find(
                &mut db,
                connector.maintainer_id.ok_or(ApiError::BadRequest)?,
            )
            .await?;
        }
        _ => {}
    }
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
            "scope_type": &connector.scope_type,
            "owner_user_id": connector.owner_user_id,
            "maintainer_id": connector.maintainer_id,
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

#[rocket::put("/connectors/<source>/scope", format = "json", data = "<scope>")]
pub async fn update_connector_scope(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    source: String,
    scope: Json<ConnectorScopeUpdate>,
) -> ApiResult<Connector> {
    require_admin(&auth)?;
    let source = validate_source(source)?;
    let scope = validate_request(scope.into_inner())?;
    match scope.scope_type.as_str() {
        "user" => {
            UserRepository::find(&mut db, scope.owner_user_id.ok_or(ApiError::BadRequest)?).await?;
        }
        "maintainer" => {
            MaintainerRepository::find(&mut db, scope.maintainer_id.ok_or(ApiError::BadRequest)?)
                .await?;
        }
        _ => {}
    }

    let previous = ConnectorRepository::find_by_source(&mut db, &source).await?;
    let connector = ConnectorRepository::update_scope(
        &mut db,
        &source,
        &scope.scope_type,
        scope.owner_user_id,
        scope.maintainer_id,
    )
    .await?;
    record_audit_log(
        &mut db,
        &auth,
        "update_scope",
        "connector",
        &connector.source,
        json!({
            "previous": {
                "scope_type": previous.scope_type,
                "owner_user_id": previous.owner_user_id,
                "maintainer_id": previous.maintainer_id,
            },
            "current": {
                "scope_type": &connector.scope_type,
                "owner_user_id": connector.owner_user_id,
                "maintainer_id": connector.maintainer_id,
            }
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

    if !connector_run_is_retryable(&original.status) {
        return Err(validation_error(
            "status",
            "must be failed, partial_success, or cancelled",
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

    let run = create_queued_retry_run(
        &mut db,
        &original.source,
        &original.target,
        original.payload.clone(),
        original.id,
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

fn connector_run_is_retryable(status: &str) -> bool {
    matches!(status, "failed" | "partial_success" | "cancelled")
}

#[cfg(test)]
mod tests {
    use super::connector_run_is_retryable;

    #[test]
    fn cancelled_runs_require_an_explicit_retry() {
        for status in ["failed", "partial_success", "cancelled"] {
            assert!(connector_run_is_retryable(status));
        }
        for status in ["queued", "running", "success"] {
            assert!(!connector_run_is_retryable(status));
        }
    }
}

#[rocket::post("/connectors/runs/<id>/cancel")]
pub async fn cancel_connector_run(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
) -> ApiResult<ConnectorRun> {
    require_admin(&auth)?;
    let run = ConnectorRunRepository::find(&mut db, id).await?;
    if !matches!(run.status.as_str(), "queued" | "running") {
        return Err(validation_error("status", "must be queued or running"));
    }

    let run = ConnectorRunRepository::request_cancel(&mut db, id)
        .await?
        .ok_or_else(|| validation_error("status", "must still be queued or running"))?;
    record_audit_log(
        &mut db,
        &auth,
        "cancel",
        "connector_run",
        run.id,
        json!({
            "source": &run.source,
            "target": &run.target,
            "status": &run.status,
            "cancel_requested_at": run.cancel_requested_at,
        }),
    )
    .await?;

    ok(run)
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
    "/connectors/<source>/calendar-events/import",
    format = "json",
    data = "<request>"
)]
pub async fn import_calendar_events(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    source: String,
    request: Json<CalendarEventImportRequest>,
) -> CreatedApiResult<ConnectorRunExecutionResponse> {
    require_admin(&auth)?;
    let source = validate_source(source)?;
    let run = create_running_run(&mut db, &source, "calendar_events", "import", None).await?;
    let run_id = run.id;
    let request = request.into_inner();
    let transaction_source = source.clone();
    let transaction_result = db
        .transaction::<ConnectorRunExecutionResponse, ApiError, _>(|conn| {
            Box::pin(async move {
                let execution = execute_calendar_event_items(
                    conn,
                    &transaction_source,
                    run.id,
                    request.items,
                    request.snapshot_complete,
                )
                .await?;
                finish_connector_run(conn, &transaction_source, "calendar_events", run, execution)
                    .await
            })
        })
        .await;
    let response = match transaction_result {
        Ok(response) => response,
        Err(error) => {
            if ConnectorRunRepository::cancellation_requested(&mut db, run_id).await? {
                settle_cancelled_connector_run(&mut db, run_id).await?
            } else {
                return Err(error);
            }
        }
    };
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
            "snapshot_complete": response.run.snapshot_complete,
            "archived_count": response.run.archived_count,
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
    let run_id = run.id;
    let request = request.into_inner();
    let items = request.items;
    let snapshot_complete = request.snapshot_complete;
    let transaction_source = source.clone();
    let transaction_result = db
        .transaction::<ConnectorRunExecutionResponse, ApiError, _>(|conn| {
            Box::pin(async move {
                let execution = execute_work_card_items(
                    conn,
                    &transaction_source,
                    run.id,
                    items,
                    snapshot_complete,
                )
                .await?;
                finish_connector_run(conn, &transaction_source, "work_cards", run, execution).await
            })
        })
        .await;
    let response = match transaction_result {
        Ok(response) => response,
        Err(error) => {
            if ConnectorRunRepository::cancellation_requested(&mut db, run_id).await? {
                settle_cancelled_connector_run(&mut db, run_id).await?
            } else {
                return Err(error);
            }
        }
    };
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
            "snapshot_complete": response.run.snapshot_complete,
            "archived_count": response.run.archived_count,
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
    let run_id = run.id;
    let request = request.into_inner();
    let items = request.items;
    let snapshot_complete = request.snapshot_complete;
    let transaction_source = source.clone();
    let transaction_result = db
        .transaction::<ConnectorRunExecutionResponse, ApiError, _>(|conn| {
            Box::pin(async move {
                let execution = execute_notification_items(
                    conn,
                    &transaction_source,
                    run.id,
                    items,
                    snapshot_complete,
                )
                .await?;
                finish_connector_run(conn, &transaction_source, "notifications", run, execution)
                    .await
            })
        })
        .await;
    let response = match transaction_result {
        Ok(response) => response,
        Err(error) => {
            if ConnectorRunRepository::cancellation_requested(&mut db, run_id).await? {
                settle_cancelled_connector_run(&mut db, run_id).await?
            } else {
                return Err(error);
            }
        }
    };
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
            "snapshot_complete": response.run.snapshot_complete,
            "archived_count": response.run.archived_count,
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
    let run_id = run.id;
    let items = request.into_inner().items;
    let transaction_source = source.clone();
    let transaction_result = db
        .transaction::<ConnectorRunExecutionResponse, ApiError, _>(|conn| {
            Box::pin(async move {
                let execution =
                    execute_service_health_items(conn, &transaction_source, run.id, items).await?;
                finish_connector_run(conn, &transaction_source, "service_health", run, execution)
                    .await
            })
        })
        .await;
    let response = match transaction_result {
        Ok(response) => response,
        Err(error) => {
            if ConnectorRunRepository::cancellation_requested(&mut db, run_id).await? {
                settle_cancelled_connector_run(&mut db, run_id).await?
            } else {
                return Err(error);
            }
        }
    };
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
