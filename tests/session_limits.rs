use std::process::Command;

use diesel::{sql_query, sql_types::BigInt, QueryableByName};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use internal_developer_portal::{config::AppConfig, server_app};
use rocket::{
    figment::Figment,
    http::{ContentType, Cookie, Status},
    local::asynchronous::Client,
};
use serde_json::json;

pub mod common;

struct TestUser {
    id: i32,
    username: String,
    password: String,
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
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
async fn password_login_evicts_only_the_users_oldest_session() {
    common::assert_safe_test_database_async().await;
    let user = create_test_user("session_limit_route");
    let other_user = create_test_user("session_limit_other");
    let client = test_client(2).await;

    let oldest = login_cookie(&client, &user).await;
    let still_active = login_cookie(&client, &user).await;
    let other_users_cookie = login_cookie(&client, &other_user).await;
    let newest = login_cookie(&client, &user).await;

    assert_eq!(me_status(&client, &oldest).await, Status::Unauthorized);
    assert_eq!(me_status(&client, &still_active).await, Status::Ok);
    assert_eq!(me_status(&client, &newest).await, Status::Ok);
    assert_eq!(me_status(&client, &other_users_cookie).await, Status::Ok);

    let mut db = AsyncPgConnection::establish(common::database_url())
        .await
        .unwrap();
    assert_eq!(active_session_count(&mut db, user.id).await, 2);
    assert_eq!(active_session_count(&mut db, other_user.id).await, 1);
}

async fn test_client(max_active_sessions: i64) -> Client {
    let rocket = server_app::build(AppConfig {
        environment: "test".to_owned(),
        auth_token_ttl_seconds: 3_600,
        auth_max_active_sessions_per_user: max_active_sessions,
        auth_cookie_secure: false,
        auth_login_max_failures: 5,
        auth_login_account_max_failures: 50,
        auth_login_window_seconds: 900,
        auth_login_lockout_seconds: 900,
        auth_password_login_enabled: true,
        entra: None,
    });
    let figment: Figment = rocket
        .figment()
        .clone()
        .merge(("databases.postgres.url", common::database_url()));
    Client::untracked(rocket.configure(figment)).await.unwrap()
}

async fn login_cookie(client: &Client, user: &TestUser) -> String {
    let response = client
        .post("/login")
        .header(ContentType::JSON)
        .body(
            json!({
                "username": user.username,
                "password": user.password,
            })
            .to_string(),
        )
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Ok);
    let cookie = response
        .headers()
        .get("Set-Cookie")
        .find_map(|value| {
            value
                .split(';')
                .next()
                .and_then(|pair| pair.strip_prefix("idp_session="))
                .map(str::to_owned)
        })
        .expect("password login must set the session cookie");
    cookie
}

async fn me_status(client: &Client, token: &str) -> Status {
    client
        .get("/me")
        .cookie(Cookie::new("idp_session", token.to_owned()))
        .dispatch()
        .await
        .status()
}

async fn active_session_count(db: &mut AsyncPgConnection, user_id: i32) -> i64 {
    sql_query(
        "SELECT COUNT(*)::bigint AS count FROM sessions \
         WHERE user_id = $1 AND expires_at > NOW()",
    )
    .bind::<diesel::sql_types::Integer, _>(user_id)
    .get_result::<CountRow>(db)
    .await
    .unwrap()
    .count
}

fn create_test_user(prefix: &str) -> TestUser {
    let username = format!("{prefix}_{}", uuid::Uuid::new_v4().simple());
    let password = "session-limit-secret".to_owned();
    let output = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(["users", "create", &username, &password, "member"])
        .env("DATABASE_URL", common::database_url())
        .output()
        .expect("CLI must run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let id = stdout
        .split("id=")
        .nth(1)
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .expect("CLI create output must contain the user id");

    TestUser {
        id,
        username,
        password,
    }
}
