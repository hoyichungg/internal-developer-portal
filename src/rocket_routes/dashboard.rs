use rocket::serde::Serialize;
use rocket_db_pools::Connection;

use crate::api::{ok, ApiResult};
use crate::auth::AuthenticatedUser;
use crate::models::{
    ConnectorRun, ConnectorWorker, Notification, Package, Service, ServiceHealthCheck, WorkCard,
};
use crate::repositories::{
    ConnectorRunRepository, ConnectorWorkerRepository, DashboardRepository, NotificationRepository,
    PackageRepository, ServiceHealthCheckRepository, ServiceRepository, WorkCardRepository,
};
use crate::rocket_routes::connectors::connector_worker_stale_after_seconds;
use crate::rocket_routes::DbConn;
use chrono::{Duration, NaiveDateTime, Utc};
use utoipa::ToSchema;

pub const HEALTH_HISTORY_WINDOW_HOURS: i64 = 24;
pub const SERVICE_HEALTH_STALE_AFTER_HOURS: i64 = 2;

#[derive(Serialize, ToSchema)]
pub struct DashboardSummary {
    pub total_services: i64,
    pub healthy_services: i64,
    pub degraded_services: i64,
    pub down_services: i64,
    pub active_packages: i64,
    pub open_work_cards: i64,
    pub unread_notifications: i64,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct DashboardScope {
    pub maintainer_id: Option<i32>,
    pub source: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct DashboardResponse {
    pub scope: DashboardScope,
    pub summary: DashboardSummary,
    pub priority_items: Vec<DashboardPriorityItem>,
    pub health_history: ServiceHealthHistory,
    pub service_health: Vec<Service>,
    pub work_cards: Vec<WorkCard>,
    pub notifications: Vec<Notification>,
    pub recent_packages: Vec<Package>,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct ServiceHealthHistory {
    pub summary: ServiceHealthHistorySummary,
    pub recent_checks: Vec<ServiceHealthCheck>,
    pub recent_incidents: Vec<ServiceHealthCheck>,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct ServiceHealthHistorySummary {
    pub window_hours: i64,
    pub checks: usize,
    pub healthy_checks: usize,
    pub degraded_checks: usize,
    pub down_checks: usize,
    pub unknown_checks: usize,
    pub changed_checks: usize,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct DashboardPriorityItem {
    pub key: String,
    pub kind: String,
    pub severity: String,
    pub rank: i32,
    pub title: String,
    pub detail: String,
    pub source: Option<String>,
    pub target: Option<String>,
    pub record_id: Option<i32>,
    pub service_id: Option<i32>,
    pub url: Option<String>,
    pub occurred_at: Option<NaiveDateTime>,
}

pub struct DashboardPriorityContext {
    pub worker_status: Option<String>,
    pub active_workers: usize,
    pub stale_workers: usize,
    pub latest_worker_seen_at: Option<NaiveDateTime>,
    pub worker_stale_after_seconds: i64,
    pub health_data_stale: bool,
    pub latest_health_check_at: Option<NaiveDateTime>,
    pub health_stale_after_hours: i64,
}

#[rocket::get("/dashboard?<maintainer_id>&<source>")]
pub async fn dashboard(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
    maintainer_id: Option<i32>,
    source: Option<String>,
) -> ApiResult<DashboardResponse> {
    let scope = DashboardScope {
        maintainer_id,
        source,
    };

    let summary = DashboardSummary {
        total_services: DashboardRepository::total_services_scoped(
            &mut db,
            scope.maintainer_id,
            scope.source.as_deref(),
        )
        .await?,
        healthy_services: DashboardRepository::healthy_services_scoped(
            &mut db,
            scope.maintainer_id,
            scope.source.as_deref(),
        )
        .await?,
        degraded_services: DashboardRepository::degraded_services_scoped(
            &mut db,
            scope.maintainer_id,
            scope.source.as_deref(),
        )
        .await?,
        down_services: DashboardRepository::down_services_scoped(
            &mut db,
            scope.maintainer_id,
            scope.source.as_deref(),
        )
        .await?,
        active_packages: DashboardRepository::active_packages_scoped(&mut db, scope.maintainer_id)
            .await?,
        open_work_cards: DashboardRepository::open_work_cards_scoped(
            &mut db,
            scope.source.as_deref(),
        )
        .await?,
        unread_notifications: DashboardRepository::unread_notifications_scoped(
            &mut db,
            scope.source.as_deref(),
        )
        .await?,
    };

    let service_health = ServiceRepository::find_health_snapshot_scoped(
        &mut db,
        10,
        scope.maintainer_id,
        scope.source.as_deref(),
    )
    .await?;
    let work_cards =
        WorkCardRepository::find_open_scoped(&mut db, 10, scope.source.as_deref()).await?;
    let notifications =
        NotificationRepository::find_unread_scoped(&mut db, 10, scope.source.as_deref()).await?;
    let failed_connector_runs =
        ConnectorRunRepository::find_failed_scoped(&mut db, 25, scope.source.as_deref()).await?;
    let recent_packages =
        PackageRepository::find_recent_for_maintainer(&mut db, 5, scope.maintainer_id).await?;
    let health_checks = ServiceHealthCheckRepository::find_recent_scoped(
        &mut db,
        250,
        Utc::now().naive_utc() - Duration::hours(HEALTH_HISTORY_WINDOW_HOURS),
        scope.maintainer_id,
        scope.source.as_deref(),
    )
    .await?;
    let now = Utc::now().naive_utc();
    let health_history = build_service_health_history(health_checks, HEALTH_HISTORY_WINDOW_HOURS);
    let latest_health_check_at = health_history
        .recent_checks
        .iter()
        .map(|check| check.checked_at)
        .max();
    let health_data_stale = !service_health.is_empty()
        && latest_health_check_at
            .map(|checked_at| checked_at < now - Duration::hours(SERVICE_HEALTH_STALE_AFTER_HOURS))
            .unwrap_or(true);
    let worker_stale_after_seconds = connector_worker_stale_after_seconds();
    let workers = ConnectorWorkerRepository::find_recent(&mut db, 20).await?;
    let (worker_status, active_workers, stale_workers, latest_worker_seen_at) =
        summarize_workers(&workers, now, worker_stale_after_seconds);
    let priority_items = build_dashboard_priority_items(
        &service_health,
        &work_cards,
        &notifications,
        &failed_connector_runs,
        DashboardPriorityContext {
            worker_status: Some(worker_status),
            active_workers,
            stale_workers,
            latest_worker_seen_at,
            worker_stale_after_seconds,
            health_data_stale,
            latest_health_check_at,
            health_stale_after_hours: SERVICE_HEALTH_STALE_AFTER_HOURS,
        },
    );

    ok(DashboardResponse {
        scope,
        summary,
        priority_items,
        health_history,
        service_health,
        work_cards,
        notifications,
        recent_packages,
    })
}

pub fn build_dashboard_priority_items(
    services: &[Service],
    work_cards: &[WorkCard],
    notifications: &[Notification],
    failed_connector_runs: &[ConnectorRun],
    context: DashboardPriorityContext,
) -> Vec<DashboardPriorityItem> {
    let mut items = Vec::new();

    if context
        .worker_status
        .as_deref()
        .is_some_and(|status| status != "healthy")
    {
        let status = context
            .worker_status
            .clone()
            .unwrap_or_else(|| "unknown".to_owned());
        let title = if status == "missing" {
            "Connector worker heartbeat is missing".to_owned()
        } else {
            "Connector worker heartbeat is stale".to_owned()
        };
        let detail = context
            .latest_worker_seen_at
            .map(|seen_at| {
                format!(
                    "Last seen at {seen_at}; {} stale worker(s), {} active",
                    context.stale_workers, context.active_workers
                )
            })
            .unwrap_or_else(|| "No connector worker has checked in".to_owned());

        items.push(DashboardPriorityItem {
            key: "operations-worker".to_owned(),
            kind: "worker".to_owned(),
            severity: status,
            rank: 5,
            title,
            detail: format!(
                "{detail}; stale after {}s",
                context.worker_stale_after_seconds
            ),
            source: None,
            target: None,
            record_id: None,
            service_id: None,
            url: None,
            occurred_at: context.latest_worker_seen_at,
        });
    }

    if context.health_data_stale {
        let detail = context
            .latest_health_check_at
            .map(|checked_at| {
                format!(
                    "Latest health check was {checked_at}; stale after {}h",
                    context.health_stale_after_hours
                )
            })
            .unwrap_or_else(|| "No service health checks are available".to_owned());

        items.push(DashboardPriorityItem {
            key: "operations-health-data".to_owned(),
            kind: "health_data".to_owned(),
            severity: "stale".to_owned(),
            rank: 6,
            title: "Service health data is stale".to_owned(),
            detail,
            source: None,
            target: Some("service_health".to_owned()),
            record_id: None,
            service_id: None,
            url: None,
            occurred_at: context.latest_health_check_at,
        });
    }

    for service in services
        .iter()
        .filter(|service| matches!(service.health_status.as_str(), "down" | "degraded"))
    {
        items.push(DashboardPriorityItem {
            key: format!("service-{}", service.id),
            kind: "service".to_owned(),
            severity: service.health_status.clone(),
            rank: if service.health_status == "down" {
                10
            } else {
                20
            },
            title: service.name.clone(),
            detail: format!("{} service from {}", service.health_status, service.source),
            source: Some(service.source.clone()),
            target: Some("service_health".to_owned()),
            record_id: Some(service.id),
            service_id: Some(service.id),
            url: service
                .dashboard_url
                .clone()
                .or_else(|| service.runbook_url.clone()),
            occurred_at: service.last_checked_at.or(Some(service.updated_at)),
        });
    }

    for card in work_cards.iter().filter(|card| {
        card.status == "blocked" || (card.priority == "urgent" && card.status != "done")
    }) {
        items.push(DashboardPriorityItem {
            key: format!("work-card-{}", card.id),
            kind: "work_card".to_owned(),
            severity: if card.status == "blocked" {
                "blocked".to_owned()
            } else {
                "urgent".to_owned()
            },
            rank: if card.status == "blocked" { 30 } else { 40 },
            title: card.title.clone(),
            detail: [
                Some(card.priority.as_str()),
                card.assignee.as_deref(),
                Some(card.source.as_str()),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" - "),
            source: Some(card.source.clone()),
            target: Some("work_cards".to_owned()),
            record_id: Some(card.id),
            service_id: None,
            url: card.url.clone(),
            occurred_at: Some(card.updated_at),
        });
    }

    for notification in notifications
        .iter()
        .filter(|notification| notification.severity == "critical")
    {
        items.push(DashboardPriorityItem {
            key: format!("notification-{}", notification.id),
            kind: "notification".to_owned(),
            severity: notification.severity.clone(),
            rank: 50,
            title: notification.title.clone(),
            detail: notification
                .body
                .clone()
                .unwrap_or_else(|| notification.source.clone()),
            source: Some(notification.source.clone()),
            target: Some("notifications".to_owned()),
            record_id: Some(notification.id),
            service_id: None,
            url: notification.url.clone(),
            occurred_at: Some(notification.updated_at),
        });
    }

    for run in failed_connector_runs {
        items.push(DashboardPriorityItem {
            key: format!("connector-run-{}", run.id),
            kind: "connector_run".to_owned(),
            severity: run.status.clone(),
            rank: if run.status == "failed" { 60 } else { 65 },
            title: format!("{} / {}", run.source, run.target),
            detail: run
                .error_message
                .clone()
                .unwrap_or_else(|| format!("{} failed item(s)", run.failure_count)),
            source: Some(run.source.clone()),
            target: Some(run.target.clone()),
            record_id: Some(run.id),
            service_id: None,
            url: None,
            occurred_at: run.finished_at.or(Some(run.started_at)),
        });
    }

    items.sort_by(|left, right| {
        left.rank
            .cmp(&right.rank)
            .then_with(|| right.occurred_at.cmp(&left.occurred_at))
            .then_with(|| left.title.cmp(&right.title))
    });
    items.truncate(12);
    items
}

pub fn summarize_workers(
    workers: &[ConnectorWorker],
    now: NaiveDateTime,
    stale_after_seconds: i64,
) -> (String, usize, usize, Option<NaiveDateTime>) {
    let latest_worker_seen_at = workers.iter().map(|worker| worker.last_seen_at).max();
    let active_workers = workers
        .iter()
        .filter(|worker| (now - worker.last_seen_at).num_seconds() <= stale_after_seconds)
        .count();
    let stale_workers = workers.len().saturating_sub(active_workers);
    let worker_status = if active_workers > 0 {
        "healthy"
    } else if workers.is_empty() {
        "missing"
    } else {
        "stale"
    }
    .to_owned();

    (
        worker_status,
        active_workers,
        stale_workers,
        latest_worker_seen_at,
    )
}

pub fn build_service_health_history(
    recent_checks: Vec<ServiceHealthCheck>,
    window_hours: i64,
) -> ServiceHealthHistory {
    let healthy_checks = count_checks(&recent_checks, "healthy");
    let degraded_checks = count_checks(&recent_checks, "degraded");
    let down_checks = count_checks(&recent_checks, "down");
    let unknown_checks = count_checks(&recent_checks, "unknown");
    let changed_checks = recent_checks
        .iter()
        .filter(|check| {
            check
                .previous_health_status
                .as_deref()
                .is_some_and(|previous| previous != check.health_status)
        })
        .count();
    let recent_incidents = recent_checks
        .iter()
        .filter(|check| matches!(check.health_status.as_str(), "degraded" | "down"))
        .take(8)
        .cloned()
        .collect();

    ServiceHealthHistory {
        summary: ServiceHealthHistorySummary {
            window_hours,
            checks: recent_checks.len(),
            healthy_checks,
            degraded_checks,
            down_checks,
            unknown_checks,
            changed_checks,
        },
        recent_checks,
        recent_incidents,
    }
}

fn count_checks(checks: &[ServiceHealthCheck], status: &str) -> usize {
    checks
        .iter()
        .filter(|check| check.health_status == status)
        .count()
}
