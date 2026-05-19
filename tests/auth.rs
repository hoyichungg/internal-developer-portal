use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};

pub mod common;

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

    let login: Value = response.json().unwrap();
    let token = login["data"]["token"].as_str().unwrap();
    assert_eq!(login["data"]["token_type"], "Bearer");

    let response = client
        .get(format!("{}/me", common::APP_HOST))
        .bearer_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let me: Value = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(me["username"], username);
    assert!(me["roles"].as_array().unwrap().contains(&json!("admin")));
    assert!(me["roles"].as_array().unwrap().contains(&json!("member")));

    let response = client
        .post(format!("{}/logout", common::APP_HOST))
        .bearer_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = client
        .get(format!("{}/me", common::APP_HOST))
        .bearer_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    common::delete_test_user(me["id"].as_i64().unwrap() as i32);
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
    let me_id = client
        .get(format!("{}/me", common::APP_HOST))
        .bearer_auth(
            response.json::<Value>().unwrap()["data"]["token"]
                .as_str()
                .unwrap(),
        )
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
        .bearer_auth(&admin.token)
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
            .bearer_auth(&admin.token)
            .json(&json!({
                "user_id": user_id,
                "role": role
            }))
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let response = client
        .get(format!("{}/users", common::APP_HOST))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = client
        .get(format!("{}/users", common::APP_HOST))
        .bearer_auth(&admin.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = client
        .get(format!("{}/users", common::APP_HOST))
        .bearer_auth(&owner.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_contains_user_id(
        &response.json::<Value>().unwrap()["data"],
        outsider.user_id as i64,
    );

    for token in [&maintainer_member.token, &viewer.token, &outsider.token] {
        let response = client
            .get(format!("{}/users", common::APP_HOST))
            .bearer_auth(token)
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
        .bearer_auth(&admin.token)
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
