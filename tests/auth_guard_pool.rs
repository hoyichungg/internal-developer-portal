use std::{
    process::Command,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use diesel::sql_query;
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use internal_developer_portal::{config::AppConfig, rocket_routes::DbConn, server_app};
use reqwest::StatusCode;
use rocket::{figment::Figment, http::ContentType, local::asynchronous::Client};
use rocket_db_pools::Database;
use serde_json::{json, Value};
use tokio::time::timeout;

pub mod common;

struct TestUser {
    id: i32,
}

impl Drop for TestUser {
    fn drop(&mut self) {
        let _ = Command::new(env!("CARGO_BIN_EXE_cli"))
            .args(["users", "delete", &self.id.to_string()])
            .env("DATABASE_URL", common::database_url())
            .output();
    }
}

#[rocket::async_test]
async fn authenticated_database_route_works_with_one_pool_connection() {
    common::assert_safe_test_database_async().await;
    let username = format!(
        "single_pool_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos()
    );
    let password = "single-pool-secret";
    let user = create_test_user(&username, password);

    let rocket = server_app::build(test_config());
    let figment: Figment = rocket
        .figment()
        .clone()
        .merge(("databases.postgres.url", common::database_url()))
        .merge(("databases.postgres.max_connections", 1))
        .merge(("databases.postgres.connect_timeout", 30));
    let client = Arc::new(
        Client::tracked(rocket.configure(figment))
            .await
            .expect("Rocket must launch with the one-connection test pool"),
    );

    let pool = DbConn::fetch(client.rocket()).expect("database pool must be initialized");
    let held_connection = timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(connection) = pool.get().await {
                break connection;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the sole test pool connection must become available");
    let malformed = timeout(
        Duration::from_secs(2),
        client
            .post("/login")
            .header(ContentType::JSON)
            .body(r#"{"username":"unterminated""#)
            .dispatch(),
    )
    .await
    .expect("JSON parsing must finish before /login checks out a DB connection");
    assert_eq!(malformed.status(), rocket::http::Status::BadRequest);
    assert_eq!(
        malformed.into_json::<Value>().await.unwrap()["error"]["code"],
        "bad_request"
    );

    drop(held_connection);

    let login = client
        .post("/login")
        .header(ContentType::JSON)
        .body(
            json!({
                "username": username,
                "password": password,
            })
            .to_string(),
        )
        .dispatch()
        .await;
    assert_eq!(login.status().code, StatusCode::OK.as_u16());
    let login_body = login
        .into_json::<Value>()
        .await
        .expect("/login must return a JSON data envelope");
    assert_eq!(login_body["data"]["auth_method"], "password");
    assert!(login_body["data"]["expires_at"].as_str().is_some());
    assert!(login_body["data"].get("token").is_none());
    assert!(login_body["data"].get("token_type").is_none());

    let response = timeout(Duration::from_secs(2), client.get("/me").dispatch())
        .await
        .expect("authenticated DB route must not wait for a second pool connection");
    assert_eq!(response.status().code, StatusCode::OK.as_u16());
    let body = response
        .into_json::<Value>()
        .await
        .expect("/me must return JSON");
    assert_eq!(body["data"]["id"], user.id);

    let response = timeout(Duration::from_secs(2), client.get("/services").dispatch())
        .await
        .expect("authenticated catalog DB route must not wait for a second connection");
    assert_eq!(response.status().code, StatusCode::OK.as_u16());

    make_password_verification_slow(user.id).await;
    let slow_login_client = Arc::clone(&client);
    let slow_login_username = username.clone();
    let slow_login = tokio::spawn(async move {
        slow_login_client
            .post("/login")
            .header(ContentType::JSON)
            .body(
                json!({
                    "username": slow_login_username,
                    "password": "this-password-will-not-match",
                })
                .to_string(),
            )
            .dispatch()
            .await
            .status()
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !slow_login.is_finished(),
        "the deliberately expensive Argon2 verification must still be running"
    );

    let readiness = timeout(Duration::from_millis(750), client.get("/readyz").dispatch())
        .await
        .expect("Argon2 verification must not retain the sole DB pool connection");
    assert_eq!(readiness.status(), rocket::http::Status::Ok);
    let slow_login_status = timeout(Duration::from_secs(15), slow_login)
        .await
        .expect("slow password verification must remain bounded")
        .expect("login task must not panic");
    assert_eq!(slow_login_status, rocket::http::Status::Unauthorized);

    assert_unavailable_database_returns_structured_login_error(&username, password).await;
}

async fn assert_unavailable_database_returns_structured_login_error(
    username: &str,
    password: &str,
) {
    // This branch verifies runtime 503 behavior, so launch it outside the
    // test-only identity fairing that intentionally rejects an unreachable
    // database during ignition.
    let mut unavailable_config = test_config();
    unavailable_config.environment = "development".to_owned();
    let rocket = server_app::build(unavailable_config);
    let figment = rocket
        .figment()
        .clone()
        .merge((
            "databases.postgres.url",
            "postgres://postgres:postgres@127.0.0.1:1/unreachable_test",
        ))
        .merge(("databases.postgres.max_connections", 1))
        .merge(("databases.postgres.connect_timeout", 1));
    let unavailable_client = Client::tracked(rocket.configure(figment))
        .await
        .expect("unavailable database pool is initialized lazily");
    let unavailable = timeout(
        Duration::from_secs(2),
        unavailable_client
            .post("/login")
            .header(ContentType::JSON)
            .body(
                json!({
                    "username": username,
                    "password": password,
                })
                .to_string(),
            )
            .dispatch(),
    )
    .await
    .expect("login DB checkout must obey the configured pool timeout");
    assert_eq!(
        unavailable.status(),
        rocket::http::Status::ServiceUnavailable
    );
    assert_eq!(
        unavailable.into_json::<Value>().await.unwrap()["error"]["code"],
        "service_unavailable"
    );
}

async fn make_password_verification_slow(user_id: i32) {
    let mut db = AsyncPgConnection::establish(common::database_url())
        .await
        .expect("test database connection");
    let updated = sql_query(
        "UPDATE users \
         SET password = regexp_replace(password, 't=[0-9]+', 't=12') \
         WHERE id = $1",
    )
    .bind::<diesel::sql_types::Integer, _>(user_id)
    .execute(&mut db)
    .await
    .expect("test password hash cost update");
    assert_eq!(updated, 1);
}

fn create_test_user(username: &str, password: &str) -> TestUser {
    let output = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(["users", "create", username, password, "admin,member"])
        .env("DATABASE_URL", common::database_url())
        .output()
        .expect("CLI must run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("CLI output must be UTF-8");
    let id = stdout
        .split("id=")
        .nth(1)
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .expect("CLI create output must contain the user id");

    TestUser { id }
}

fn test_config() -> AppConfig {
    AppConfig {
        environment: "test".to_owned(),
        auth_token_ttl_seconds: 3_600,
        auth_max_active_sessions_per_user: 10,
        auth_cookie_secure: false,
        auth_login_max_failures: 5,
        auth_login_account_max_failures: 50,
        auth_login_window_seconds: 900,
        auth_login_lockout_seconds: 900,
        auth_password_login_enabled: true,
        entra: None,
    }
}
