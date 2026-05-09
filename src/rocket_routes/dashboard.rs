use rocket::serde::Serialize;
use rocket_db_pools::Connection;

use crate::api::{ok, ApiResult};
use crate::auth::AuthenticatedUser;
use crate::models::{Notification, Package, Service, ServiceHealthCheck, WorkCard};
use crate::repositories::{
    DashboardRepository, NotificationRepository, PackageRepository, ServiceHealthCheckRepository,
    ServiceRepository, WorkCardRepository,
};
use crate::rocket_routes::DbConn;
use chrono::{Duration, Utc};

pub const HEALTH_HISTORY_WINDOW_HOURS: i64 = 24;

#[derive(Serialize)]
pub struct DashboardSummary {
    pub total_services: i64,
    pub healthy_services: i64,
    pub degraded_services: i64,
    pub down_services: i64,
    pub active_packages: i64,
    pub open_work_cards: i64,
    pub unread_notifications: i64,
}

#[derive(Clone, Serialize)]
pub struct DashboardScope {
    pub maintainer_id: Option<i32>,
    pub source: Option<String>,
}

#[derive(Serialize)]
pub struct DashboardResponse {
    pub scope: DashboardScope,
    pub summary: DashboardSummary,
    pub health_history: ServiceHealthHistory,
    pub service_health: Vec<Service>,
    pub work_cards: Vec<WorkCard>,
    pub notifications: Vec<Notification>,
    pub recent_packages: Vec<Package>,
}

#[derive(Clone, Serialize)]
pub struct ServiceHealthHistory {
    pub summary: ServiceHealthHistorySummary,
    pub recent_checks: Vec<ServiceHealthCheck>,
    pub recent_incidents: Vec<ServiceHealthCheck>,
}

#[derive(Clone, Serialize)]
pub struct ServiceHealthHistorySummary {
    pub window_hours: i64,
    pub checks: usize,
    pub healthy_checks: usize,
    pub degraded_checks: usize,
    pub down_checks: usize,
    pub unknown_checks: usize,
    pub changed_checks: usize,
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
    let health_history = build_service_health_history(health_checks, HEALTH_HISTORY_WINDOW_HOURS);

    ok(DashboardResponse {
        scope,
        summary,
        health_history,
        service_health,
        work_cards,
        notifications,
        recent_packages,
    })
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
