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

fn assert_contains_id(items: &Value, id: i64) {
    assert!(
        items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"].as_i64() == Some(id)),
        "expected collection to include id {id}, got {items:?}"
    );
}
