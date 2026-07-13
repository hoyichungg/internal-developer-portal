use diesel::{sql_query, sql_types::Integer, QueryableByName};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use reqwest::{
    blocking::Client,
    header::{COOKIE, RETRY_AFTER, SET_COOKIE},
    StatusCode,
};
use serde_json::{json, Value};

pub mod common;
use common::CookieAuthRequest;

#[test]
fn test_public_auth_config_and_disabled_entra_start() {
    let client = Client::new();
    let response = client
        .get(format!("{}/auth/config", common::APP_HOST))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("Cache-Control").unwrap(), "no-store");
    let body: Value = response.json().unwrap();
    assert_eq!(body["data"]["password_login_enabled"], true);
    assert_eq!(body["data"]["entra_login_enabled"], false);
    assert_eq!(body["data"].as_object().unwrap().len(), 2);

    let response = client
        .get(format!("{}/auth/entra/start", common::APP_HOST))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body: Value = response.json().unwrap();
    assert_eq!(body["error"]["code"], "not_found");
}

#[test]
fn test_login_me_logout_flow() {
    let client = Client::new();
    let username = common::unique_name("auth_user");
    let password = "secret123";

    common::create_test_user(&username, password, "admin,member");

    let response = client
        .post(format!("{}/login", common::APP_HOST))
        .json(&json!({
            "username": username,
            "password": password,
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let set_cookie = response
        .headers()
        .get(SET_COOKIE)
        .expect("login must set the browser session cookie")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(set_cookie.starts_with("idp_session="));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Lax"));
    assert!(set_cookie.contains("Path=/"));
    let cookie = set_cookie
        .split(';')
        .next()
        .expect("session cookie pair")
        .to_owned();

    let login: Value = response.json().unwrap();
    assert_eq!(login["data"]["auth_method"], "password");
    assert!(login["data"]["expires_at"].as_str().is_some());
    assert!(login["data"].get("token").is_none());
    assert!(login["data"].get("token_type").is_none());

    let response = client
        .get(format!("{}/me", common::APP_HOST))
        .header(COOKIE, &cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let me: Value = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(me["username"], username);
    assert!(me["roles"].as_array().unwrap().contains(&json!("admin")));
    assert!(me["roles"].as_array().unwrap().contains(&json!("member")));
    assert!(me["expires_at"].as_str().is_some());
    assert_eq!(me["auth_method"], "password");
    assert_eq!(me["capabilities"]["manage_connectors"], true);
    assert_eq!(me["capabilities"]["view_audit"], true);
    assert_eq!(me["capabilities"]["manage_maintainers"], true);
    assert_eq!(me["capabilities"]["view_user_directory"], true);
    assert_eq!(me["maintainer_access"], json!([]));

    let token_hash = stored_session_token_hash(me["id"].as_i64().unwrap() as i32);
    assert_eq!(token_hash.len(), 64);

    let response = client
        .post(format!("{}/logout", common::APP_HOST))
        .header(COOKIE, &cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = client
        .get(format!("{}/me", common::APP_HOST))
        .header(COOKIE, &cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = client
        .post(format!("{}/logout", common::APP_HOST))
        .header(COOKIE, &cookie)
        .header("X-IDP-CSRF", "1")
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cleared_cookie = response
        .headers()
        .get(SET_COOKIE)
        .expect("logout must expire the browser session cookie")
        .to_str()
        .unwrap();
    assert!(cleared_cookie.starts_with("idp_session="));
    assert!(cleared_cookie.contains("Max-Age=0"));

    let response = client
        .get(format!("{}/me", common::APP_HOST))
        .header(COOKIE, &cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    common::delete_test_user(me["id"].as_i64().unwrap() as i32);
}

#[test]
fn test_bearer_guard_remains_compatible_with_an_existing_session() {
    let client = Client::new();
    let username = common::unique_name("bearer_compat_user");
    let password = "secret123";
    common::create_test_user(&username, password, "member");

    let cookie = login_cookie(&client, &username, password);
    let raw_session_token = cookie.split_once('=').expect("session cookie pair").1;
    let response = client
        .get(format!("{}/me", common::APP_HOST))
        .bearer_auth(raw_session_token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let me: Value = response.json::<Value>().unwrap()["data"].clone();

    let response = client
        .post(format!("{}/logout", common::APP_HOST))
        .bearer_auth(raw_session_token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    common::delete_test_user(me["id"].as_i64().unwrap() as i32);
}

#[test]
fn test_revoke_all_sessions_invalidates_every_cookie() {
    let client = Client::new();
    let username = common::unique_name("revoke_all_user");
    let password = "secret123";
    common::create_test_user(&username, password, "member");

    let first_cookie = login_cookie(&client, &username, password);
    let second_cookie = login_cookie(&client, &username, password);
    let me = client
        .get(format!("{}/me", common::APP_HOST))
        .cookie_auth(&first_cookie)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]
        .clone();

    let response = client
        .post(format!("{}/sessions/revoke-all", common::APP_HOST))
        .cookie_auth(&first_cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().unwrap();
    assert_eq!(body["data"]["revoked_sessions"], 2);

    for cookie in [&first_cookie, &second_cookie] {
        let response = client
            .get(format!("{}/me", common::APP_HOST))
            .cookie_auth(cookie)
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    common::delete_test_user(me["id"].as_i64().unwrap() as i32);
}

#[test]
fn test_login_throttles_repeated_failures() {
    let client = Client::new();
    let username = common::unique_name("throttled_user");
    let password = "secret123";
    common::create_test_user(&username, password, "member");

    let cookie = login_cookie(&client, &username, password);
    let me_id = client
        .get(format!("{}/me", common::APP_HOST))
        .cookie_auth(&cookie)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]["id"]
        .as_i64()
        .unwrap() as i32;

    for attempt in 1..=5 {
        let response = client
            .post(format!("{}/login", common::APP_HOST))
            .json(&json!({
                "username": username,
                "password": "wrong-password",
            }))
            .send()
            .unwrap();

        if attempt < 5 {
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        } else {
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
            assert!(response.headers().get(RETRY_AFTER).is_some());
            let body: Value = response.json().unwrap();
            assert_eq!(body["error"]["code"], "login_throttled");
        }
    }

    let response = client
        .post(format!("{}/login", common::APP_HOST))
        .json(&json!({
            "username": username,
            "password": password,
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    common::delete_test_user(me_id);
}

#[test]
fn test_login_rejects_invalid_credentials() {
    let client = Client::new();
    let username = common::unique_name("auth_user");
    let password = "secret123";

    common::create_test_user(&username, password, "member");

    let response = client
        .post(format!("{}/login", common::APP_HOST))
        .json(&json!({
            "username": username,
            "password": "wrong-password",
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body: Value = response.json().unwrap();
    assert_eq!(body["error"]["code"], "unauthorized");

    let response = client
        .post(format!("{}/login", common::APP_HOST))
        .json(&json!({
            "username": username,
            "password": password,
        }))
        .send()
        .unwrap();
    let cookie = common::session_cookie_from_response(&response);
    let me_id = client
        .get(format!("{}/me", common::APP_HOST))
        .cookie_auth(&cookie)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]["id"]
        .as_i64()
        .unwrap() as i32;

    common::delete_test_user(me_id);
}

#[test]
fn test_users_directory_is_limited_to_admins_and_maintainer_owners() {
    let client = Client::new();
    let admin = common::create_admin_auth(&client);
    let owner = common::create_test_auth(&client, "member");
    let maintainer_member = common::create_test_auth(&client, "member");
    let viewer = common::create_test_auth(&client, "member");
    let outsider = common::create_test_auth(&client, "member");

    let maintainer = client
        .post(format!("{}/maintainers", common::APP_HOST))
        .cookie_auth(&admin.cookie)
        .json(&json!({
            "display_name": "Directory Access",
            "email": "directory-access@example.com"
        }))
        .send()
        .unwrap();
    assert_eq!(maintainer.status(), StatusCode::CREATED);
    let maintainer: Value = maintainer.json::<Value>().unwrap()["data"].clone();

    for (user_id, role) in [
        (owner.user_id, "owner"),
        (maintainer_member.user_id, "maintainer"),
        (viewer.user_id, "viewer"),
    ] {
        let response = client
            .post(format!(
                "{}/maintainers/{}/members",
                common::APP_HOST,
                maintainer["id"]
            ))
            .cookie_auth(&admin.cookie)
            .json(&json!({
                "user_id": user_id,
                "role": role
            }))
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let owner_me = client
        .get(format!("{}/me", common::APP_HOST))
        .cookie_auth(&owner.cookie)
        .send()
        .unwrap();
    assert_eq!(owner_me.status(), StatusCode::OK);
    let owner_me: Value = owner_me.json::<Value>().unwrap()["data"].clone();
    assert_eq!(owner_me["capabilities"]["manage_connectors"], false);
    assert_eq!(owner_me["capabilities"]["view_audit"], false);
    assert_eq!(owner_me["capabilities"]["manage_maintainers"], false);
    assert_eq!(owner_me["capabilities"]["view_user_directory"], true);
    assert_eq!(
        owner_me["maintainer_access"][0]["maintainer_id"],
        maintainer["id"]
    );
    assert_eq!(owner_me["maintainer_access"][0]["role"], "owner");
    assert_eq!(owner_me["maintainer_access"][0]["can_write"], true);
    assert_eq!(owner_me["maintainer_access"][0]["can_manage_members"], true);

    let response = client
        .get(format!("{}/users", common::APP_HOST))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = client
        .get(format!("{}/users", common::APP_HOST))
        .cookie_auth(&admin.cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = client
        .get(format!("{}/users", common::APP_HOST))
        .cookie_auth(&owner.cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_contains_user_id(
        &response.json::<Value>().unwrap()["data"],
        outsider.user_id as i64,
    );

    for token in [&maintainer_member.cookie, &viewer.cookie, &outsider.cookie] {
        let response = client
            .get(format!("{}/users", common::APP_HOST))
            .cookie_auth(token)
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    let response = client
        .delete(format!(
            "{}/maintainers/{}",
            common::APP_HOST,
            maintainer["id"]
        ))
        .cookie_auth(&admin.cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    common::delete_test_user(outsider.user_id);
    common::delete_test_user(viewer.user_id);
    common::delete_test_user(maintainer_member.user_id);
    common::delete_test_user(owner.user_id);
    common::delete_test_user(admin.user_id);
}

fn assert_contains_user_id(users: &Value, id: i64) {
    assert!(
        users
            .as_array()
            .unwrap()
            .iter()
            .any(|user| user["id"].as_i64() == Some(id)),
        "expected user directory to include id {id}, got {users:?}"
    );
}

fn login_cookie(client: &Client, username: &str, password: &str) -> String {
    let response = client
        .post(format!("{}/login", common::APP_HOST))
        .json(&json!({
            "username": username,
            "password": password,
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    common::session_cookie_from_response(&response)
}

#[derive(QueryableByName)]
struct StoredSessionToken {
    #[diesel(sql_type = diesel::sql_types::Text)]
    token_hash: String,
}

fn stored_session_token_hash(user_id: i32) -> String {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async move {
            let mut connection = AsyncPgConnection::establish(common::database_url())
                .await
                .unwrap();
            sql_query("SELECT token_hash FROM sessions WHERE user_id = $1 ORDER BY id DESC LIMIT 1")
                .bind::<Integer, _>(user_id)
                .get_result::<StoredSessionToken>(&mut connection)
                .await
                .unwrap()
                .token_hash
        })
}
