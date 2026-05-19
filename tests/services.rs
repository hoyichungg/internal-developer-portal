use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};

pub mod common;

#[test]
fn test_service_overview_returns_operational_context() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer = common::create_test_maintainer(&client);
    let package = common::create_test_package(&client, &maintainer);
    let source = common::unique_name("monitoring");

    let response = client
        .post(format!("{}/connectors", common::APP_HOST))
        .bearer_auth(&auth.token)
        .json(&json!({
            "source": source.clone(),
            "kind": "monitoring",
            "display_name": "Service Overview Monitoring",
            "status": "active"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = client
        .post(format!(
            "{}/connectors/{}/service-health/import",
            common::APP_HOST,
            source
        ))
        .bearer_auth(&auth.token)
        .json(&json!({
            "items": [{
                "external_id": "svc-overview",
                "maintainer_id": maintainer["id"],
                "slug": "overview-service",
                "name": "Overview Service",
                "lifecycle_status": "active",
                "health_status": "healthy",
                "description": "Service overview aggregation test",
                "repository_url": "https://github.com/acme/overview-service",
                "dashboard_url": "https://grafana.acme.test/d/overview",
                "runbook_url": "https://docs.acme.test/runbooks/overview",
                "last_checked_at": null
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let service = response.json::<Value>().unwrap()["data"]["data"][0].clone();

    let response = client
        .get(format!(
            "{}/services/{}/overview",
            common::APP_HOST,
            service["id"]
        ))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let overview = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(overview["service"]["id"], service["id"]);
    assert_eq!(overview["service"]["source"].as_str().unwrap(), source);
    assert_eq!(overview["owner"]["id"], maintainer["id"]);
    assert_eq!(
        overview["owner"]["display_name"],
        maintainer["display_name"]
    );
    assert_eq!(overview["maintainer"]["id"], maintainer["id"]);
    assert_eq!(overview["health"]["status"], "healthy");
    assert_eq!(overview["health"]["lifecycle_status"], "active");
    assert_eq!(
        overview["links"]["repository_url"],
        "https://github.com/acme/overview-service"
    );
    assert_eq!(
        overview["links"]["dashboard_url"],
        "https://grafana.acme.test/d/overview"
    );
    assert_eq!(
        overview["links"]["runbook_url"],
        "https://docs.acme.test/runbooks/overview"
    );
    assert_eq!(overview["connector"]["source"].as_str().unwrap(), source);
    assert_eq!(overview["connector"]["status"], "active");
    assert_contains_id(&overview["packages"], package["id"].as_i64().unwrap());
    assert!(overview["recent_connector_runs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|run| run["source"].as_str() == Some(source.as_str())
            && run["target"].as_str() == Some("service_health")
            && run["status"].as_str() == Some("success")));

    let response = client
        .delete(format!("{}/connectors/{}", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    common::delete_test_service(&client, service);
    common::delete_test_package(&client, package);
    common::delete_test_maintainer(&client, maintainer);
    common::delete_test_user(auth.user_id);
}

#[test]
fn test_service_overview_scopes_maintainer_member_visibility() {
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
            "display_name": "Overview Visibility",
            "email": "overview-visibility@example.com"
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
        .post(format!("{}/services", common::APP_HOST))
        .bearer_auth(&admin.token)
        .json(&json!({
            "maintainer_id": maintainer["id"],
            "slug": "overview-visibility-service",
            "name": "Overview Visibility Service",
            "lifecycle_status": "active",
            "health_status": "healthy",
            "description": "Service overview membership visibility test",
            "repository_url": null,
            "dashboard_url": null,
            "runbook_url": null,
            "last_checked_at": null
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let service: Value = response.json::<Value>().unwrap()["data"].clone();

    let response = client
        .get(format!(
            "{}/services/{}/overview",
            common::APP_HOST,
            service["id"]
        ))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    for token in [
        &admin.token,
        &owner.token,
        &maintainer_member.token,
        &viewer.token,
    ] {
        let overview = client
            .get(format!(
                "{}/services/{}/overview",
                common::APP_HOST,
                service["id"]
            ))
            .bearer_auth(token)
            .send()
            .unwrap();
        assert_eq!(overview.status(), StatusCode::OK);
        let overview = overview.json::<Value>().unwrap()["data"].clone();
        assert_contains_field_id(
            &overview["maintainer_members"],
            owner.user_id as i64,
            "user_id",
        );
        assert_contains_field_id(
            &overview["maintainer_members"],
            maintainer_member.user_id as i64,
            "user_id",
        );
        assert_contains_field_id(
            &overview["maintainer_members"],
            viewer.user_id as i64,
            "user_id",
        );
    }

    let overview = client
        .get(format!(
            "{}/services/{}/overview",
            common::APP_HOST,
            service["id"]
        ))
        .bearer_auth(&outsider.token)
        .send()
        .unwrap();
    assert_eq!(overview.status(), StatusCode::OK);
    let overview = overview.json::<Value>().unwrap()["data"].clone();
    assert_eq!(overview["maintainer_members"].as_array().unwrap().len(), 0);
    assert_eq!(overview["service"]["id"], service["id"]);

    let response = client
        .delete(format!("{}/services/{}", common::APP_HOST, service["id"]))
        .bearer_auth(&admin.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
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

fn assert_contains_id(items: &Value, id: i64) {
    assert_contains_field_id(items, id, "id");
}

fn assert_contains_field_id(items: &Value, id: i64, field: &str) {
    assert!(
        items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item[field].as_i64() == Some(id)),
        "expected collection to include id {id}, got {items:?}"
    );
}
