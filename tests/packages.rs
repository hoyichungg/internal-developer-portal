use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};

pub mod common;

#[test]
fn test_get_packages() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer: Value = common::create_test_maintainer(&client);
    let package1 = common::create_test_package(&client, &maintainer);
    let package2 = common::create_test_package(&client, &maintainer);

    let response = client
        .get(format!("{}/packages", common::APP_HOST))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json: Value = response.json().unwrap();
    let packages = json["data"].as_array().unwrap();
    assert!(packages.contains(&package1));
    assert!(packages.contains(&package2));

    common::delete_test_package(&client, package1);
    common::delete_test_package(&client, package2);
    common::delete_test_maintainer(&client, maintainer);
}

#[test]
fn test_create_package() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer = common::create_test_maintainer(&client);

    let response = client
        .post(format!("{}/packages", common::APP_HOST))
        .bearer_auth(&auth.token)
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

    let package: Value = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(
        package,
        json!({
          "id": package["id"],
          "slug": "catalog-api",
          "name": "Catalog API",
          "version": "0.1",
          "status": "active",
          "description": "Internal software catalog service",
          "repository_url": "https://github.com/acme/catalog-api",
          "documentation_url": "https://docs.acme.test/catalog-api",
          "maintainer_id": maintainer["id"],
          "created_at": package["created_at"],
          "updated_at": package["updated_at"],
        })
    );

    common::delete_test_package(&client, package);
    common::delete_test_maintainer(&client, maintainer);
}

#[test]
fn test_view_package() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer = common::create_test_maintainer(&client);
    let package = common::create_test_package(&client, &maintainer);

    let response = client
        .get(format!("{}/packages/{}", common::APP_HOST, package["id"]))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let package: Value = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(
        package,
        json!({
          "id": package["id"],
          "slug": "catalog-api",
          "name": "Catalog API",
          "version": "0.1",
          "status": "active",
          "description": "Internal software catalog service",
          "repository_url": "https://github.com/acme/catalog-api",
          "documentation_url": "https://docs.acme.test/catalog-api",
          "maintainer_id": maintainer["id"],
          "created_at": package["created_at"],
          "updated_at": package["updated_at"],
        })
    );

    common::delete_test_package(&client, package);
    common::delete_test_maintainer(&client, maintainer);
}

#[test]
fn test_update_package() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer = common::create_test_maintainer(&client);
    let package = common::create_test_package(&client, &maintainer);

    let response = client
        .put(format!("{}/packages/{}", common::APP_HOST, package["id"]))
        .bearer_auth(&auth.token)
        .json(&json!({
            "slug": "catalog-api",
            "name":"Catalog API",
            "version":"0.2",
            "status": "deprecated",
            "description": "Package catalog API service",
            "repository_url": "https://github.com/acme/catalog-api",
            "documentation_url": "https://docs.acme.test/catalog-api/v2",
            "maintainer_id": maintainer["id"],
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let package: Value = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(
        package,
        json!({
            "id": package["id"],
            "slug": "catalog-api",
            "name": "Catalog API",
            "version": "0.2",
            "status": "deprecated",
            "description": "Package catalog API service",
            "repository_url": "https://github.com/acme/catalog-api",
            "documentation_url": "https://docs.acme.test/catalog-api/v2",
            "maintainer_id": maintainer["id"],
            "created_at": package["created_at"],
            "updated_at": package["updated_at"],
        })
    );

    let maintainer2 = common::create_test_maintainer(&client);
    let response = client
        .put(format!("{}/packages/{}", common::APP_HOST, package["id"]))
        .bearer_auth(&auth.token)
        .json(&json!({
            "slug": "catalog-api",
            "name":"Catalog API",
            "version":"0.2",
            "status": "archived",
            "description": "A long internal package description used to prove large service notes can be stored without truncation.",
            "repository_url": "https://github.com/acme/catalog-api",
            "documentation_url": null,
            "maintainer_id": maintainer2["id"],
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let package: Value = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(
        package,
        json!({
            "id": package["id"],
            "slug": "catalog-api",
            "name": "Catalog API",
            "version": "0.2",
            "status": "archived",
            "description": "A long internal package description used to prove large service notes can be stored without truncation.",
            "repository_url": "https://github.com/acme/catalog-api",
            "documentation_url": null,
            "maintainer_id": maintainer2["id"],
            "created_at": package["created_at"],
            "updated_at": package["updated_at"],
        })
    );

    common::delete_test_package(&client, package);
    common::delete_test_maintainer(&client, maintainer);
    common::delete_test_maintainer(&client, maintainer2);
}

#[test]
fn test_delete_package() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer = common::create_test_maintainer(&client);
    let package = common::create_test_package(&client, &maintainer);

    let response = client
        .delete(format!("{}/packages/{}", common::APP_HOST, package["id"]))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    common::delete_test_maintainer(&client, maintainer);
}
