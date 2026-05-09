use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub static APP_HOST: &str = "http://127.0.0.1:8000";
pub static DATABASE_URL: &str = "postgres://postgres:postgres@localhost:5432/app_db";
static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct TestAuth {
    pub token: String,
    pub user_id: i32,
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

    let token = response.json::<Value>().unwrap()["data"]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let user_id = client
        .get(format!("{}/me", APP_HOST))
        .bearer_auth(&token)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]["id"]
        .as_i64()
        .unwrap() as i32;

    TestAuth { token, user_id }
}

pub fn create_admin_auth(client: &Client) -> TestAuth {
    create_test_auth(client, "admin,member")
}

pub fn create_test_maintainer(client: &Client) -> Value {
    let auth = create_admin_auth(client);
    let response = client
        .post(format!("{}/maintainers", APP_HOST))
        .bearer_auth(&auth.token)
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
        .bearer_auth(&auth.token)
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
        .bearer_auth(&auth.token)
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
        .bearer_auth(&auth.token)
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
        .bearer_auth(&auth.token)
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
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

pub fn delete_test_service(client: &Client, service: Value) {
    let auth = create_admin_auth(client);
    let response = client
        .delete(format!("{}/services/{}", APP_HOST, service["id"]))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

pub fn delete_test_work_card(client: &Client, work_card: Value) {
    let auth = create_admin_auth(client);
    let response = client
        .delete(format!("{}/work-cards/{}", APP_HOST, work_card["id"]))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

pub fn delete_test_notification(client: &Client, notification: Value) {
    let auth = create_admin_auth(client);
    let response = client
        .delete(format!("{}/notifications/{}", APP_HOST, notification["id"]))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

pub fn delete_test_package(client: &Client, package: Value) {
    let auth = create_admin_auth(client);
    let response = client
        .delete(format!("{}/packages/{}", APP_HOST, package["id"]))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}
