use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};

pub mod common;

#[test]
fn test_writes_create_audit_log_entries() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer = common::create_test_maintainer(&client);

    let response = client
        .post(format!("{}/packages", common::APP_HOST))
        .bearer_auth(&auth.token)
        .json(&json!({
            "maintainer_id": maintainer["id"],
            "slug": "audit-api",
            "name": "Audit API",
            "version": "1.0.0",
            "status": "active",
            "description": "Audit log verification package",
            "repository_url": null,
            "documentation_url": null,
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let package: Value = response.json::<Value>().unwrap()["data"].clone();

    let response = client
        .get(format!(
            "{}/audit-logs?resource_type=package&resource_id={}",
            common::APP_HOST,
            package["id"]
        ))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let logs = response.json::<Value>().unwrap()["data"].clone();
    let create_log = logs
        .as_array()
        .unwrap()
        .iter()
        .find(|log| {
            log["action"].as_str() == Some("create")
                && log["resource_type"].as_str() == Some("package")
        })
        .expect("package create should write an audit log");

    assert_eq!(
        create_log["actor_user_id"].as_i64(),
        Some(auth.user_id as i64)
    );
    assert!(create_log["metadata"]
        .as_str()
        .unwrap()
        .contains("audit-api"));

    common::delete_test_package(&client, package);
    common::delete_test_maintainer(&client, maintainer);
    common::delete_test_user(auth.user_id);
}
