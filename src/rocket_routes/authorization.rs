use crate::{
    api::{ok, ApiError, ApiResult},
    auth::AuthenticatedUser,
    config::AppConfig,
    models::{ConnectorRun, Maintainer, MaintenanceRun, Notification, Package, Service, WorkCard},
    repositories::{
        ConnectorRunRepository, ConnectorWorkerRepository, MaintainerMemberRepository,
        MaintainerRepository, MaintenanceRunRepository, NotificationRepository, PackageRepository,
        ServiceHealthCheckRepository, ServiceRepository, SessionRepository, UserRepository,
        WorkCardRepository,
    },
    rocket_routes::connectors::connector_worker_stale_after_seconds,
    rocket_routes::dashboard::{
        build_dashboard_priority_items, build_service_health_history, summarize_workers,
        DashboardPriorityContext, DashboardPriorityItem, ServiceHealthHistory,
        HEALTH_HISTORY_WINDOW_HOURS, SERVICE_HEALTH_STALE_AFTER_HOURS,
    },
    rocket_routes::DbConn,
    validation::{required, FieldViolation, Validate},
};
use argon2::{PasswordHash, PasswordVerifier};
use chrono::{Duration, NaiveDateTime, Utc};
use diesel::result::Error as DieselError;
use rocket::response::status::NoContent;
use rocket::serde::json::Json;
use rocket::serde::Serialize;
use rocket::State;
use rocket_db_pools::Connection;
use std::collections::{BTreeSet, HashMap};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(serde::Deserialize, ToSchema)]
pub struct Credentials {
    username: String,
    password: String,
}

impl Validate for Credentials {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        required(&mut errors, "username", &self.username);
        required(&mut errors, "password", &self.password);

        errors
    }
}

#[derive(Serialize, ToSchema)]
pub struct LoginResponse {
    pub token: String,
    pub token_type: &'static str,
    pub expires_at: NaiveDateTime,
}

#[derive(Serialize, ToSchema)]
pub struct MeResponse {
    pub id: i32,
    pub username: String,
    pub roles: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct MeOverviewResponse {
    pub user: MeResponse,
    pub maintainers: Vec<MeMaintainerOverview>,
    pub services: Vec<Service>,
    pub packages: Vec<Package>,
    pub open_work_cards: Vec<WorkCard>,
    pub unread_notifications: Vec<Notification>,
    pub failed_connector_runs: Vec<ConnectorRun>,
    pub priority_items: Vec<DashboardPriorityItem>,
    pub health_history: ServiceHealthHistory,
    pub operations: MeOperationsStatus,
    pub summary: MeOverviewSummary,
}

#[derive(Serialize, ToSchema)]
pub struct MeMaintainerOverview {
    pub maintainer: Maintainer,
    pub role: String,
}

#[derive(Serialize, ToSchema)]
pub struct MeOverviewSummary {
    pub maintainers: usize,
    pub services: usize,
    pub unhealthy_services: usize,
    pub packages: usize,
    pub open_work_cards: usize,
    pub unread_notifications: usize,
    pub failed_connector_runs: usize,
}

#[derive(Serialize, ToSchema)]
pub struct MeOperationsStatus {
    pub worker_status: String,
    pub active_workers: usize,
    pub stale_workers: usize,
    pub latest_worker_seen_at: Option<NaiveDateTime>,
    pub worker_stale_after_seconds: i64,
    pub latest_retention_cleanup: Option<MaintenanceRun>,
    pub latest_health_check_at: Option<NaiveDateTime>,
    pub health_data_stale: bool,
    pub health_stale_after_hours: i64,
}

#[rocket::post("/login", format = "json", data = "<credentials>")]
pub async fn login(
    mut db: Connection<DbConn>,
    config: &State<AppConfig>,
    credentials: Json<Credentials>,
) -> ApiResult<LoginResponse> {
    let credentials = crate::validation::validate_request(credentials.into_inner())?;

    let user = match UserRepository::find_by_username(&mut db, &credentials.username).await {
        Ok(user) => user,
        Err(DieselError::NotFound) => return Err(ApiError::Unauthorized),
        Err(e) => return Err(e.into()),
    };

    let argon2 = argon2::Argon2::default();
    let db_hash = PasswordHash::new(&user.password).map_err(|e| {
        rocket::error!("Invalid password hash for user {}: {}", user.username, e);
        ApiError::Internal
    })?;

    if argon2
        .verify_password(credentials.password.as_bytes(), &db_hash)
        .is_ok()
    {
        let token = generate_token();
        let expires_at = Utc::now().naive_utc() + Duration::seconds(config.auth_token_ttl_seconds);

        SessionRepository::create(&mut db, user.id, token.clone(), expires_at).await?;

        ok(LoginResponse {
            token,
            token_type: "Bearer",
            expires_at,
        })
    } else {
        Err(ApiError::Unauthorized)
    }
}

#[rocket::get("/me")]
pub async fn me(auth: AuthenticatedUser) -> ApiResult<MeResponse> {
    ok(MeResponse {
        id: auth.user.id,
        username: auth.user.username,
        roles: auth.roles.into_iter().map(|role| role.code).collect(),
    })
}

#[rocket::get("/me/overview")]
pub async fn me_overview(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
) -> ApiResult<MeOverviewResponse> {
    let membership_rows = MaintainerMemberRepository::find_by_user(&mut db, auth.user.id).await?;
    let roles_by_maintainer: HashMap<i32, String> = membership_rows
        .iter()
        .map(|member| (member.maintainer_id, member.role.clone()))
        .collect();

    let maintainers = if membership_rows.is_empty() && auth.is_admin() {
        MaintainerRepository::find_multiple(&mut db, 100).await?
    } else {
        let maintainer_ids: Vec<i32> = membership_rows
            .iter()
            .map(|member| member.maintainer_id)
            .collect();
        MaintainerRepository::find_by_ids(&mut db, &maintainer_ids).await?
    };

    let maintainer_ids: Vec<i32> = maintainers.iter().map(|maintainer| maintainer.id).collect();
    let services = ServiceRepository::find_for_maintainers(&mut db, 100, &maintainer_ids).await?;
    let packages =
        PackageRepository::find_recent_for_maintainers(&mut db, 100, &maintainer_ids).await?;
    let sources: Vec<String> = services
        .iter()
        .map(|service| service.source.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let open_work_cards = WorkCardRepository::find_open_for_sources(&mut db, 50, &sources).await?;
    let unread_notifications = if auth.is_admin() {
        NotificationRepository::find_unread_scoped(&mut db, 50, None).await?
    } else {
        NotificationRepository::find_unread_for_sources(&mut db, 50, &sources).await?
    };
    let failed_connector_runs =
        ConnectorRunRepository::find_failed_for_sources(&mut db, 25, &sources).await?;
    let now = Utc::now().naive_utc();
    let health_checks = ServiceHealthCheckRepository::find_recent_for_maintainers(
        &mut db,
        250,
        now - Duration::hours(HEALTH_HISTORY_WINDOW_HOURS),
        &maintainer_ids,
    )
    .await?;
    let health_history = build_service_health_history(health_checks, HEALTH_HISTORY_WINDOW_HOURS);
    let latest_health_check_at = health_history
        .recent_checks
        .iter()
        .map(|check| check.checked_at)
        .max();
    let health_data_stale = !services.is_empty()
        && latest_health_check_at
            .map(|checked_at| checked_at < now - Duration::hours(SERVICE_HEALTH_STALE_AFTER_HOURS))
            .unwrap_or(true);
    let worker_stale_after_seconds = connector_worker_stale_after_seconds();
    let workers = ConnectorWorkerRepository::find_recent(&mut db, 20).await?;
    let (worker_status, active_workers, stale_workers, latest_worker_seen_at) =
        summarize_workers(&workers, now, worker_stale_after_seconds);
    let latest_retention_cleanup =
        MaintenanceRunRepository::find_latest_success(&mut db, "retention_cleanup").await?;
    let priority_items = build_dashboard_priority_items(
        &services,
        &open_work_cards,
        &unread_notifications,
        &failed_connector_runs,
        DashboardPriorityContext {
            worker_status: Some(worker_status.clone()),
            active_workers,
            stale_workers,
            latest_worker_seen_at,
            worker_stale_after_seconds,
            health_data_stale,
            latest_health_check_at,
            health_stale_after_hours: SERVICE_HEALTH_STALE_AFTER_HOURS,
        },
    );
    let unhealthy_services = services
        .iter()
        .filter(|service| service.health_status != "healthy")
        .count();
    let maintainer_count = maintainers.len();
    let service_count = services.len();
    let package_count = packages.len();
    let open_work_card_count = open_work_cards.len();
    let unread_notification_count = unread_notifications.len();
    let failed_connector_run_count = failed_connector_runs.len();
    let maintainer_overviews = maintainers
        .into_iter()
        .map(|maintainer| {
            let role = roles_by_maintainer
                .get(&maintainer.id)
                .cloned()
                .unwrap_or_else(|| "admin".to_owned());

            MeMaintainerOverview { maintainer, role }
        })
        .collect();

    ok(MeOverviewResponse {
        user: MeResponse {
            id: auth.user.id,
            username: auth.user.username,
            roles: auth.roles.into_iter().map(|role| role.code).collect(),
        },
        maintainers: maintainer_overviews,
        services,
        packages,
        open_work_cards,
        unread_notifications,
        failed_connector_runs,
        priority_items,
        health_history,
        operations: MeOperationsStatus {
            worker_status,
            active_workers,
            stale_workers,
            latest_worker_seen_at,
            worker_stale_after_seconds,
            latest_retention_cleanup,
            latest_health_check_at,
            health_data_stale,
            health_stale_after_hours: SERVICE_HEALTH_STALE_AFTER_HOURS,
        },
        summary: MeOverviewSummary {
            maintainers: maintainer_count,
            services: service_count,
            unhealthy_services,
            packages: package_count,
            open_work_cards: open_work_card_count,
            unread_notifications: unread_notification_count,
            failed_connector_runs: failed_connector_run_count,
        },
    })
}

#[rocket::post("/logout")]
pub async fn logout(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
) -> Result<NoContent, ApiError> {
    SessionRepository::delete_by_token(&mut db, &auth.session.token).await?;

    Ok(NoContent)
}

fn generate_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
