use crate::{
    api::{ok, ApiError, ApiResult},
    auth::{
        require_user_directory_access, session_cookie_name, AuthenticatedUser, SESSION_COOKIE_NAME,
    },
    config::AppConfig,
    models::{
        CalendarEvent, ConnectorRun, Maintainer, MaintainerMember, MaintenanceRun,
        NotificationView, Package, Service, User, UserRole, WorkCard,
    },
    repositories::{
        CalendarEventRepository, ConnectorRunRepository, ConnectorWorkerRepository, CreateSession,
        LoginThrottleRepository, MaintainerMemberRepository, MaintainerRepository,
        MaintenanceRunRepository, NotificationRepository, PackageRepository, RecordAccessScope,
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
    validation::{canonical_username, max_len, required, FieldViolation, Validate},
};
use argon2::{password_hash::SaltString, Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use chrono::{DateTime, Duration, Utc};
use diesel::result::Error as DieselError;
use diesel_async::AsyncPgConnection;
use rocket::http::{Cookie, CookieJar, SameSite};
use rocket::request::{FromRequest, Outcome, Request};
use rocket::response::status::NoContent;
use rocket::serde::json::Json;
use rocket::serde::Serialize;
use rocket::time::Duration as CookieDuration;
use rocket::State;
use rocket_db_pools::Connection;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
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
        max_len(&mut errors, "username", &self.username, 64);
        max_len(&mut errors, "password", &self.password, 1024);

        errors
    }
}

#[derive(Serialize, ToSchema)]
pub struct LoginResponse {
    pub expires_at: DateTime<Utc>,
    pub auth_method: &'static str,
}

#[derive(Serialize, ToSchema)]
pub struct MeResponse {
    pub id: i32,
    pub username: String,
    pub expires_at: DateTime<Utc>,
    pub auth_method: String,
    pub roles: Vec<String>,
    pub capabilities: MeCapabilities,
    pub maintainer_access: Vec<MeMaintainerAccess>,
}

#[derive(Serialize, ToSchema)]
pub struct RevokeAllSessionsResponse {
    pub revoked_sessions: usize,
}

pub struct LoginClientContext {
    pub(crate) ip_address: Option<String>,
    pub(crate) user_agent: Option<String>,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for LoginClientContext {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        Outcome::Success(Self {
            ip_address: request.client_ip().map(|address| address.to_string()),
            user_agent: request
                .headers()
                .get_one("User-Agent")
                .map(|value| value.chars().take(512).collect()),
        })
    }
}

static DUMMY_PASSWORD_HASH: OnceLock<String> = OnceLock::new();
static PASSWORD_VERIFICATION_LIMITER: OnceLock<PasswordVerificationLimiter> = OnceLock::new();
const MAX_CONCURRENT_PASSWORD_VERIFICATIONS: usize = 4;
const PASSWORD_VERIFICATION_QUEUE_TIMEOUT_SECONDS: u64 = 5;

struct PasswordVerificationLimiter {
    semaphore: Arc<Semaphore>,
}

impl PasswordVerificationLimiter {
    fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    async fn acquire(&self) -> Result<OwnedSemaphorePermit, ApiError> {
        tokio::time::timeout(
            std::time::Duration::from_secs(PASSWORD_VERIFICATION_QUEUE_TIMEOUT_SECONDS),
            Arc::clone(&self.semaphore).acquire_owned(),
        )
        .await
        .map_err(|_| ApiError::AuthenticationCapacityLimited {
            retry_after_seconds: 1,
        })?
        .map_err(|_| ApiError::Internal)
    }
}

fn password_verification_limiter() -> &'static PasswordVerificationLimiter {
    PASSWORD_VERIFICATION_LIMITER
        .get_or_init(|| PasswordVerificationLimiter::new(MAX_CONCURRENT_PASSWORD_VERIFICATIONS))
}

pub fn initialize_dummy_password_hash() {
    let _ = dummy_password_hash();
}

fn dummy_password_hash() -> &'static str {
    DUMMY_PASSWORD_HASH.get_or_init(|| {
        let salt = SaltString::encode_b64(b"portal-login-dummy-salt")
            .expect("the static dummy password salt must be valid");
        Argon2::default()
            .hash_password(b"not-a-real-password", &salt)
            .expect("the static dummy password must hash")
            .to_string()
    })
}

#[derive(Serialize, ToSchema)]
pub struct MeCapabilities {
    pub manage_connectors: bool,
    pub view_audit: bool,
    pub manage_maintainers: bool,
    pub view_user_directory: bool,
}

#[derive(Serialize, ToSchema)]
pub struct MeMaintainerAccess {
    pub maintainer_id: i32,
    pub role: String,
    pub can_write: bool,
    pub can_manage_members: bool,
}

#[derive(Serialize, ToSchema)]
pub struct UserSummary {
    pub id: i32,
    pub username: String,
    pub roles: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct MeOverviewResponse {
    pub user: MeResponse,
    pub maintainers: Vec<MeMaintainerOverview>,
    pub services: Vec<Service>,
    pub packages: Vec<Package>,
    pub today_calendar_events: Vec<CalendarEvent>,
    pub open_work_cards: Vec<WorkCard>,
    pub unread_notifications: Vec<NotificationView>,
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
    pub today_calendar_events: usize,
    pub open_work_cards: usize,
    pub unread_notifications: usize,
    pub failed_connector_runs: usize,
}

#[derive(Serialize, ToSchema)]
pub struct MeOperationsStatus {
    pub worker_status: String,
    pub active_workers: usize,
    pub stale_workers: usize,
    pub latest_worker_seen_at: Option<DateTime<Utc>>,
    pub worker_stale_after_seconds: i64,
    pub latest_retention_cleanup: Option<MaintenanceRun>,
    pub latest_health_check_at: Option<DateTime<Utc>>,
    pub health_data_stale: bool,
    pub health_stale_after_hours: i64,
}

#[rocket::post("/login", format = "json", data = "<credentials>")]
pub async fn login(
    db: &State<DbConn>,
    config: &State<AppConfig>,
    cookies: &CookieJar<'_>,
    client: LoginClientContext,
    credentials: Json<Credentials>,
) -> ApiResult<LoginResponse> {
    if !config.auth_password_login_enabled {
        return Err(ApiError::Forbidden);
    }

    let credentials = crate::validation::validate_request(credentials.into_inner())?;
    let now = Utc::now();
    let username = canonical_username(&credentials.username);
    let throttle_buckets = login_throttle_buckets(&username, client.ip_address.as_deref());

    let user = {
        let mut db = db.get().await.map_err(|pool_error| {
            rocket::error!(
                "Could not check out a database connection to begin password login: {}",
                pool_error
            );
            ApiError::ServiceUnavailable
        })?;

        if let Err(error) =
            LoginThrottleRepository::prune_before(&mut db, now - Duration::days(1)).await
        {
            rocket::warn!("Could not prune stale login throttle buckets: {}", error);
        }

        let client_retry_after =
            LoginThrottleRepository::retry_after_seconds(&mut db, &throttle_buckets.client, now)
                .await?;
        let account_retry_after =
            LoginThrottleRepository::retry_after_seconds(&mut db, &throttle_buckets.account, now)
                .await?;
        if let Some(retry_after_seconds) = client_retry_after.max(account_retry_after) {
            return Err(ApiError::RateLimited {
                retry_after_seconds,
            });
        }

        match UserRepository::find_by_username(&mut db, &username).await {
            Ok(user) => Some(user),
            Err(DieselError::NotFound) => None,
            Err(e) => return Err(e.into()),
        }
    };

    let encoded_password = user
        .as_ref()
        .map(|user| user.password.clone())
        .unwrap_or_else(|| dummy_password_hash().to_owned());
    let supplied_password = credentials.password;
    // The first DB checkout has already left scope. Queueing for bounded CPU
    // work must never reserve a scarce database connection.
    let verification_permit = password_verification_limiter().acquire().await?;
    let password_matches = tokio::task::spawn_blocking(move || {
        // Keep the owned permit inside the blocking task. If the HTTP request
        // is cancelled, Argon2 cannot be cancelled and must remain counted.
        let _verification_permit = verification_permit;
        let db_hash = PasswordHash::new(&encoded_password).map_err(|error| error.to_string())?;
        Ok::<_, String>(
            Argon2::default()
                .verify_password(supplied_password.as_bytes(), &db_hash)
                .is_ok(),
        )
    })
    .await
    .map_err(|join_error| {
        rocket::error!("Password verification worker failed: {}", join_error);
        ApiError::Internal
    })?
    .map_err(|password_hash_error| {
        if let Some(user) = user.as_ref() {
            rocket::error!(
                "Invalid password hash for user {}: {}",
                user.username,
                password_hash_error
            );
        } else {
            rocket::error!(
                "The static dummy login password hash is invalid: {}",
                password_hash_error
            );
        }
        ApiError::Internal
    })?;

    let mut db = db.get().await.map_err(|pool_error| {
        rocket::error!(
            "Could not check out a database connection to finish password login: {}",
            pool_error
        );
        ApiError::ServiceUnavailable
    })?;

    let Some(user) = user.filter(|_| password_matches) else {
        let now = Utc::now();
        let client_retry_after = LoginThrottleRepository::record_failure(
            &mut db,
            &throttle_buckets.client,
            now,
            config.auth_login_max_failures,
            config.auth_login_window_seconds,
            config.auth_login_lockout_seconds,
        )
        .await?;
        let account_retry_after = LoginThrottleRepository::record_failure(
            &mut db,
            &throttle_buckets.account,
            now,
            config.auth_login_account_max_failures,
            config.auth_login_window_seconds,
            config.auth_login_lockout_seconds,
        )
        .await?;

        return match client_retry_after.max(account_retry_after) {
            Some(retry_after_seconds) => Err(ApiError::RateLimited {
                retry_after_seconds,
            }),
            None => Err(ApiError::Unauthorized),
        };
    };

    LoginThrottleRepository::clear(&mut db, &throttle_buckets.client).await?;
    LoginThrottleRepository::clear(&mut db, &throttle_buckets.account).await?;
    let (_, expires_at) =
        establish_session(&mut db, user.id, "password", client, cookies, config).await?;

    ok(LoginResponse {
        expires_at,
        auth_method: "password",
    })
}

#[rocket::get("/me")]
pub async fn me(auth: AuthenticatedUser, mut db: Connection<DbConn>) -> ApiResult<MeResponse> {
    let memberships = MaintainerMemberRepository::find_by_user(&mut db, auth.user.id).await?;

    ok(build_me_response(&auth, &memberships))
}

#[rocket::get("/users")]
pub async fn users(
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
) -> ApiResult<Vec<UserSummary>> {
    require_user_directory_access(&mut db, &auth).await?;
    let mut users = UserRepository::find_with_roles(&mut db)
        .await?
        .into_iter()
        .map(user_summary)
        .collect::<Vec<_>>();

    users.sort_by(|left, right| left.username.cmp(&right.username));

    ok(users)
}

#[rocket::get("/me/overview")]
pub async fn me_overview(
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
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
    let access = RecordAccessScope {
        user_id: auth.user.id,
        is_admin: auth.is_admin(),
        maintainer_ids: membership_rows
            .iter()
            .map(|membership| membership.maintainer_id)
            .collect(),
    };
    let services = ServiceRepository::find_for_maintainers(&mut db, 100, &maintainer_ids).await?;
    let packages =
        PackageRepository::find_recent_for_maintainers(&mut db, 100, &maintainer_ids).await?;
    let now = Utc::now();
    let today_calendar_events = CalendarEventRepository::find_upcoming_for_access(
        &mut db,
        100,
        now - Duration::hours(18),
        now + Duration::hours(42),
        None,
        None,
        &access,
    )
    .await?;
    let open_work_cards =
        WorkCardRepository::find_open_for_access(&mut db, 50, None, None, &access).await?;
    let unread_notifications =
        NotificationRepository::find_actionable_for_access(&mut db, 50, None, None, &access)
            .await?;
    let failed_connector_runs =
        ConnectorRunRepository::find_failed_for_access(&mut db, 25, None, None, &access).await?;
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
    let today_calendar_event_count = today_calendar_events.len();
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

    let user = build_me_response(&auth, &membership_rows);

    ok(MeOverviewResponse {
        user,
        maintainers: maintainer_overviews,
        services,
        packages,
        today_calendar_events,
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
            today_calendar_events: today_calendar_event_count,
            open_work_cards: open_work_card_count,
            unread_notifications: unread_notification_count,
            failed_connector_runs: failed_connector_run_count,
        },
    })
}

fn build_me_response(auth: &AuthenticatedUser, memberships: &[MaintainerMember]) -> MeResponse {
    let is_admin = auth.is_admin();
    let mut maintainer_access = memberships
        .iter()
        .map(|membership| MeMaintainerAccess {
            maintainer_id: membership.maintainer_id,
            role: membership.role.clone(),
            can_write: is_admin || matches!(membership.role.as_str(), "owner" | "maintainer"),
            can_manage_members: is_admin || membership.role == "owner",
        })
        .collect::<Vec<_>>();
    maintainer_access.sort_by_key(|access| access.maintainer_id);

    MeResponse {
        id: auth.user.id,
        username: auth.user.username.clone(),
        expires_at: auth.session.expires_at,
        auth_method: auth.session.auth_method.clone(),
        roles: auth.roles.iter().map(|role| role.code.clone()).collect(),
        capabilities: MeCapabilities {
            manage_connectors: is_admin,
            view_audit: is_admin,
            manage_maintainers: is_admin,
            view_user_directory: is_admin
                || memberships
                    .iter()
                    .any(|membership| membership.role == "owner"),
        },
        maintainer_access,
    }
}

#[rocket::post("/logout")]
pub async fn logout(
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
    config: &State<AppConfig>,
    cookies: &CookieJar<'_>,
) -> Result<NoContent, ApiError> {
    SessionRepository::delete_by_token(&mut db, &auth.token).await?;
    clear_session_cookies(cookies, config);

    Ok(NoContent)
}

#[rocket::post("/sessions/revoke-all")]
pub async fn revoke_all_sessions(
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
    config: &State<AppConfig>,
    cookies: &CookieJar<'_>,
) -> ApiResult<RevokeAllSessionsResponse> {
    let revoked_sessions = SessionRepository::delete_by_user(&mut db, auth.user.id).await?;
    clear_session_cookies(cookies, config);

    ok(RevokeAllSessionsResponse { revoked_sessions })
}

pub(crate) async fn establish_session(
    db: &mut AsyncPgConnection,
    user_id: i32,
    auth_method: &str,
    client: LoginClientContext,
    cookies: &CookieJar<'_>,
    config: &AppConfig,
) -> Result<(String, DateTime<Utc>), ApiError> {
    let now = Utc::now();
    let token = generate_token();
    let expires_at = now + Duration::seconds(config.auth_token_ttl_seconds);
    SessionRepository::create(
        db,
        CreateSession {
            user_id,
            token: token.clone(),
            expires_at,
            max_active_sessions: config.auth_max_active_sessions_per_user,
            auth_method: auth_method.to_owned(),
            ip_address: client.ip_address,
            user_agent: client.user_agent,
        },
    )
    .await?;
    clear_legacy_session_cookie(cookies, config);
    cookies.add(session_cookie(&token, config));

    Ok((token, expires_at))
}

fn session_cookie(token: &str, config: &AppConfig) -> Cookie<'static> {
    Cookie::build((session_cookie_name(config), token.to_owned()))
        .path("/")
        .http_only(true)
        .secure(config.auth_cookie_secure)
        .same_site(SameSite::Lax)
        .max_age(CookieDuration::seconds(config.auth_token_ttl_seconds))
        .build()
}

fn clear_session_cookies(cookies: &CookieJar<'_>, config: &AppConfig) {
    cookies.remove(session_cookie_removal(session_cookie_name(config), config));
    clear_legacy_session_cookie(cookies, config);
}

fn clear_legacy_session_cookie(cookies: &CookieJar<'_>, config: &AppConfig) {
    if config.environment == "production" {
        cookies.remove(session_cookie_removal(SESSION_COOKIE_NAME, config));
    }
}

fn session_cookie_removal(name: &'static str, config: &AppConfig) -> Cookie<'static> {
    Cookie::build((name, ""))
        .path("/")
        .http_only(true)
        .secure(config.auth_cookie_secure)
        .same_site(SameSite::Lax)
        .build()
}

struct LoginThrottleBuckets {
    client: String,
    account: String,
}

fn login_throttle_buckets(username: &str, client_ip: Option<&str>) -> LoginThrottleBuckets {
    let username = username.trim().to_lowercase();
    let client_ip = client_ip.unwrap_or("unknown-client");

    LoginThrottleBuckets {
        client: login_throttle_bucket_hash("username-client", &username, Some(client_ip)),
        account: login_throttle_bucket_hash("account", &username, None),
    }
}

fn login_throttle_bucket_hash(scope: &str, username: &str, client_ip: Option<&str>) -> String {
    let mut input = format!("portal-login-throttle:v2\0{scope}\0{username}");
    if let Some(client_ip) = client_ip {
        input.push('\0');
        input.push_str(client_ip);
    }

    format!("{:x}", Sha256::digest(input.as_bytes()))
}

fn generate_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn user_summary((user, roles): (User, Vec<(UserRole, crate::models::Role)>)) -> UserSummary {
    UserSummary {
        id: user.id,
        username: user.username,
        roles: roles.into_iter().map(|(_, role)| role.code).collect(),
        created_at: user.created_at,
    }
}

#[cfg(test)]
mod login_throttle_tests {
    use super::*;

    #[test]
    fn client_bucket_is_scoped_by_ip_while_account_bucket_is_shared() {
        let first = login_throttle_buckets(" RecoveryAdmin ", Some("192.0.2.10"));
        let second = login_throttle_buckets("recoveryadmin", Some("192.0.2.11"));

        assert_ne!(first.client, second.client);
        assert_eq!(first.account, second.account);
    }

    #[test]
    fn missing_client_ip_uses_a_stable_fail_closed_bucket() {
        let first = login_throttle_buckets("RecoveryAdmin", None);
        let second = login_throttle_buckets("recoveryadmin", None);
        let known_ip = login_throttle_buckets("recoveryadmin", Some("192.0.2.10"));

        assert_eq!(first.client, second.client);
        assert_ne!(first.client, known_ip.client);
        assert_eq!(first.client.len(), 64);
        assert_eq!(first.account.len(), 64);
    }

    #[rocket::async_test]
    async fn password_verification_limiter_never_exceeds_its_permit_count() {
        let limiter = PasswordVerificationLimiter::new(2);
        let first = Arc::clone(&limiter.semaphore)
            .acquire_owned()
            .await
            .unwrap();
        let second = Arc::clone(&limiter.semaphore)
            .acquire_owned()
            .await
            .unwrap();
        assert_eq!(limiter.semaphore.available_permits(), 0);

        let waiting = Arc::clone(&limiter.semaphore).acquire_owned();
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), &mut waiting)
                .await
                .is_err(),
            "a third password verification must wait while two permits are held"
        );

        drop(first);
        let third = tokio::time::timeout(std::time::Duration::from_millis(100), &mut waiting)
            .await
            .expect("a queued verification must proceed when one permit is released")
            .unwrap();
        assert_eq!(limiter.semaphore.available_permits(), 0);
        drop((second, third));
        assert_eq!(limiter.semaphore.available_permits(), 2);
    }
}

#[cfg(test)]
mod session_cookie_tests {
    use super::*;
    use crate::auth::PRODUCTION_SESSION_COOKIE_NAME;
    use rocket::http::Cookie as RequestCookie;
    use rocket::local::blocking::Client;

    #[rocket::get("/set-session")]
    fn set_session(cookies: &CookieJar<'_>, config: &State<AppConfig>) -> NoContent {
        clear_legacy_session_cookie(cookies, config);
        cookies.add(session_cookie("test-token", config));
        NoContent
    }

    #[rocket::get("/clear-session")]
    fn clear_session(cookies: &CookieJar<'_>, config: &State<AppConfig>) -> NoContent {
        clear_session_cookies(cookies, config);
        NoContent
    }

    fn test_config(environment: &str) -> AppConfig {
        AppConfig {
            environment: environment.to_owned(),
            auth_token_ttl_seconds: 3_600,
            auth_max_active_sessions_per_user: 10,
            auth_cookie_secure: environment == "production",
            auth_login_max_failures: 5,
            auth_login_account_max_failures: 50,
            auth_login_window_seconds: 900,
            auth_login_lockout_seconds: 900,
            auth_password_login_enabled: true,
            entra: None,
        }
    }

    fn client(environment: &str) -> Client {
        Client::untracked(
            rocket::build()
                .manage(test_config(environment))
                .mount("/", rocket::routes![set_session, clear_session]),
        )
        .expect("test Rocket instance")
    }

    #[test]
    fn non_production_session_cookie_keeps_legacy_name_without_secure() {
        for environment in ["development", "test"] {
            let client = client(environment);
            let response = client.get("/set-session").dispatch();
            let cookies = response.headers().get("Set-Cookie").collect::<Vec<_>>();

            assert_eq!(cookies.len(), 1);
            assert!(cookies[0].starts_with("idp_session=test-token"));
            assert!(cookies[0].contains("HttpOnly"));
            assert!(cookies[0].contains("SameSite=Lax"));
            assert!(cookies[0].contains("Path=/"));
            assert!(!cookies[0].contains("Secure"));
        }
    }

    #[test]
    fn production_session_cookie_uses_host_prefix_and_expires_legacy_cookie() {
        let client = client("production");
        let response = client
            .get("/set-session")
            .cookie(RequestCookie::new(SESSION_COOKIE_NAME, "legacy-token"))
            .dispatch();
        let cookies = response.headers().get("Set-Cookie").collect::<Vec<_>>();

        let session = cookies
            .iter()
            .find(|value| value.starts_with(PRODUCTION_SESSION_COOKIE_NAME))
            .expect("production session cookie");
        assert!(session.starts_with("__Host-idp_session=test-token"));
        assert!(session.contains("HttpOnly"));
        assert!(session.contains("Secure"));
        assert!(session.contains("SameSite=Lax"));
        assert!(session.contains("Path=/"));
        assert!(!session.contains("Domain="));

        let legacy = cookies
            .iter()
            .find(|value| value.starts_with("idp_session="))
            .expect("legacy session removal cookie");
        assert!(legacy.contains("Max-Age=0"));
        assert!(legacy.contains("Secure"));
    }

    #[test]
    fn production_logout_removes_current_and_legacy_cookie_names() {
        let client = client("production");
        let response = client
            .get("/clear-session")
            .cookie(RequestCookie::new(
                PRODUCTION_SESSION_COOKIE_NAME,
                "current-token",
            ))
            .cookie(RequestCookie::new(SESSION_COOKIE_NAME, "legacy-token"))
            .dispatch();
        let cookies = response.headers().get("Set-Cookie").collect::<Vec<_>>();

        for name in [PRODUCTION_SESSION_COOKIE_NAME, SESSION_COOKIE_NAME] {
            let removal = cookies
                .iter()
                .find(|value| value.starts_with(&format!("{name}=")))
                .unwrap_or_else(|| panic!("missing removal cookie for {name}"));
            assert!(removal.contains("Max-Age=0"));
            assert!(removal.contains("Secure"));
            assert!(removal.contains("Path=/"));
        }
    }
}
