use chrono::{Duration, Utc};
use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};

pub mod common;

#[test]
fn test_dashboard_aggregates_morning_work_context() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer = common::create_test_maintainer(&client);
    let package = common::create_test_package(&client, &maintainer);
    let service = common::create_test_service(&client, &maintainer);
    let work_card = common::create_test_work_card(&client);
    let notification = common::create_test_notification(&client);

    let response = client
        .get(format!("{}/dashboard", common::APP_HOST))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let dashboard: Value = response.json::<Value>().unwrap()["data"].clone();
    assert!(dashboard["summary"]["total_services"].as_i64().unwrap() >= 1);
    assert!(dashboard["summary"]["degraded_services"].as_i64().unwrap() >= 1);
    assert!(dashboard["summary"]["active_packages"].as_i64().unwrap() >= 1);
    assert!(dashboard["summary"]["open_work_cards"].as_i64().unwrap() >= 1);
    assert!(
        dashboard["summary"]["unread_notifications"]
            .as_i64()
            .unwrap()
            >= 1
    );

    assert_contains_id(
        &dashboard["service_health"],
        service["id"].as_i64().unwrap(),
    );
    assert_contains_id(&dashboard["work_cards"], work_card["id"].as_i64().unwrap());
    assert_contains_id(
        &dashboard["notifications"],
        notification["id"].as_i64().unwrap(),
    );
    assert_contains_id(
        &dashboard["recent_packages"],
        package["id"].as_i64().unwrap(),
    );

    common::delete_test_notification(&client, notification);
    common::delete_test_work_card(&client, work_card);
    common::delete_test_service(&client, service);
    common::delete_test_package(&client, package);
    common::delete_test_maintainer(&client, maintainer);
}

#[test]
fn test_dashboard_can_scope_catalog_and_service_health_by_maintainer() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer1 = common::create_test_maintainer(&client);
    let maintainer2 = common::create_test_maintainer(&client);
    let package1 = common::create_test_package(&client, &maintainer1);
    let package2 = common::create_test_package(&client, &maintainer2);
    let service1 = common::create_test_service(&client, &maintainer1);
    let service2 = common::create_test_service(&client, &maintainer2);

    let response = client
        .get(format!(
            "{}/dashboard?maintainer_id={}",
            common::APP_HOST,
            maintainer1["id"].as_i64().unwrap()
        ))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let dashboard: Value = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(dashboard["scope"]["maintainer_id"], maintainer1["id"]);
    assert_eq!(dashboard["scope"]["source"], Value::Null);
    assert_eq!(dashboard["summary"]["total_services"], 1);
    assert_eq!(dashboard["summary"]["degraded_services"], 1);
    assert_eq!(dashboard["summary"]["active_packages"], 1);

    assert_contains_id(
        &dashboard["service_health"],
        service1["id"].as_i64().unwrap(),
    );
    assert_not_contains_id(
        &dashboard["service_health"],
        service2["id"].as_i64().unwrap(),
    );
    assert_contains_id(
        &dashboard["recent_packages"],
        package1["id"].as_i64().unwrap(),
    );
    assert_not_contains_id(
        &dashboard["recent_packages"],
        package2["id"].as_i64().unwrap(),
    );

    common::delete_test_service(&client, service1);
    common::delete_test_service(&client, service2);
    common::delete_test_package(&client, package1);
    common::delete_test_package(&client, package2);
    common::delete_test_maintainer(&client, maintainer1);
    common::delete_test_maintainer(&client, maintainer2);
}

#[test]
fn test_dashboard_priority_items_put_today_first() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer = common::create_test_maintainer(&client);
    let source = common::unique_name("priority");
    let checked_at = (Utc::now().naive_utc() - Duration::minutes(5))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();

    let response = client
        .post(format!(
            "{}/connectors/{}/service-health/import",
            common::APP_HOST,
            source.as_str()
        ))
        .bearer_auth(&auth.token)
        .json(&json!({
            "items": [{
                "external_id": "priority-api",
                "maintainer_id": maintainer["id"],
                "slug": common::unique_name("priority-api"),
                "name": "Priority API",
                "lifecycle_status": "active",
                "health_status": "down",
                "description": "Priority dashboard service",
                "repository_url": null,
                "dashboard_url": "https://grafana.acme.test/d/priority",
                "runbook_url": null,
                "last_checked_at": format!("{checked_at}Z")
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let service: Value = response.json::<Value>().unwrap()["data"]["data"][0].clone();

    let response = client
        .post(format!("{}/work-cards", common::APP_HOST))
        .bearer_auth(&auth.token)
        .json(&json!({
            "source": source.as_str(),
            "external_id": common::unique_name("blocked"),
            "title": "Unblock production rollout",
            "status": "blocked",
            "priority": "urgent",
            "assignee": "platform-team",
            "due_at": null,
            "url": "https://dev.azure.test/work-items/priority-blocked"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let work_card: Value = response.json::<Value>().unwrap()["data"].clone();

    let response = client
        .post(format!("{}/notifications", common::APP_HOST))
        .bearer_auth(&auth.token)
        .json(&json!({
            "source": source.as_str(),
            "external_id": common::unique_name("critical"),
            "title": "Critical deployment approval",
            "body": "Approval is blocking today's release.",
            "severity": "critical",
            "is_read": false,
            "url": "https://erp.acme.test/messages/priority"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let notification: Value = response.json::<Value>().unwrap()["data"].clone();

    let response = client
        .post(format!(
            "{}/connectors/{}/work-cards/import",
            common::APP_HOST,
            source.as_str()
        ))
        .bearer_auth(&auth.token)
        .json(&json!({
            "items": [{
                "external_id": "bad-priority-work",
                "title": "Invalid priority work",
                "status": "todo",
                "priority": "not-a-priority",
                "assignee": null,
                "due_at": null,
                "url": null
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let failed_run: Value = response.json::<Value>().unwrap()["data"]["run"].clone();
    assert_eq!(failed_run["status"], "failed");

    let response = client
        .get(format!(
            "{}/dashboard?source={}",
            common::APP_HOST,
            source.as_str()
        ))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let dashboard: Value = response.json::<Value>().unwrap()["data"].clone();
    let priorities = dashboard["priority_items"].as_array().unwrap();
    let service_index = priority_index(priorities, "service", service["id"].as_i64().unwrap());
    let work_index = priority_index(priorities, "work_card", work_card["id"].as_i64().unwrap());
    let notification_index = priority_index(
        priorities,
        "notification",
        notification["id"].as_i64().unwrap(),
    );
    let run_index = priority_index(
        priorities,
        "connector_run",
        failed_run["id"].as_i64().unwrap(),
    );

    assert!(
        service_index < work_index
            && work_index < notification_index
            && notification_index < run_index,
        "priority order should be service, work, notification, connector run; got {priorities:?}"
    );
    assert_eq!(priorities[service_index]["severity"], "down");
    assert_eq!(priorities[work_index]["severity"], "blocked");
    assert_eq!(priorities[notification_index]["severity"], "critical");
    assert_eq!(priorities[run_index]["severity"], "failed");

    let response = client
        .delete(format!(
            "{}/connectors/{}",
            common::APP_HOST,
            source.as_str()
        ))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    common::delete_test_notification(&client, notification);
    common::delete_test_work_card(&client, work_card);
    common::delete_test_service(&client, service);
    common::delete_test_maintainer(&client, maintainer);
    common::delete_test_user(auth.user_id);
}

#[test]
fn test_me_overview_returns_user_owned_operational_context() {
    let client = Client::new();
    let admin = common::create_admin_auth(&client);
    let owner = common::create_test_auth(&client, "member");
    let maintainer = common::create_test_maintainer(&client);
    let source = common::unique_name("meov");
    let checked_at = (Utc::now().naive_utc() - Duration::minutes(10))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();

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
        .post(format!(
            "{}/connectors/{}/service-health/import",
            common::APP_HOST,
            source.as_str()
        ))
        .bearer_auth(&admin.token)
        .json(&json!({
            "items": [{
                "external_id": "me-overview-service",
                "maintainer_id": maintainer["id"],
                "slug": common::unique_name("svc"),
                "name": "Me Overview Service",
                "lifecycle_status": "active",
                "health_status": "down",
                "description": "Owned service for me overview",
                "repository_url": "https://github.com/acme/me-overview",
                "dashboard_url": "https://grafana.acme.test/d/me-overview",
                "runbook_url": "https://docs.acme.test/runbooks/me-overview",
                "last_checked_at": format!("{checked_at}Z")
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let service: Value = response.json::<Value>().unwrap()["data"]["data"][0].clone();

    let response = client
        .post(format!("{}/packages", common::APP_HOST))
        .bearer_auth(&admin.token)
        .json(&json!({
            "maintainer_id": maintainer["id"],
            "slug": common::unique_name("pkg"),
            "name": "Me Overview Package",
            "version": "1.0.0",
            "status": "active",
            "description": "Owned package for me overview",
            "repository_url": "https://github.com/acme/me-overview-package",
            "documentation_url": null,
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let package: Value = response.json::<Value>().unwrap()["data"].clone();

    let response = client
        .post(format!("{}/work-cards", common::APP_HOST))
        .bearer_auth(&admin.token)
        .json(&json!({
            "source": source.as_str(),
            "external_id": common::unique_name("work"),
            "title": "Investigate owned service outage",
            "status": "in_progress",
            "priority": "urgent",
            "assignee": "service-owner",
            "due_at": null,
            "url": "https://dev.azure.test/work-items/me-overview"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let work_card: Value = response.json::<Value>().unwrap()["data"].clone();

    let response = client
        .post(format!("{}/notifications", common::APP_HOST))
        .bearer_auth(&admin.token)
        .json(&json!({
            "source": source.as_str(),
            "external_id": common::unique_name("message"),
            "title": "ERP approval waiting on owned service",
            "body": "Deployment access needs review.",
            "severity": "critical",
            "is_read": false,
            "url": "https://erp.acme.test/messages/me-overview"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let notification: Value = response.json::<Value>().unwrap()["data"].clone();

    let response = client
        .post(format!(
            "{}/connectors/{}/work-cards/import",
            common::APP_HOST,
            source.as_str()
        ))
        .bearer_auth(&admin.token)
        .json(&json!({
            "items": [{
                "external_id": "bad-owned-work",
                "title": "Invalid owned work",
                "status": "todo",
                "priority": "not-a-priority",
                "assignee": null,
                "due_at": null,
                "url": null
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let failed_run: Value = response.json::<Value>().unwrap()["data"]["run"].clone();
    assert_eq!(failed_run["status"], "failed");

    let response = client
        .get(format!("{}/me/overview", common::APP_HOST))
        .bearer_auth(&owner.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let overview: Value = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(overview["user"]["id"].as_i64(), Some(owner.user_id as i64));
    assert_eq!(overview["summary"]["maintainers"], 1);
    assert_eq!(overview["summary"]["services"], 1);
    assert_eq!(overview["summary"]["unhealthy_services"], 1);
    assert_eq!(overview["summary"]["packages"], 1);
    assert_eq!(overview["summary"]["open_work_cards"], 1);
    assert_eq!(overview["summary"]["unread_notifications"], 1);
    assert_eq!(overview["summary"]["failed_connector_runs"], 1);
    assert_eq!(overview["maintainers"][0]["role"], "owner");
    assert_contains_id(&overview["services"], service["id"].as_i64().unwrap());
    assert_contains_id(&overview["packages"], package["id"].as_i64().unwrap());
    assert_contains_id(
        &overview["open_work_cards"],
        work_card["id"].as_i64().unwrap(),
    );
    assert_contains_id(
        &overview["unread_notifications"],
        notification["id"].as_i64().unwrap(),
    );
    assert_contains_id(
        &overview["failed_connector_runs"],
        failed_run["id"].as_i64().unwrap(),
    );
    assert_eq!(overview["health_history"]["summary"]["checks"], 1);
    assert_eq!(overview["health_history"]["summary"]["down_checks"], 1);
    assert_eq!(
        overview["health_history"]["recent_incidents"][0]["service_id"],
        service["id"]
    );
    assert!(overview["operations"]["worker_status"].as_str().is_some());
    assert!(
        overview["operations"]["worker_stale_after_seconds"]
            .as_i64()
            .unwrap()
            > 0
    );
    assert!(overview["operations"]["health_data_stale"].is_boolean());
    assert!(overview["operations"]["latest_health_check_at"]
        .as_str()
        .is_some());
    let priority_items = overview["priority_items"].as_array().unwrap();
    assert_eq!(priority_items[0]["kind"], "service");
    assert_eq!(priority_items[0]["severity"], "down");
    assert_eq!(priority_items[1]["kind"], "work_card");
    assert_eq!(priority_items[1]["severity"], "urgent");
    assert_eq!(priority_items[2]["kind"], "notification");
    assert_eq!(priority_items[2]["severity"], "critical");
    assert_eq!(priority_items[3]["kind"], "connector_run");
    assert_eq!(priority_items[3]["severity"], "failed");

    let response = client
        .delete(format!(
            "{}/connectors/{}",
            common::APP_HOST,
            source.as_str()
        ))
        .bearer_auth(&admin.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    common::delete_test_notification(&client, notification);
    common::delete_test_work_card(&client, work_card);
    common::delete_test_service(&client, service);
    common::delete_test_package(&client, package);
    common::delete_test_maintainer(&client, maintainer);
    common::delete_test_user(owner.user_id);
    common::delete_test_user(admin.user_id);
}

fn assert_contains_id(items: &Value, id: i64) {
    assert!(
        items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"].as_i64() == Some(id)),
        "expected dashboard collection to include id {id}, got {items:?}"
    );
}

fn assert_not_contains_id(items: &Value, id: i64) {
    assert!(
        !items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"].as_i64() == Some(id)),
        "expected dashboard collection to exclude id {id}, got {items:?}"
    );
}

fn priority_index(items: &[Value], kind: &str, id: i64) -> usize {
    items
        .iter()
        .position(|item| {
            item["kind"].as_str() == Some(kind) && item["record_id"].as_i64() == Some(id)
        })
        .unwrap_or_else(|| panic!("expected priority item kind={kind} id={id}, got {items:?}"))
}
