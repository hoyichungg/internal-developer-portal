use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub static APP_HOST: &str = "http://127.0.0.1:8000";
pub static DATABASE_URL: &str = "postgres://postgres:postgres@localhost:5432/app_db";
static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

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

pub fn create_test_maintainer(client: &Client) -> Value {
    let response = client
        .post(format!("{}/maintainers", APP_HOST))
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
    let response = client
        .post(format!("{}/packages", APP_HOST))
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

pub fn delete_test_maintainer(client: &Client, maintainer: Value) {
    let response = client
        .delete(format!("{}/maintainers/{}", APP_HOST, maintainer["id"]))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

pub fn delete_test_package(client: &Client, package: Value) {
    let response = client
        .delete(format!("{}/packages/{}", APP_HOST, package["id"]))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}
