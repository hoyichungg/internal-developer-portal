use diesel::{sql_query, sql_types::Text, QueryableByName};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use reqwest::{
    blocking::{Client, RequestBuilder, Response},
    header::{COOKIE, SET_COOKIE},
    StatusCode, Url,
};
use serde_json::{json, Value};
use std::ffi::OsStr;
use std::fmt;
use std::process::Command;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    OnceLock,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::OnceCell;

pub static APP_HOST: PortalTestBaseUrl = PortalTestBaseUrl;
pub static DATABASE_URL: PortalTestDatabaseUrl = PortalTestDatabaseUrl;
static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);
static PORTAL_TEST_BASE_URL: OnceLock<String> = OnceLock::new();
static PORTAL_TEST_DATABASE_URL: OnceLock<String> = OnceLock::new();
static TEST_DATABASE_VERIFIED: OnceCell<()> = OnceCell::const_new();

#[derive(Clone, Copy)]
pub struct PortalTestBaseUrl;

impl fmt::Display for PortalTestBaseUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(app_host())
    }
}

#[derive(Clone, Copy)]
pub struct PortalTestDatabaseUrl;

impl AsRef<OsStr> for PortalTestDatabaseUrl {
    fn as_ref(&self) -> &OsStr {
        OsStr::new(database_url())
    }
}

#[derive(QueryableByName)]
struct CurrentDatabase {
    #[diesel(sql_type = Text)]
    database_name: String,
}

pub fn app_host() -> &'static str {
    PORTAL_TEST_BASE_URL
        .get_or_init(|| {
            let value = std::env::var("PORTAL_TEST_BASE_URL")
                .expect("PORTAL_TEST_BASE_URL must point to the isolated integration server");
            let parsed = Url::parse(&value)
                .expect("PORTAL_TEST_BASE_URL must be an absolute HTTP loopback origin");
            assert_eq!(
                parsed.scheme(),
                "http",
                "PORTAL_TEST_BASE_URL must use HTTP for the local integration server"
            );
            assert!(
                matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "::1")),
                "PORTAL_TEST_BASE_URL must use a loopback host"
            );
            assert!(
                parsed.username().is_empty() && parsed.password().is_none(),
                "PORTAL_TEST_BASE_URL must not contain credentials"
            );
            assert!(
                parsed.path() == "/" && parsed.query().is_none() && parsed.fragment().is_none(),
                "PORTAL_TEST_BASE_URL must be an origin without a path, query, or fragment"
            );

            value.trim_end_matches('/').to_owned()
        })
        .as_str()
}

pub fn database_url() -> &'static str {
    PORTAL_TEST_DATABASE_URL
        .get_or_init(|| {
            let value = std::env::var("PORTAL_TEST_DATABASE_URL").expect(
                "PORTAL_TEST_DATABASE_URL must point to the migrated integration test database",
            );
            let database_name = database_name_from_url(&value);
            assert!(
                has_standalone_test_segment(&database_name),
                "PORTAL_TEST_DATABASE_URL database '{database_name}' must contain a standalone 'test' segment"
            );
            value
        })
        .as_str()
}

pub fn assert_safe_test_database() {
    if TEST_DATABASE_VERIFIED.get().is_some() {
        return;
    }
    assert!(
        tokio::runtime::Handle::try_current().is_err(),
        "async tests must await common::assert_safe_test_database_async() before database writes"
    );
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("the integration-test database guard runtime must start")
        .block_on(assert_safe_test_database_async());
}

pub async fn assert_safe_test_database_async() {
    TEST_DATABASE_VERIFIED
        .get_or_init(|| async {
        assert_eq!(
            std::env::var("APP_ENV").unwrap_or_default(),
            "test",
            "refusing integration-test writes unless APP_ENV is exactly 'test'"
        );

        let expected_name = database_name_from_url(database_url());
        let mut connection = AsyncPgConnection::establish(database_url())
            .await
            .expect("PORTAL_TEST_DATABASE_URL must be reachable before integration-test writes");
        let actual_name = sql_query("SELECT current_database()::text AS database_name")
            .get_result::<CurrentDatabase>(&mut connection)
            .await
            .expect("the integration test database name must be queryable")
            .database_name;
        assert_eq!(
            actual_name, expected_name,
            "PORTAL_TEST_DATABASE_URL resolved to an unexpected database"
        );
        assert!(
            has_standalone_test_segment(&actual_name),
            "refusing integration-test writes to database '{actual_name}' without a standalone 'test' segment"
        );
        })
        .await;
}

fn database_name_from_url(value: &str) -> String {
    let parsed = Url::parse(value)
        .expect("PORTAL_TEST_DATABASE_URL must be an absolute postgres:// or postgresql:// URL");
    assert!(
        matches!(parsed.scheme(), "postgres" | "postgresql"),
        "PORTAL_TEST_DATABASE_URL must use postgres:// or postgresql://"
    );
    assert!(
        parsed.query().is_none() && parsed.fragment().is_none(),
        "PORTAL_TEST_DATABASE_URL must not contain a query or fragment"
    );
    let database_name = parsed.path().trim_matches('/');
    assert!(
        !database_name.is_empty() && !database_name.contains('/'),
        "PORTAL_TEST_DATABASE_URL must contain exactly one database name"
    );
    database_name.to_owned()
}

fn has_standalone_test_segment(database_name: &str) -> bool {
    database_name
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|segment| segment == "test")
}

pub struct TestAuth {
    pub cookie: String,
    pub user_id: i32,
}

pub trait CookieAuthRequest {
    fn cookie_auth(self, cookie: &str) -> Self;
}

impl CookieAuthRequest for RequestBuilder {
    fn cookie_auth(self, cookie: &str) -> Self {
        self.header(COOKIE, cookie).header("X-IDP-CSRF", "1")
    }
}

pub fn session_cookie_from_response(response: &Response) -> String {
    response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            let pair = value.split(';').next()?;
            (pair.starts_with("idp_session=") || pair.starts_with("__Host-idp_session="))
                .then(|| pair.to_owned())
        })
        .expect("login must return an HttpOnly portal session cookie")
}

pub fn unique_name(prefix: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);

    format!(
        "{}_{}_{}_{}",
        prefix,
        std::process::id(),
        timestamp,
        counter
    )
}

pub fn create_test_user(username: &str, password: &str, roles: &str) {
    assert_safe_test_database();
    let output = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(["users", "create", username, password, roles])
        .env("DATABASE_URL", DATABASE_URL)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn delete_test_user(id: i32) {
    assert_safe_test_database();
    let output = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(["users", "delete", &id.to_string()])
        .env("DATABASE_URL", DATABASE_URL)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn create_test_auth(client: &Client, roles: &str) -> TestAuth {
    let username = unique_name("auth_user");
    let password = "secret123";
    create_test_user(&username, password, roles);

    let response = client
        .post(format!("{}/login", APP_HOST))
        .json(&json!({
            "username": username,
            "password": password,
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let cookie = session_cookie_from_response(&response);
    let login = response.json::<Value>().unwrap();
    assert_eq!(login["data"]["auth_method"], "password");
    assert!(login["data"]["expires_at"].as_str().is_some());
    assert!(login["data"].get("token").is_none());
    assert!(login["data"].get("token_type").is_none());
    let user_id = client
        .get(format!("{}/me", APP_HOST))
        .cookie_auth(&cookie)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]["id"]
        .as_i64()
        .unwrap() as i32;

    TestAuth { cookie, user_id }
}

pub fn create_admin_auth(client: &Client) -> TestAuth {
    create_test_auth(client, "admin,member")
}

pub fn create_test_maintainer(client: &Client) -> Value {
    let auth = create_admin_auth(client);
    let response = client
        .post(format!("{}/maintainers", APP_HOST))
        .cookie_auth(&auth.cookie)
        .json(&json!({
          "display_name":"Luke Ho",
          "email": "luke@ho.com"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    response.json::<Value>().unwrap()["data"].clone()
}

pub fn create_test_package(client: &Client, maintainer: &Value) -> Value {
    let auth = create_admin_auth(client);
    let response = client
        .post(format!("{}/packages", APP_HOST))
        .cookie_auth(&auth.cookie)
        .json(&json!({
          "maintainer_id": maintainer["id"],
          "slug": "catalog-api",
          "name":"Catalog API",
          "version":"0.1",
          "status": "active",
          "description": "Internal software catalog service",
          "repository_url": "https://github.com/acme/catalog-api",
          "documentation_url": "https://docs.acme.test/catalog-api",
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    response.json::<Value>().unwrap()["data"].clone()
}

pub fn create_test_service(client: &Client, maintainer: &Value) -> Value {
    let auth = create_admin_auth(client);
    let response = client
        .post(format!("{}/services", APP_HOST))
        .cookie_auth(&auth.cookie)
        .json(&json!({
          "maintainer_id": maintainer["id"],
          "slug": "identity-service",
          "name":"Identity Service",
          "lifecycle_status": "active",
          "health_status": "degraded",
          "description": "Authentication and user session service",
          "repository_url": "https://github.com/acme/identity-service",
          "dashboard_url": "https://grafana.acme.test/d/identity",
          "runbook_url": "https://docs.acme.test/runbooks/identity",
          "last_checked_at": null,
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    response.json::<Value>().unwrap()["data"].clone()
}

pub fn create_test_work_card(client: &Client) -> Value {
    let auth = create_admin_auth(client);
    let response = client
        .post(format!("{}/work-cards", APP_HOST))
        .cookie_auth(&auth.cookie)
        .json(&json!({
          "source": "azure-devops",
          "external_id": unique_name("work"),
          "title": "Review catalog deployment pipeline",
          "status": "in_progress",
          "priority": "high",
          "assignee": "platform-team",
          "due_at": null,
          "url": "https://dev.azure.test/work-items/catalog-pipeline",
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    response.json::<Value>().unwrap()["data"].clone()
}

pub fn create_test_notification(client: &Client) -> Value {
    let auth = create_admin_auth(client);
    let response = client
        .post(format!("{}/notifications", APP_HOST))
        .cookie_auth(&auth.cookie)
        .json(&json!({
          "source": "erp",
          "title": "ERP approval waiting for platform team",
          "body": "A deployment access request needs review.",
          "severity": "warning",
          "is_read": false,
          "url": "https://erp.acme.test/messages/deployment-access",
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    response.json::<Value>().unwrap()["data"].clone()
}

pub fn delete_test_maintainer(client: &Client, maintainer: Value) {
    let auth = create_admin_auth(client);
    let response = client
        .delete(format!("{}/maintainers/{}", APP_HOST, maintainer["id"]))
        .cookie_auth(&auth.cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

pub fn delete_test_service(client: &Client, service: Value) {
    let auth = create_admin_auth(client);
    let response = client
        .delete(format!("{}/services/{}", APP_HOST, service["id"]))
        .cookie_auth(&auth.cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

pub fn delete_test_work_card(client: &Client, work_card: Value) {
    let auth = create_admin_auth(client);
    let response = client
        .delete(format!("{}/work-cards/{}", APP_HOST, work_card["id"]))
        .cookie_auth(&auth.cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

pub fn delete_test_notification(client: &Client, notification: Value) {
    let auth = create_admin_auth(client);
    let response = client
        .delete(format!("{}/notifications/{}", APP_HOST, notification["id"]))
        .cookie_auth(&auth.cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

pub fn delete_test_package(client: &Client, package: Value) {
    let auth = create_admin_auth(client);
    let response = client
        .delete(format!("{}/packages/{}", APP_HOST, package["id"]))
        .cookie_auth(&auth.cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}
