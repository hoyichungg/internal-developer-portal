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
