use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};

pub mod common;
use common::CookieAuthRequest;

#[test]
fn test_service_overview_returns_operational_context() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer = common::create_test_maintainer(&client);
    let package = common::create_test_package(&client, &maintainer);
    let source = common::unique_name("monitoring");

    let response = client
        .post(format!("{}/connectors", common::APP_HOST))
        .cookie_auth(&auth.cookie)
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
        .cookie_auth(&auth.cookie)
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
        .cookie_auth(&auth.cookie)
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
        .cookie_auth(&auth.cookie)
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
        .cookie_auth(&admin.cookie)
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
            .cookie_auth(&admin.cookie)
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
        .cookie_auth(&admin.cookie)
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
        &admin.cookie,
        &owner.cookie,
        &maintainer_member.cookie,
        &viewer.cookie,
    ] {
        let overview = client
            .get(format!(
                "{}/services/{}/overview",
                common::APP_HOST,
                service["id"]
            ))
            .cookie_auth(token)
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
        .cookie_auth(&outsider.cookie)
        .send()
        .unwrap();
    assert_eq!(overview.status(), StatusCode::OK);
    let overview = overview.json::<Value>().unwrap()["data"].clone();
    assert_eq!(overview["maintainer_members"].as_array().unwrap().len(), 0);
    assert_eq!(overview["service"]["id"], service["id"]);

    let response = client
        .delete(format!("{}/services/{}", common::APP_HOST, service["id"]))
        .cookie_auth(&admin.cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
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

#[test]
fn test_service_overview_scopes_connector_metadata_and_run_history() {
    let client = Client::new();
    let admin = common::create_admin_auth(&client);
    let private_owner = common::create_test_auth(&client, "member");
    let team_member = common::create_test_auth(&client, "member");
    let outsider = common::create_test_auth(&client, "member");
    let private_source = common::unique_name("overview_private");
    let team_source = common::unique_name("overview_team");

    let maintainer = client
        .post(format!("{}/maintainers", common::APP_HOST))
        .cookie_auth(&admin.cookie)
        .json(&json!({
            "display_name": "Scoped Overview Team",
            "email": format!("{}@example.com", common::unique_name("overview-team"))
        }))
        .send()
        .unwrap();
    assert_eq!(maintainer.status(), StatusCode::CREATED);
    let maintainer: Value = maintainer.json::<Value>().unwrap()["data"].clone();
    let maintainer_id = maintainer["id"].as_i64().unwrap();

    let membership = client
        .post(format!(
            "{}/maintainers/{maintainer_id}/members",
            common::APP_HOST
        ))
        .cookie_auth(&admin.cookie)
        .json(&json!({
            "user_id": team_member.user_id,
            "role": "viewer"
        }))
        .send()
        .unwrap();
    assert_eq!(membership.status(), StatusCode::CREATED);

    create_scoped_connector(
        &client,
        &admin.cookie,
        &private_source,
        "user",
        Some(private_owner.user_id),
        None,
    );
    create_scoped_connector(
        &client,
        &admin.cookie,
        &team_source,
        "maintainer",
        None,
        Some(maintainer_id),
    );
    let private_service = import_scoped_service(
        &client,
        &admin.cookie,
        &private_source,
        maintainer_id,
        &common::unique_name("private-svc"),
    );
    let team_service = import_scoped_service(
        &client,
        &admin.cookie,
        &team_source,
        maintainer_id,
        &common::unique_name("team-svc"),
    );

    let services = client
        .get(format!("{}/services", common::APP_HOST))
        .cookie_auth(&outsider.cookie)
        .send()
        .unwrap();
    assert_eq!(services.status(), StatusCode::OK);
    let services = services.json::<Value>().unwrap()["data"].clone();
    assert_contains_id(&services, private_service["id"].as_i64().unwrap());
    assert_contains_id(&services, team_service["id"].as_i64().unwrap());

    for token in [&admin.cookie, &private_owner.cookie] {
        assert_connector_context(
            &client,
            token,
            private_service["id"].as_i64().unwrap(),
            &private_source,
            true,
        );
    }
    for token in [&team_member.cookie, &outsider.cookie] {
        assert_connector_context(
            &client,
            token,
            private_service["id"].as_i64().unwrap(),
            &private_source,
            false,
        );
    }
    for token in [&admin.cookie, &team_member.cookie] {
        assert_connector_context(
            &client,
            token,
            team_service["id"].as_i64().unwrap(),
            &team_source,
            true,
        );
    }
    for token in [&private_owner.cookie, &outsider.cookie] {
        assert_connector_context(
            &client,
            token,
            team_service["id"].as_i64().unwrap(),
            &team_source,
            false,
        );
    }

    for source in [&private_source, &team_source] {
        let response = client
            .delete(format!("{}/connectors/{source}", common::APP_HOST))
            .cookie_auth(&admin.cookie)
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
    for service in [&private_service, &team_service] {
        let response = client
            .delete(format!("{}/services/{}", common::APP_HOST, service["id"]))
            .cookie_auth(&admin.cookie)
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
    let response = client
        .delete(format!("{}/maintainers/{maintainer_id}", common::APP_HOST))
        .cookie_auth(&admin.cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    common::delete_test_user(outsider.user_id);
    common::delete_test_user(team_member.user_id);
    common::delete_test_user(private_owner.user_id);
    common::delete_test_user(admin.user_id);
}

fn create_scoped_connector(
    client: &Client,
    token: &str,
    source: &str,
    scope_type: &str,
    owner_user_id: Option<i32>,
    maintainer_id: Option<i64>,
) {
    let response = client
        .post(format!("{}/connectors", common::APP_HOST))
        .cookie_auth(token)
        .json(&json!({
            "source": source,
            "kind": "monitoring",
            "display_name": format!("Scoped {source}"),
            "status": "active",
            "scope_type": scope_type,
            "owner_user_id": owner_user_id,
            "maintainer_id": maintainer_id
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

fn import_scoped_service(
    client: &Client,
    token: &str,
    source: &str,
    maintainer_id: i64,
    slug: &str,
) -> Value {
    let response = client
        .post(format!(
            "{}/connectors/{source}/service-health/import",
            common::APP_HOST
        ))
        .cookie_auth(token)
        .json(&json!({
            "items": [{
                "external_id": format!("{slug}-external"),
                "maintainer_id": maintainer_id,
                "slug": slug,
                "name": format!("Scoped service {slug}"),
                "lifecycle_status": "active",
                "health_status": "healthy",
                "description": "Scoped service overview test",
                "repository_url": null,
                "dashboard_url": null,
                "runbook_url": null,
                "last_checked_at": null
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json::<Value>().unwrap()["data"]["data"][0].clone()
}

fn assert_connector_context(
    client: &Client,
    token: &str,
    service_id: i64,
    source: &str,
    visible: bool,
) {
    let response = client
        .get(format!(
            "{}/services/{service_id}/overview",
            common::APP_HOST
        ))
        .cookie_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let overview = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(overview["service"]["id"].as_i64(), Some(service_id));
    if visible {
        assert_eq!(overview["connector"]["source"].as_str(), Some(source));
        assert!(overview["recent_connector_runs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|run| run["source"].as_str() == Some(source)
                && run["target"].as_str() == Some("service_health")));
    } else {
        assert_eq!(overview["connector"], Value::Null);
        assert_eq!(overview["recent_connector_runs"], json!([]));
    }
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
