use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};

pub mod common;

#[test]
fn test_maintainer_membership_controls_catalog_writes() {
    let client = Client::new();
    let admin = common::create_admin_auth(&client);
    let owner = common::create_test_auth(&client, "member");
    let writer = common::create_test_auth(&client, "member");

    let maintainer = client
        .post(format!("{}/maintainers", common::APP_HOST))
        .bearer_auth(&admin.token)
        .json(&json!({
            "display_name": "Platform Ownership",
            "email": "platform-ownership@example.com"
        }))
        .send()
        .unwrap();
    assert_eq!(maintainer.status(), StatusCode::CREATED);
    let maintainer: Value = maintainer.json::<Value>().unwrap()["data"].clone();

    let response = client
        .post(format!(
            "{}/maintainers/{}/members",
            common::APP_HOST,
            maintainer["id"]
        ))
        .bearer_auth(&admin.token)
        .json(&json!({
            "user_id": owner.user_id,
            "role": "owner"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = client
        .get(format!(
            "{}/maintainers/{}/members",
            common::APP_HOST,
            maintainer["id"]
        ))
        .bearer_auth(&owner.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let members: Value = response.json::<Value>().unwrap()["data"].clone();
    assert!(members
        .as_array()
        .unwrap()
        .iter()
        .any(|member| member["user_id"].as_i64() == Some(owner.user_id as i64)));

    let response = client
        .post(format!(
            "{}/maintainers/{}/members",
            common::APP_HOST,
            maintainer["id"]
        ))
        .bearer_auth(&owner.token)
        .json(&json!({
            "user_id": writer.user_id,
            "role": "viewer"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = client
        .post(format!("{}/packages", common::APP_HOST))
        .bearer_auth(&writer.token)
        .json(&json!({
            "maintainer_id": maintainer["id"],
            "slug": "ownership-api",
            "name": "Ownership API",
            "version": "1.0.0",
            "status": "active",
            "description": "Catalog ownership boundary test",
            "repository_url": "https://github.com/acme/ownership-api",
            "documentation_url": null,
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = client
        .post(format!(
            "{}/maintainers/{}/members",
            common::APP_HOST,
            maintainer["id"]
        ))
        .bearer_auth(&owner.token)
        .json(&json!({
            "user_id": writer.user_id,
            "role": "maintainer"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = client
        .post(format!("{}/packages", common::APP_HOST))
        .bearer_auth(&writer.token)
        .json(&json!({
            "maintainer_id": maintainer["id"],
            "slug": "ownership-api",
            "name": "Ownership API",
            "version": "1.0.0",
            "status": "active",
            "description": "Catalog ownership boundary test",
            "repository_url": "https://github.com/acme/ownership-api",
            "documentation_url": null,
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let package: Value = response.json::<Value>().unwrap()["data"].clone();

    let response = client
        .post(format!("{}/services", common::APP_HOST))
        .bearer_auth(&writer.token)
        .json(&json!({
            "maintainer_id": maintainer["id"],
            "slug": "ownership-service",
            "name": "Ownership Service",
            "lifecycle_status": "active",
            "health_status": "healthy",
            "description": "Service owned by a maintainer member",
            "repository_url": "https://github.com/acme/ownership-service",
            "dashboard_url": null,
            "runbook_url": null,
            "last_checked_at": null,
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let service: Value = response.json::<Value>().unwrap()["data"].clone();

    let response = client
        .delete(format!("{}/services/{}", common::APP_HOST, service["id"]))
        .bearer_auth(&writer.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = client
        .delete(format!("{}/packages/{}", common::APP_HOST, package["id"]))
        .bearer_auth(&writer.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    common::delete_test_maintainer(&client, maintainer);
    common::delete_test_user(writer.user_id);
    common::delete_test_user(owner.user_id);
    common::delete_test_user(admin.user_id);
}

#[test]
fn test_catalog_writes_require_authentication() {
    let client = Client::new();
    let maintainer = common::create_test_maintainer(&client);

    let response = client
        .get(format!("{}/packages", common::APP_HOST))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = client
        .get(format!("{}/dashboard", common::APP_HOST))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = client
        .post(format!("{}/packages", common::APP_HOST))
        .json(&json!({
            "maintainer_id": maintainer["id"],
            "slug": "unauthenticated-package",
            "name": "Unauthenticated Package",
            "version": "1.0.0",
            "status": "active",
            "description": null,
            "repository_url": null,
            "documentation_url": null,
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    common::delete_test_maintainer(&client, maintainer);
}
