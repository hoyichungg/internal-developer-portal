use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use diesel::{
    sql_query,
    sql_types::{BigInt, Integer, Text},
    QueryableByName,
};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use internal_developer_portal::{config::AppConfig, server_app};
use rocket::{
    figment::Figment,
    http::{ContentType, Status},
    local::asynchronous::Client,
};
use serde_json::json;
use sha2::{Digest, Sha256};

pub mod common;

struct TestUser {
    id: i32,
}

#[derive(QueryableByName)]
struct UsernameRow {
    #[diesel(sql_type = Text)]
    username: String,
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

#[derive(Debug)]
struct LoginOutcome {
    status: Status,
    has_retry_after: bool,
}

#[rocket::async_test]
async fn dual_bucket_throttle_isolates_clients_and_limits_distributed_guessing() {
    common::assert_safe_test_database_async().await;
    let username = unique_username();
    let password = "dual-bucket-secret";
    let _user = create_test_user(&username, password);
    let client = test_client().await;

    let abusive_ip = ip(10);
    assert_eq!(
        login(&client, abusive_ip, &username, "wrong-password")
            .await
            .status,
        Status::Unauthorized
    );
    let outcome = login(&client, abusive_ip, &username, "wrong-password").await;
    assert_eq!(outcome.status, Status::TooManyRequests);
    assert!(outcome.has_retry_after);

    let second_ip = ip(11);
    assert_eq!(
        login(&client, second_ip, &username, "wrong-password")
            .await
            .status,
        Status::Unauthorized,
        "a new source must not inherit the low-threshold client lock"
    );

    let recovery_ip = ip(12);
    assert_eq!(
        login(&client, recovery_ip, &username, password)
            .await
            .status,
        Status::Ok,
        "a valid login from another source must clear the account-wide failures"
    );
    assert_eq!(
        login(&client, abusive_ip, &username, password).await.status,
        Status::TooManyRequests,
        "successful recovery must not unlock another source's abusive bucket"
    );

    let distributed_ips = (20..26).map(ip).collect::<Vec<_>>();
    for (index, source_ip) in distributed_ips.iter().copied().enumerate() {
        let outcome = login(&client, source_ip, &username, "wrong-password").await;
        if index + 1 < distributed_ips.len() {
            assert_eq!(outcome.status, Status::Unauthorized);
        } else {
            assert_eq!(outcome.status, Status::TooManyRequests);
            assert!(outcome.has_retry_after);
        }
    }

    let fresh_ip = ip(30);
    assert_eq!(
        login(&client, fresh_ip, &username, password).await.status,
        Status::TooManyRequests,
        "the account-wide bucket must lock only after its higher threshold"
    );

    let mut created_ips = vec![abusive_ip, second_ip, recovery_ip, fresh_ip];
    created_ips.extend(distributed_ips);
    cleanup_throttle_buckets(&username, &created_ips).await;
}

#[rocket::async_test]
async fn cli_configuration_and_login_share_the_canonical_username() {
    common::assert_safe_test_database_async().await;
    let canonical = format!("canonical_{}", uuid::Uuid::new_v4().simple());
    let uppercase = canonical.to_uppercase();
    let mixed_with_whitespace = format!("  {uppercase}  ");
    let password = "canonical-login-secret";
    let user = create_test_user(&mixed_with_whitespace, password);

    let mut db = AsyncPgConnection::establish(common::database_url())
        .await
        .expect("test database connection");
    let persisted = sql_query("SELECT username FROM users WHERE id = $1")
        .bind::<Integer, _>(user.id)
        .get_result::<UsernameRow>(&mut db)
        .await
        .expect("the CLI-created user should exist");
    assert_eq!(persisted.username, canonical);

    let ensure = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(["users", "ensure-admin"])
        .env("DATABASE_URL", common::database_url())
        .env("SEED_ADMIN_USERNAME", &uppercase)
        .env("SEED_ADMIN_PASSWORD", "unused-existing-password")
        .env("SEED_ADMIN_ROLES", "admin,member")
        .output()
        .expect("ensure-admin CLI must run");
    assert!(
        ensure.status.success(),
        "{}",
        String::from_utf8_lossy(&ensure.stderr)
    );

    let canonical_count =
        sql_query("SELECT COUNT(*)::bigint AS count FROM users WHERE lower(username) = $1")
            .bind::<Text, _>(&canonical)
            .get_result::<CountRow>(&mut db)
            .await
            .expect("canonical user count should be queryable");
    assert_eq!(
        canonical_count.count, 1,
        "ensure-admin must reuse the existing differently-cased username"
    );

    let duplicate = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args([
            "users",
            "create",
            &uppercase,
            "unused-duplicate-password",
            "member",
        ])
        .env("DATABASE_URL", common::database_url())
        .output()
        .expect("duplicate create CLI must run");
    assert!(
        !duplicate.status.success(),
        "the database must reject a case-only duplicate username"
    );

    let client = test_client().await;
    assert_eq!(
        login(&client, ip(40), &mixed_with_whitespace, password)
            .await
            .status,
        Status::Ok,
        "password login must resolve the same canonical user as the CLI"
    );
}

async fn test_client() -> Client {
    let rocket = server_app::build(AppConfig {
        environment: "test".to_owned(),
        auth_token_ttl_seconds: 3_600,
        auth_max_active_sessions_per_user: 10,
        auth_cookie_secure: false,
        auth_login_max_failures: 2,
        auth_login_account_max_failures: 6,
        auth_login_window_seconds: 900,
        auth_login_lockout_seconds: 900,
        auth_password_login_enabled: true,
        entra: None,
    });
    let figment: Figment = rocket
        .figment()
        .clone()
        .merge(("databases.postgres.url", common::database_url()));

    Client::tracked(rocket.configure(figment))
        .await
        .expect("Rocket test client")
}

async fn login(client: &Client, source_ip: IpAddr, username: &str, password: &str) -> LoginOutcome {
    let response = client
        .post("/login")
        .remote(SocketAddr::new(source_ip, 40_000))
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

    LoginOutcome {
        status: response.status(),
        has_retry_after: response.headers().get_one("Retry-After").is_some(),
    }
}

fn create_test_user(username: &str, password: &str) -> TestUser {
    let output = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(["users", "create", username, password, "member"])
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

async fn cleanup_throttle_buckets(username: &str, client_ips: &[IpAddr]) {
    let mut db = AsyncPgConnection::establish(common::database_url())
        .await
        .expect("test database connection");
    let username = username.trim().to_lowercase();
    let mut hashes = HashSet::from([bucket_hash("account", &username, None)]);
    hashes.extend(
        client_ips.iter().map(|client_ip| {
            bucket_hash("username-client", &username, Some(&client_ip.to_string()))
        }),
    );

    for hash in hashes {
        sql_query("DELETE FROM login_throttle_buckets WHERE bucket_hash = $1")
            .bind::<Text, _>(hash)
            .execute(&mut db)
            .await
            .expect("test throttle bucket cleanup");
    }
}

fn bucket_hash(scope: &str, username: &str, client_ip: Option<&str>) -> String {
    let mut input = format!("portal-login-throttle:v2\0{scope}\0{username}");
    if let Some(client_ip) = client_ip {
        input.push('\0');
        input.push_str(client_ip);
    }

    format!("{:x}", Sha256::digest(input.as_bytes()))
}

fn ip(last_octet: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(192, 0, 2, last_octet))
}

fn unique_username() -> String {
    format!(
        "dual_throttle_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos()
    )
}
