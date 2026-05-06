use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};

pub mod common;

#[test]
fn test_get_maintainers() {
    let client = Client::new();
    let maintainer1: Value = common::create_test_maintainer(&client);
    let maintainer2: Value = common::create_test_maintainer(&client);

    let response = client
        .get(format!("{}/maintainers", common::APP_HOST))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json: Value = response.json().unwrap();
    let maintainers = json["data"].as_array().unwrap();
    assert!(maintainers.contains(&maintainer1));
    assert!(maintainers.contains(&maintainer2));

    common::delete_test_maintainer(&client, maintainer1);
    common::delete_test_maintainer(&client, maintainer2);
}

#[test]
fn test_create_maintainer() {
    let client = Client::new();
    let response = client
        .post(format!("{}/maintainers", common::APP_HOST))
        .json(&json!({
          "display_name":"Luke Ho",
          "email": "luke@ho.com"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let maintainer: Value = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(
        maintainer,
        json!({
          "id": maintainer["id"],
          "display_name": "Luke Ho",
          "email": "luke@ho.com",
          "created_at": maintainer["created_at"],
        })
    );

    common::delete_test_maintainer(&client, maintainer);
}

#[test]
fn test_create_maintainer_validates_request() {
    let client = Client::new();
    let response = client
        .post(format!("{}/maintainers", common::APP_HOST))
        .json(&json!({
          "display_name":"",
          "email": "not-an-email"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: Value = response.json().unwrap();
    assert_eq!(body["error"]["code"], "validation_failed");
    assert!(body["error"]["details"].as_array().unwrap().len() >= 2);
}

#[test]
fn test_view_maintainer() {
    let client = Client::new();
    let maintainer: Value = common::create_test_maintainer(&client);

    let response = client
        .get(format!(
            "{}/maintainers/{}",
            common::APP_HOST,
            maintainer["id"]
        ))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let maintainer: Value = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(
        maintainer,
        json!({
          "id": maintainer["id"],
          "display_name": "Luke Ho",
          "email": "luke@ho.com",
          "created_at": maintainer["created_at"],
        })
    );

    common::delete_test_maintainer(&client, maintainer);
}

#[test]
fn test_update_maintainer() {
    let client = Client::new();
    let maintainer: Value = common::create_test_maintainer(&client);

    let response = client
        .put(format!(
            "{}/maintainers/{}",
            common::APP_HOST,
            maintainer["id"]
        ))
        .json(&json!({
          "display_name":"Platform Team",
          "email": "platform@example.com"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let maintainer: Value = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(
        maintainer,
        json!({
          "id": maintainer["id"],
          "display_name": "Platform Team",
          "email": "platform@example.com",
          "created_at": maintainer["created_at"],
        })
    );

    common::delete_test_maintainer(&client, maintainer);
}

#[test]
fn test_delete_maintainer() {
    let client = Client::new();
    let maintainer: Value = common::create_test_maintainer(&client);

    let response = client
        .delete(format!(
            "{}/maintainers/{}",
            common::APP_HOST,
            maintainer["id"]
        ))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}
