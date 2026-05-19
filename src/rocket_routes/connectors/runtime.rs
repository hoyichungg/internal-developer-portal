use chrono::{DateTime, NaiveDateTime, Utc};
use diesel_async::AsyncPgConnection;
use rocket::serde::Serialize;
use serde_json::Value;

use crate::api::ApiError;
use crate::connector_adapters::fetch_connector_payload;
use crate::crypto::{decrypt_connector_config, encrypt_connector_config, sanitized_json_snapshot};
use crate::models::{
    ConnectorRun, ConnectorRunStateUpdate, NewConnectorRun, NewConnectorRunItem,
    NewConnectorRunItemError, NewNotification, NewService, NewWorkCard,
};
use crate::repositories::{
    ConnectorConfigRepository, ConnectorRepository, ConnectorRunItemErrorRepository,
    ConnectorRunItemRepository, ConnectorRunRepository, NotificationRepository, ServiceRepository,
    WorkCardRepository,
};
use crate::rocket_routes::connectors::shared::{
    api_error_message, count_as_i32, validation_error, validation_error_dynamic,
};
use crate::rocket_routes::connectors::types::{
    ConnectorExecution, ConnectorImportError, ConnectorRunExecutionResponse, ConnectorRunItemDraft,
    NotificationImportItem, NotificationImportRequest, ServiceHealthImportItem,
    ServiceHealthImportRequest, WorkCardImportItem, WorkCardImportRequest,
};
use crate::validation::validate_request;

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
        Ok(adapter_result) => {
            if let Some(updated_config) = adapter_result.updated_config {
                let encrypted_config = encrypt_connector_config(&updated_config)
                    .map_err(|error| validation_error_dynamic("config", error))?;
                ConnectorConfigRepository::update_config(db, &run.source, encrypted_config)
                    .await
                    .map_err(ApiError::from)?;
            }

            match adapter_result.payload {
                Some(payload) => Ok(payload),
                None => Ok(parse_json_payload(&config.sample_payload)),
            }
        }
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

pub(crate) async fn create_running_run(
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

pub(crate) async fn finish_connector_run(
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

pub(crate) async fn execute_work_card_items(
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

pub(crate) async fn execute_notification_items(
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

pub(crate) async fn execute_service_health_items(
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
