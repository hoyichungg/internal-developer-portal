use chrono::{Duration, Utc};
use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;

pub mod common;

#[test]
fn test_connectors_import_dashboard_sources_idempotently() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer = common::create_test_maintainer(&client);
    let monitoring_source = common::unique_name("monitoring");
    let work_source = common::unique_name("azure_devops");
    let notification_source = common::unique_name("outlook");

    let response = client
        .post(format!(
            "{}/connectors/{}/service-health/import",
            common::APP_HOST,
            monitoring_source
        ))
        .bearer_auth(&auth.token)
        .json(&json!({
            "items": [{
                "external_id": "svc-identity",
                "maintainer_id": maintainer["id"],
                "slug": "identity-service",
                "name": "Identity Service",
                "lifecycle_status": "active",
                "health_status": "degraded",
                "description": "Authentication and user session service",
                "repository_url": "https://github.com/acme/identity-service",
                "dashboard_url": "https://grafana.acme.test/d/identity",
                "runbook_url": "https://docs.acme.test/runbooks/identity",
                "last_checked_at": null
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let service_import = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(
        service_import["source"].as_str().unwrap(),
        monitoring_source
    );
    assert_eq!(service_import["target"], "service_health");
    assert_eq!(service_import["imported"], 1);
    assert_eq!(service_import["failed"], 0);
    assert_eq!(service_import["run"]["status"], "success");
    assert_eq!(service_import["run"]["success_count"], 1);
    assert_eq!(service_import["run"]["failure_count"], 0);
    assert_eq!(service_import["run"]["error_message"], Value::Null);

    let service = service_import["data"][0].clone();
    assert_eq!(service["source"].as_str().unwrap(), monitoring_source);
    assert_eq!(service["external_id"], "svc-identity");
    assert_eq!(service["health_status"], "degraded");

    let response = client
        .post(format!(
            "{}/connectors/{}/service-health/import",
            common::APP_HOST,
            monitoring_source
        ))
        .bearer_auth(&auth.token)
        .json(&json!({
            "items": [{
                "external_id": "svc-identity",
                "maintainer_id": maintainer["id"],
                "slug": "identity-service",
                "name": "Identity Service",
                "lifecycle_status": "active",
                "health_status": "healthy",
                "description": "Authentication and user session service",
                "repository_url": "https://github.com/acme/identity-service",
                "dashboard_url": "https://grafana.acme.test/d/identity",
                "runbook_url": "https://docs.acme.test/runbooks/identity",
                "last_checked_at": null
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let updated_service = response.json::<Value>().unwrap()["data"]["data"][0].clone();
    assert_eq!(updated_service["id"], service["id"]);
    assert_eq!(updated_service["health_status"], "healthy");

    let runs = client
        .get(format!(
            "{}/connectors/runs?source={}&target=service_health",
            common::APP_HOST,
            monitoring_source
        ))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(runs.status(), StatusCode::OK);
    let runs = runs.json::<Value>().unwrap()["data"].clone();
    assert!(
        runs.as_array().unwrap().len() >= 2,
        "expected at least two service health runs, got {runs:?}"
    );
    assert!(runs.as_array().unwrap().iter().all(|run| {
        run["source"].as_str() == Some(monitoring_source.as_str())
            && run["target"].as_str() == Some("service_health")
    }));

    let response = client
        .post(format!(
            "{}/connectors/{}/work-cards/import",
            common::APP_HOST,
            work_source
        ))
        .bearer_auth(&auth.token)
        .json(&json!({
            "items": [{
                "external_id": "ADO-42",
                "title": "Review catalog deployment pipeline",
                "status": "in_progress",
                "priority": "high",
                "assignee": "platform-team",
                "due_at": null,
                "url": "https://dev.azure.test/work-items/42"
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let work_card = response.json::<Value>().unwrap()["data"]["data"][0].clone();
    assert_eq!(work_card["source"].as_str().unwrap(), work_source);
    assert_eq!(work_card["external_id"], "ADO-42");

    let response = client
        .post(format!(
            "{}/connectors/{}/notifications/import",
            common::APP_HOST,
            notification_source
        ))
        .bearer_auth(&auth.token)
        .json(&json!({
            "items": [{
                "external_id": "mail-9001",
                "title": "Daily deployment review",
                "body": "Morning release notes are ready.",
                "severity": "info",
                "is_read": false,
                "url": "https://outlook.office.test/mail/9001"
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let notification = response.json::<Value>().unwrap()["data"]["data"][0].clone();
    assert_eq!(
        notification["source"].as_str().unwrap(),
        notification_source
    );
    assert_eq!(notification["external_id"], "mail-9001");

    let dashboard = client
        .get(format!("{}/dashboard", common::APP_HOST))
        .bearer_auth(&auth.token)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]
        .clone();
    assert_contains_id(
        &dashboard["service_health"],
        service["id"].as_i64().unwrap(),
    );
    assert_contains_id(&dashboard["work_cards"], work_card["id"].as_i64().unwrap());
    assert_contains_id(
        &dashboard["notifications"],
        notification["id"].as_i64().unwrap(),
    );

    let source_dashboard = client
        .get(format!(
            "{}/dashboard?source={}",
            common::APP_HOST,
            work_source
        ))
        .bearer_auth(&auth.token)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]
        .clone();
    assert_eq!(
        source_dashboard["scope"]["source"].as_str().unwrap(),
        work_source
    );
    assert_contains_id(
        &source_dashboard["work_cards"],
        work_card["id"].as_i64().unwrap(),
    );
    assert_not_contains_id(
        &source_dashboard["service_health"],
        service["id"].as_i64().unwrap(),
    );
    assert_not_contains_id(
        &source_dashboard["notifications"],
        notification["id"].as_i64().unwrap(),
    );

    common::delete_test_notification(&client, notification);
    common::delete_test_work_card(&client, work_card);
    common::delete_test_service(&client, updated_service);
    common::delete_test_maintainer(&client, maintainer);
}

#[test]
fn test_connector_registry_can_be_managed_and_tracks_run_state() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer = common::create_test_maintainer(&client);
    let source = common::unique_name("monitoring");

    let response = client
        .post(format!("{}/connectors", common::APP_HOST))
        .bearer_auth(&auth.token)
        .json(&json!({
            "source": source.clone(),
            "kind": "monitoring",
            "display_name": "Monitoring Connector",
            "status": "active"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let connector = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(connector["source"].as_str().unwrap(), source);
    assert_eq!(connector["kind"], "monitoring");
    assert_eq!(connector["display_name"], "Monitoring Connector");
    assert_eq!(connector["last_run_at"], Value::Null);
    assert_eq!(connector["last_success_at"], Value::Null);

    let response = client
        .post(format!(
            "{}/connectors/{}/service-health/import",
            common::APP_HOST,
            source
        ))
        .bearer_auth(&auth.token)
        .json(&json!({
            "items": [{
                "external_id": "svc-registry",
                "maintainer_id": maintainer["id"],
                "slug": "registry-service",
                "name": "Registry Service",
                "lifecycle_status": "active",
                "health_status": "healthy",
                "description": "Service synced from connector registry test",
                "repository_url": "https://github.com/acme/registry-service",
                "dashboard_url": "https://grafana.acme.test/d/registry",
                "runbook_url": "https://docs.acme.test/runbooks/registry",
                "last_checked_at": null
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let service = response.json::<Value>().unwrap()["data"]["data"][0].clone();

    let response = client
        .get(format!("{}/connectors/{}", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let connector = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(connector["source"].as_str().unwrap(), source);
    assert_eq!(connector["status"], "active");
    assert_ne!(connector["last_run_at"], Value::Null);
    assert_ne!(connector["last_success_at"], Value::Null);

    let response = client
        .get(format!("{}/connectors", common::APP_HOST))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let connectors = response.json::<Value>().unwrap()["data"].clone();
    assert!(connectors
        .as_array()
        .unwrap()
        .iter()
        .any(|connector| connector["source"].as_str() == Some(source.as_str())));

    let response = client
        .put(format!("{}/connectors/{}", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .json(&json!({
            "kind": "monitoring",
            "display_name": "Monitoring Connector Paused",
            "status": "paused"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let connector = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(connector["display_name"], "Monitoring Connector Paused");
    assert_eq!(connector["status"], "paused");

    let response = client
        .delete(format!("{}/connectors/{}", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    common::delete_test_service(&client, service);
    common::delete_test_maintainer(&client, maintainer);
    common::delete_test_user(auth.user_id);
}

#[test]
fn test_connector_config_manual_run_records_item_errors() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let source = common::unique_name("azure_devops_runtime");

    let response = client
        .post(format!("{}/connectors", common::APP_HOST))
        .bearer_auth(&auth.token)
        .json(&json!({
            "source": source.clone(),
            "kind": "azure_devops",
            "display_name": "Azure DevOps Runtime",
            "status": "active"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let sample_payload = json!({
        "items": [{
            "external_id": "ADO-runtime-ok",
            "title": "Runtime generated work card",
            "status": "in_progress",
            "priority": "high",
            "assignee": "platform-team",
            "due_at": null,
            "url": "https://dev.azure.test/work-items/runtime-ok"
        }, {
            "external_id": "ADO-runtime-bad",
            "title": "Runtime generated invalid work card",
            "status": "todo",
            "priority": "not-a-priority",
            "assignee": null,
            "due_at": null,
            "url": null
        }]
    })
    .to_string();

    let response = client
        .put(format!("{}/connectors/{}/config", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .json(&json!({
            "target": "work_cards",
            "enabled": true,
            "schedule_cron": null,
            "config": "{\"project\":\"platform\"}",
            "sample_payload": sample_payload
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let config = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(config["source"].as_str().unwrap(), source);
    assert_eq!(config["target"], "work_cards");
    assert_eq!(config["enabled"], true);

    let response = client
        .get(format!("{}/connectors/{}/config", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().unwrap()["data"]["target"],
        "work_cards"
    );

    let response = client
        .post(format!("{}/connectors/{}/runs", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .json(&json!({}))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let execution = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(execution["source"].as_str().unwrap(), source);
    assert_eq!(execution["target"], "work_cards");
    assert_eq!(execution["imported"], 1);
    assert_eq!(execution["failed"], 1);
    assert_eq!(execution["run"]["status"], "partial_success");
    assert_eq!(execution["run"]["success_count"], 1);
    assert_eq!(execution["run"]["failure_count"], 1);
    assert_eq!(execution["errors"].as_array().unwrap().len(), 1);
    assert_eq!(execution["item_errors"].as_array().unwrap().len(), 1);
    assert_eq!(execution["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        execution["item_errors"][0]["external_id"],
        "ADO-runtime-bad"
    );
    assert!(execution["item_errors"][0]["message"]
        .as_str()
        .unwrap()
        .contains("priority"));
    let work_card = execution["data"][0].clone();
    assert_eq!(work_card["external_id"], "ADO-runtime-ok");
    let imported_item = execution["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["status"].as_str() == Some("imported"))
        .expect("run items should include imported item");
    assert_eq!(imported_item["record_id"], work_card["id"]);
    assert_eq!(imported_item["external_id"], "ADO-runtime-ok");
    assert!(imported_item["snapshot"]
        .as_str()
        .unwrap()
        .contains("Runtime generated work card"));
    let failed_item = execution["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["status"].as_str() == Some("failed"))
        .expect("run items should include failed item");
    assert_eq!(failed_item["external_id"], "ADO-runtime-bad");

    let run_id = execution["run"]["id"].as_i64().unwrap();
    let response = client
        .get(format!("{}/connectors/runs/{}", common::APP_HOST, run_id))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let detail = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(detail["run"]["id"].as_i64().unwrap(), run_id);
    assert_eq!(detail["items"].as_array().unwrap().len(), 2);
    assert_eq!(detail["item_errors"].as_array().unwrap().len(), 1);

    let response = client
        .post(format!(
            "{}/connectors/runs/{}/retry",
            common::APP_HOST,
            run_id
        ))
        .bearer_auth(&auth.token)
        .json(&json!({}))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let retry = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(retry["run"]["status"], "queued");
    assert_eq!(retry["run"]["trigger"], "retry");
    let retry_id = retry["run"]["id"].as_i64().unwrap();
    let retry_detail = wait_for_run_status(&client, &auth.token, retry_id, "partial_success");
    assert_eq!(retry_detail["run"]["trigger"], "retry");
    assert_eq!(retry_detail["run"]["success_count"], 1);
    assert_eq!(retry_detail["run"]["failure_count"], 1);

    let connector = client
        .get(format!("{}/connectors/{}", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]
        .clone();
    assert_ne!(connector["last_run_at"], Value::Null);
    assert_eq!(connector["last_success_at"], Value::Null);

    common::delete_test_work_card(&client, work_card);
    let response = client
        .delete(format!("{}/connectors/{}", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    common::delete_test_user(auth.user_id);
}

#[test]
fn test_connector_manual_run_queues_and_worker_executes_it() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let source = common::unique_name("outlook_runtime");

    let response = client
        .post(format!("{}/connectors", common::APP_HOST))
        .bearer_auth(&auth.token)
        .json(&json!({
            "source": source.clone(),
            "kind": "outlook",
            "display_name": "Outlook Runtime",
            "status": "active"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = client
        .put(format!("{}/connectors/{}/config", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .json(&json!({
            "target": "notifications",
            "enabled": true,
            "schedule_cron": null,
            "config": "{}",
            "sample_payload": "{\"items\":[]}"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = client
        .post(format!("{}/connectors/{}/runs", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .json(&json!({ "mode": "queue" }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let queued = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(queued["run"]["status"], "queued");
    assert_eq!(queued["run"]["trigger"], "manual");
    assert_eq!(queued["run"]["finished_at"], Value::Null);
    assert_eq!(queued["imported"], 0);
    assert_eq!(queued["failed"], 0);

    let run_id = queued["run"]["id"].as_i64().unwrap();
    let detail = wait_for_run_status(&client, &auth.token, run_id, "success");
    assert_eq!(detail["run"]["trigger"], "manual");
    assert_ne!(detail["run"]["claimed_at"], Value::Null);
    assert!(detail["run"]["worker_id"]
        .as_str()
        .unwrap()
        .starts_with("connector-worker-"));
    assert!(detail["item_errors"].as_array().unwrap().is_empty());

    let response = client
        .delete(format!("{}/connectors/{}", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    common::delete_test_user(auth.user_id);
}

#[test]
fn test_connector_operations_reports_worker_and_retention_history() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let operations = wait_for_connector_operations(&client, &auth.token);

    assert!(operations["stale_after_seconds"].as_i64().unwrap() > 0);
    assert!(
        operations["workers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|worker| {
                worker["worker_id"]
                    .as_str()
                    .unwrap()
                    .starts_with("connector-worker-")
                    && worker["status"].as_str().is_some()
                    && worker["last_seen_at"].as_str().is_some()
            }),
        "operations should include worker heartbeat: {operations:?}"
    );
    assert!(
        operations["maintenance_runs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|run| {
                run["task"].as_str() == Some("retention_cleanup")
                    && run["status"].as_str().is_some()
                    && run["finished_at"].as_str().is_some()
            }),
        "operations should include retention cleanup history: {operations:?}"
    );

    common::delete_test_user(auth.user_id);
}

#[test]
fn test_scheduler_enqueues_due_config_and_worker_executes_it() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let source = common::unique_name("azure_devops_scheduler");

    let response = client
        .post(format!("{}/connectors", common::APP_HOST))
        .bearer_auth(&auth.token)
        .json(&json!({
            "source": source.clone(),
            "kind": "azure_devops",
            "display_name": "Azure DevOps Scheduler",
            "status": "active"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = client
        .put(format!("{}/connectors/{}/config", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .json(&json!({
            "target": "work_cards",
            "enabled": true,
            "schedule_cron": "@every 1s",
            "config": "{\"project\":\"platform\"}",
            "sample_payload": json!({
                "items": [{
                    "external_id": "ADO-scheduled",
                    "title": "Scheduled connector work card",
                    "status": "todo",
                    "priority": "medium",
                    "assignee": "platform-team",
                    "due_at": null,
                    "url": "https://dev.azure.test/work-items/scheduled"
                }]
            }).to_string()
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let config = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(config["target"], "work_cards");
    assert_ne!(config["next_run_at"], Value::Null);

    let run = wait_for_scheduled_run(&client, &auth.token, &source, "work_cards");
    assert_eq!(run["trigger"], "scheduled");
    assert_eq!(run["status"], "success");
    assert_eq!(run["success_count"], 1);
    assert_eq!(run["failure_count"], 0);
    assert!(run["worker_id"]
        .as_str()
        .unwrap()
        .starts_with("connector-worker-"));

    let dashboard = client
        .get(format!("{}/dashboard?source={}", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]
        .clone();
    let work_card = dashboard["work_cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["external_id"].as_str() == Some("ADO-scheduled"))
        .cloned()
        .expect("scheduled work card should be visible on the dashboard");

    let config = client
        .get(format!("{}/connectors/{}/config", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]
        .clone();
    assert_ne!(config["last_scheduled_at"], Value::Null);
    assert_ne!(config["last_scheduled_run_id"], Value::Null);

    let response = client
        .delete(format!("{}/connectors/{}", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    common::delete_test_work_card(&client, work_card);
    common::delete_test_user(auth.user_id);
}

#[test]
fn test_sample_notification_adapters_import_product_core_feeds() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);

    let calendar = execute_sample_notification_adapter(
        &client,
        &auth.token,
        SampleNotificationAdapterCase {
            source_prefix: "calendar_adapter",
            kind: "calendar",
            display_name: "Calendar Adapter",
            config: json!({
                "adapter": "calendar_sample",
                "events": [{
                    "id": "calendar-standup",
                    "subject": "Platform standup in 15 minutes",
                    "organizer": "Taylor Lin",
                    "location": "Teams",
                    "starts_at": "2026-05-11T09:30:00Z",
                    "webLink": "https://calendar.example.test/events/calendar-standup"
                }]
            }),
            expected_external_id: "calendar-standup",
            expected_title: "Platform standup in 15 minutes",
            expected_severity: "info",
        },
    );
    assert!(calendar.notification["body"]
        .as_str()
        .unwrap()
        .contains("Organizer: Taylor Lin"));

    let mail = execute_sample_notification_adapter(
        &client,
        &auth.token,
        SampleNotificationAdapterCase {
            source_prefix: "outlook_mail_adapter",
            kind: "outlook",
            display_name: "Outlook Mail Adapter",
            config: json!({
                "adapter": "outlook_mail_sample",
                "messages": [{
                    "id": "mail-release-brief",
                    "subject": "Release brief ready for review",
                    "from": { "emailAddress": { "name": "Release Bot", "address": "release-bot@example.test" } },
                    "bodyPreview": "API deploy window moved to 15:30.",
                    "importance": "high",
                    "webLink": "https://outlook.example.test/mail/release-brief"
                }]
            }),
            expected_external_id: "mail-release-brief",
            expected_title: "Release brief ready for review",
            expected_severity: "warning",
        },
    );
    assert!(mail.notification["body"]
        .as_str()
        .unwrap()
        .contains("From: Release Bot"));

    let erp = execute_sample_notification_adapter(
        &client,
        &auth.token,
        SampleNotificationAdapterCase {
            source_prefix: "erp_messages_adapter",
            kind: "erp",
            display_name: "ERP Messages Adapter",
            config: json!({
                "adapter": "erp_messages_sample",
                "messages": [{
                    "id": "erp-access-approval",
                    "title": "Deployment access approval waiting",
                    "message": "Mock ERP private message for local development.",
                    "requires_approval": true
                }]
            }),
            expected_external_id: "erp-access-approval",
            expected_title: "Deployment access approval waiting",
            expected_severity: "warning",
        },
    );
    assert_eq!(erp.notification["url"], Value::Null);

    for connector in [&calendar, &mail, &erp] {
        let response = client
            .delete(format!(
                "{}/connectors/{}",
                common::APP_HOST,
                connector.source
            ))
            .bearer_auth(&auth.token)
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        common::delete_test_notification(&client, connector.notification.clone());
    }

    common::delete_test_user(auth.user_id);
}

#[test]
fn test_azure_devops_adapter_fetches_and_normalizes_work_items() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let source = common::unique_name("azure_devops_adapter");
    let mock = start_azure_devops_mock();

    let response = client
        .post(format!("{}/connectors", common::APP_HOST))
        .bearer_auth(&auth.token)
        .json(&json!({
            "source": source.clone(),
            "kind": "azure_devops",
            "display_name": "Azure DevOps Adapter",
            "status": "active"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let adapter_config = json!({
        "adapter": "azure_devops",
        "wiql_url": format!("{}/wiql", mock.base_url),
        "work_items_url": format!("{}/workitemsbatch", mock.base_url),
        "wiql": "SELECT [System.Id] FROM WorkItems",
        "personal_access_token": "test-pat",
        "web_url_base": "https://dev.azure.test/workitems"
    })
    .to_string();

    let response = client
        .put(format!("{}/connectors/{}/config", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .json(&json!({
            "target": "work_cards",
            "enabled": true,
            "schedule_cron": null,
            "config": adapter_config,
            "sample_payload": "{\"items\":[]}"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let config_response = response.json::<Value>().unwrap()["data"].clone();
    assert!(
        !config_response["config"]
            .as_str()
            .unwrap()
            .contains("test-pat"),
        "connector config response must not expose Azure DevOps PAT"
    );
    assert!(
        config_response["config"]
            .as_str()
            .unwrap()
            .contains("***redacted***"),
        "connector config response should redact secrets"
    );
    let config_response = client
        .get(format!("{}/connectors/{}/config", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]
        .clone();
    assert!(
        !config_response["config"]
            .as_str()
            .unwrap()
            .contains("test-pat"),
        "connector config read response must not expose Azure DevOps PAT"
    );
    let redacted_config = config_response["config"].as_str().unwrap();
    let mut redacted_config: Value = serde_json::from_str(redacted_config).unwrap();
    redacted_config["timeout_seconds"] = json!(5);

    let response = client
        .put(format!("{}/connectors/{}/config", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .json(&json!({
            "target": "work_cards",
            "enabled": true,
            "schedule_cron": null,
            "config": redacted_config.to_string(),
            "sample_payload": "{\"items\":[]}"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = client
        .post(format!("{}/connectors/{}/runs", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .json(&json!({ "mode": "queue" }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let queued = response.json::<Value>().unwrap()["data"].clone();
    let run_id = queued["run"]["id"].as_i64().unwrap();
    let detail = wait_for_run_status(&client, &auth.token, run_id, "success");
    assert_eq!(detail["run"]["success_count"], 2);
    assert_eq!(detail["run"]["failure_count"], 0);

    let dashboard = client
        .get(format!("{}/dashboard?source={}", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]
        .clone();
    let work_cards = dashboard["work_cards"].as_array().unwrap();
    let active_item = work_cards
        .iter()
        .find(|item| item["external_id"].as_str() == Some("42"))
        .cloned()
        .expect("adapter should import active Azure DevOps item");
    let blocked_item = work_cards
        .iter()
        .find(|item| item["external_id"].as_str() == Some("43"))
        .cloned()
        .expect("adapter should import blocked Azure DevOps item");

    assert_eq!(active_item["title"], "Ship connector adapter");
    assert_eq!(active_item["status"], "in_progress");
    assert_eq!(active_item["priority"], "high");
    assert_eq!(active_item["assignee"], "Ada Lovelace");
    assert_eq!(active_item["url"], "https://dev.azure.test/workitems/42");
    assert_eq!(blocked_item["status"], "blocked");
    assert_eq!(blocked_item["priority"], "urgent");

    let requests = mock.join();
    assert!(
        requests
            .iter()
            .any(|request| request.starts_with("POST /wiql")),
        "adapter did not call WIQL endpoint: {requests:?}"
    );
    assert!(
        requests
            .iter()
            .any(|request| request.starts_with("POST /workitemsbatch")),
        "adapter did not call work item batch endpoint: {requests:?}"
    );
    assert!(
        requests.iter().all(|request| request.contains("Basic ")),
        "adapter should send Azure DevOps PAT with Basic auth: {requests:?}"
    );
    assert!(
        requests
            .iter()
            .all(|request| request.contains("Basic OnRlc3QtcGF0")),
        "redacted config round-trip must preserve the original PAT: {requests:?}"
    );

    let response = client
        .delete(format!("{}/connectors/{}", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    common::delete_test_work_card(&client, active_item);
    common::delete_test_work_card(&client, blocked_item);
    common::delete_test_user(auth.user_id);
}

#[test]
fn test_monitoring_adapter_fetches_and_normalizes_service_health() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let maintainer = common::create_test_maintainer(&client);
    let source = common::unique_name("monitoring_adapter");
    let checked_at = (Utc::now().naive_utc() - Duration::minutes(10))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let mock = start_monitoring_mock(json!({
        "services": [{
            "id": "identity-api",
            "name": "Identity API",
            "status": "ok",
            "summary": "Login and session service",
            "url": "https://grafana.example.test/d/identity",
            "runbook": "https://docs.example.test/runbooks/identity",
            "checked_at": format!("{checked_at}Z")
        }, {
            "id": "billing-worker",
            "name": "Billing Worker",
            "health": "critical",
            "repository": "https://github.com/acme/billing-worker"
        }, {
            "id": "內部服務",
            "name": "內部服務",
            "state": "warning"
        }]
    }));

    let response = client
        .post(format!("{}/connectors", common::APP_HOST))
        .bearer_auth(&auth.token)
        .json(&json!({
            "source": source.clone(),
            "kind": "monitoring",
            "display_name": "Monitoring Adapter",
            "status": "active"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let adapter_config = json!({
        "adapter": "monitoring",
        "url": format!("{}/service-health", mock.base_url),
        "default_maintainer_id": maintainer["id"],
        "bearer_token": "monitor-token",
        "timeout_seconds": 5
    })
    .to_string();

    let response = client
        .put(format!("{}/connectors/{}/config", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .json(&json!({
            "target": "service_health",
            "enabled": true,
            "schedule_cron": null,
            "config": adapter_config,
            "sample_payload": "{\"items\":[]}"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let config_response = response.json::<Value>().unwrap()["data"].clone();
    assert!(
        !config_response["config"]
            .as_str()
            .unwrap()
            .contains("monitor-token"),
        "connector config response must not expose monitoring bearer token"
    );
    assert!(
        config_response["config"]
            .as_str()
            .unwrap()
            .contains("***redacted***"),
        "connector config response should redact monitoring secrets"
    );

    let response = client
        .post(format!("{}/connectors/{}/runs", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .json(&json!({}))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let execution = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(execution["source"].as_str().unwrap(), source);
    assert_eq!(execution["target"], "service_health");
    assert_eq!(execution["imported"], 3, "execution: {execution:#}");
    assert_eq!(execution["failed"], 0);
    assert_eq!(execution["run"]["status"], "success");
    let run_id = execution["run"]["id"].as_i64().unwrap();

    let services = execution["data"].as_array().unwrap();
    let identity = services
        .iter()
        .find(|service| service["external_id"].as_str() == Some("identity-api"))
        .cloned()
        .expect("monitoring adapter should import identity-api");
    let billing = services
        .iter()
        .find(|service| service["external_id"].as_str() == Some("billing-worker"))
        .cloned()
        .expect("monitoring adapter should import billing-worker");
    let internal = services
        .iter()
        .find(|service| service["external_id"].as_str() == Some("內部服務"))
        .cloned()
        .expect("monitoring adapter should import non-ASCII service names");

    assert_eq!(identity["maintainer_id"], maintainer["id"]);
    assert_eq!(identity["slug"], "identity-api");
    assert_eq!(identity["health_status"], "healthy");
    assert_eq!(identity["description"], "Login and session service");
    assert_eq!(
        identity["dashboard_url"],
        "https://grafana.example.test/d/identity"
    );
    assert_eq!(identity["last_checked_at"], checked_at);
    assert_eq!(billing["health_status"], "down");
    assert_eq!(
        billing["repository_url"],
        "https://github.com/acme/billing-worker"
    );
    assert_eq!(internal["health_status"], "degraded");
    assert!(
        internal["slug"].as_str().unwrap().starts_with("service-"),
        "non-ASCII service names should receive a stable ASCII slug"
    );

    let dashboard = client
        .get(format!("{}/dashboard?source={}", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]
        .clone();
    assert_contains_id(
        &dashboard["service_health"],
        identity["id"].as_i64().unwrap(),
    );
    assert_contains_id(
        &dashboard["service_health"],
        billing["id"].as_i64().unwrap(),
    );
    assert_contains_id(
        &dashboard["service_health"],
        internal["id"].as_i64().unwrap(),
    );
    assert_eq!(dashboard["health_history"]["summary"]["checks"], 3);
    assert_eq!(dashboard["health_history"]["summary"]["healthy_checks"], 1);
    assert_eq!(dashboard["health_history"]["summary"]["degraded_checks"], 1);
    assert_eq!(dashboard["health_history"]["summary"]["down_checks"], 1);
    assert_contains_service_check(
        &dashboard["health_history"]["recent_checks"],
        identity["id"].as_i64().unwrap(),
    );
    assert_contains_service_check(
        &dashboard["health_history"]["recent_incidents"],
        billing["id"].as_i64().unwrap(),
    );

    let run_detail = client
        .get(format!("{}/connectors/runs/{}", common::APP_HOST, run_id))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(run_detail.status(), StatusCode::OK);
    let run_detail = run_detail.json::<Value>().unwrap()["data"].clone();
    assert_eq!(run_detail["run"]["id"], run_id);
    assert_eq!(run_detail["items"].as_array().unwrap().len(), 3);
    assert_eq!(run_detail["item_errors"].as_array().unwrap().len(), 0);
    assert_eq!(run_detail["health_checks"].as_array().unwrap().len(), 3);
    assert!(run_detail["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["status"].as_str() == Some("imported")
            && item["record_id"] == identity["id"]
            && item["external_id"].as_str() == Some("identity-api")));
    assert_contains_service_check(
        &run_detail["health_checks"],
        identity["id"].as_i64().unwrap(),
    );
    assert_eq!(
        run_detail["health_checks"][0].get("raw_payload"),
        None,
        "run detail API should not expose raw health check payloads"
    );

    let requests = mock.join();
    assert!(
        requests
            .iter()
            .any(|request| request.starts_with("GET /service-health")),
        "monitoring adapter did not call service-health endpoint: {requests:?}"
    );
    assert!(
        requests.iter().all(|request| request
            .to_ascii_lowercase()
            .contains("authorization: bearer monitor-token")),
        "monitoring adapter should send bearer token: {requests:?}"
    );

    let response = client
        .delete(format!("{}/connectors/{}", common::APP_HOST, source))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    common::delete_test_service(&client, identity);
    common::delete_test_service(&client, billing);
    common::delete_test_service(&client, internal);
    common::delete_test_maintainer(&client, maintainer);
    common::delete_test_user(auth.user_id);
}

#[test]
fn test_connector_runs_record_failed_items() {
    let client = Client::new();
    let auth = common::create_admin_auth(&client);
    let source = common::unique_name("azure_devops");

    let response = client
        .post(format!(
            "{}/connectors/{}/work-cards/import",
            common::APP_HOST,
            source
        ))
        .bearer_auth(&auth.token)
        .json(&json!({
            "items": [{
                "external_id": "ADO-invalid",
                "title": "Invalid work item",
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

    let import = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(import["source"].as_str().unwrap(), source);
    assert_eq!(import["target"], "work_cards");
    assert_eq!(import["imported"], 0);
    assert_eq!(import["failed"], 1);
    assert!(import["data"].as_array().unwrap().is_empty());
    assert_eq!(import["errors"].as_array().unwrap().len(), 1);
    assert_eq!(import["items"].as_array().unwrap().len(), 1);
    assert_eq!(import["items"][0]["status"], "failed");
    assert_eq!(import["items"][0]["external_id"], "ADO-invalid");
    assert_eq!(import["run"]["status"], "failed");
    assert_eq!(import["run"]["success_count"], 0);
    assert_eq!(import["run"]["failure_count"], 1);
    assert!(import["run"]["duration_ms"].as_i64().unwrap() >= 0);
    assert!(import["run"]["error_message"]
        .as_str()
        .unwrap()
        .contains("priority"));

    let runs = client
        .get(format!(
            "{}/connectors/runs?source={}&target=work_cards",
            common::APP_HOST,
            source
        ))
        .bearer_auth(&auth.token)
        .send()
        .unwrap();
    assert_eq!(runs.status(), StatusCode::OK);
    let runs = runs.json::<Value>().unwrap()["data"].clone();
    assert!(runs.as_array().unwrap().iter().any(|run| {
        run["status"].as_str() == Some("failed")
            && run["success_count"].as_i64() == Some(0)
            && run["failure_count"].as_i64() == Some(1)
    }));

    common::delete_test_user(auth.user_id);
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

struct SampleNotificationExecution {
    source: String,
    notification: Value,
}

struct SampleNotificationAdapterCase {
    source_prefix: &'static str,
    kind: &'static str,
    display_name: &'static str,
    config: Value,
    expected_external_id: &'static str,
    expected_title: &'static str,
    expected_severity: &'static str,
}

fn execute_sample_notification_adapter(
    client: &Client,
    token: &str,
    case: SampleNotificationAdapterCase,
) -> SampleNotificationExecution {
    let source = common::unique_name(case.source_prefix);

    let response = client
        .post(format!("{}/connectors", common::APP_HOST))
        .bearer_auth(token)
        .json(&json!({
            "source": source.clone(),
            "kind": case.kind,
            "display_name": case.display_name,
            "status": "active"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = client
        .put(format!("{}/connectors/{}/config", common::APP_HOST, source))
        .bearer_auth(token)
        .json(&json!({
            "target": "notifications",
            "enabled": true,
            "schedule_cron": null,
            "config": case.config.to_string(),
            "sample_payload": "{\"items\":[]}"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = client
        .post(format!("{}/connectors/{}/runs", common::APP_HOST, source))
        .bearer_auth(token)
        .json(&json!({}))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let execution = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(execution["source"].as_str().unwrap(), source);
    assert_eq!(execution["target"], "notifications");
    assert_eq!(execution["imported"], 1, "execution: {execution:#}");
    assert_eq!(execution["failed"], 0);
    assert_eq!(execution["run"]["status"], "success");
    assert!(
        execution["item_errors"].as_array().unwrap().is_empty(),
        "sample adapter should not emit item errors: {execution:#}"
    );

    let notification = execution["data"][0].clone();
    assert_eq!(notification["source"].as_str().unwrap(), source);
    assert_eq!(notification["external_id"], case.expected_external_id);
    assert_eq!(notification["title"], case.expected_title);
    assert_eq!(notification["severity"], case.expected_severity);
    assert_eq!(notification["is_read"], false);

    let dashboard = client
        .get(format!("{}/dashboard?source={}", common::APP_HOST, source))
        .bearer_auth(token)
        .send()
        .unwrap()
        .json::<Value>()
        .unwrap()["data"]
        .clone();
    assert_contains_id(
        &dashboard["notifications"],
        notification["id"].as_i64().unwrap(),
    );

    SampleNotificationExecution {
        source,
        notification,
    }
}

fn assert_contains_service_check(items: &Value, service_id: i64) {
    assert!(
        items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["service_id"].as_i64() == Some(service_id)),
        "expected health checks to include service_id {service_id}, got {items:?}"
    );
}

fn wait_for_run_status(client: &Client, token: &str, run_id: i64, status: &str) -> Value {
    let mut last_detail = Value::Null;

    for _ in 0..30 {
        let response = client
            .get(format!("{}/connectors/runs/{}", common::APP_HOST, run_id))
            .bearer_auth(token)
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let detail = response.json::<Value>().unwrap()["data"].clone();
        if detail["run"]["status"].as_str() == Some(status) {
            return detail;
        }
        last_detail = detail;

        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    panic!("run {run_id} did not reach status {status}; last detail: {last_detail:?}");
}

fn wait_for_scheduled_run(client: &Client, token: &str, source: &str, target: &str) -> Value {
    for _ in 0..30 {
        let response = client
            .get(format!(
                "{}/connectors/runs?source={}&target={}",
                common::APP_HOST,
                source,
                target
            ))
            .bearer_auth(token)
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let runs = response.json::<Value>().unwrap()["data"].clone();
        if let Some(run) = runs.as_array().unwrap().iter().find(|run| {
            run["trigger"].as_str() == Some("scheduled")
                && run["status"].as_str() == Some("success")
        }) {
            return run.clone();
        }

        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    panic!("scheduled run for {source}/{target} did not finish");
}

fn wait_for_connector_operations(client: &Client, token: &str) -> Value {
    let mut last_operations = Value::Null;

    for _ in 0..40 {
        let response = client
            .get(format!("{}/connectors/operations", common::APP_HOST))
            .bearer_auth(token)
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let operations = response.json::<Value>().unwrap()["data"].clone();
        let has_worker = operations["workers"]
            .as_array()
            .is_some_and(|workers| !workers.is_empty());
        let has_retention_history = operations["maintenance_runs"]
            .as_array()
            .is_some_and(|runs| !runs.is_empty());

        if has_worker && has_retention_history {
            return operations;
        }

        last_operations = operations;
        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    panic!("connector operations did not report worker and retention history: {last_operations:?}");
}

struct AzureDevOpsMock {
    base_url: String,
    handle: thread::JoinHandle<Vec<String>>,
}

impl AzureDevOpsMock {
    fn join(self) -> Vec<String> {
        self.handle.join().unwrap()
    }
}

struct MonitoringMock {
    base_url: String,
    handle: thread::JoinHandle<Vec<String>>,
}

impl MonitoringMock {
    fn join(self) -> Vec<String> {
        self.handle.join().unwrap()
    }
}

fn start_monitoring_mock(body: Value) -> MonitoringMock {
    let listener = TcpListener::bind("0.0.0.0:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let base_url = format!("http://{}:{}", local_mock_host(), port);

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        write_http_json(&mut stream, &body);

        vec![request]
    });

    MonitoringMock { base_url, handle }
}

fn start_azure_devops_mock() -> AzureDevOpsMock {
    let listener = TcpListener::bind("0.0.0.0:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let base_url = format!("http://{}:{}", local_mock_host(), port);

    let handle = thread::spawn(move || {
        let mut requests = Vec::new();

        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            let body = if request.starts_with("POST /wiql") {
                json!({
                    "workItems": [
                        { "id": 42 },
                        { "id": 43 }
                    ]
                })
            } else if request.starts_with("POST /workitemsbatch") {
                json!({
                    "value": [{
                        "id": 42,
                        "fields": {
                            "System.Title": "Ship connector adapter",
                            "System.State": "Active",
                            "System.AssignedTo": {
                                "displayName": "Ada Lovelace"
                            },
                            "Microsoft.VSTS.Common.Priority": 2
                        }
                    }, {
                        "id": 43,
                        "fields": {
                            "System.Title": "Unblock deployment pipeline",
                            "System.State": "Blocked",
                            "System.AssignedTo": "Platform Team",
                            "Microsoft.VSTS.Common.Priority": 1
                        }
                    }]
                })
            } else {
                json!({ "error": "unexpected request" })
            };

            requests.push(request);
            write_http_json(&mut stream, &body);
        }

        requests
    });

    AzureDevOpsMock { base_url, handle }
}

fn local_mock_host() -> String {
    if let Ok(host) = std::env::var("CONNECTOR_MOCK_HOST") {
        return host;
    }

    if let Ok(host) = std::env::var("AZURE_DEVOPS_MOCK_HOST") {
        return host;
    }

    if docker_compose_service_is_running("app") || docker_compose_service_is_running("worker") {
        return "host.docker.internal".to_owned();
    }

    "127.0.0.1".to_owned()
}

fn docker_compose_service_is_running(service: &str) -> bool {
    let Ok(output) = Command::new("docker")
        .args(["compose", "ps", "--services", "--status", "running"])
        .output()
    else {
        return false;
    };

    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.trim() == service)
}

fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    let mut buffer = [0_u8; 8192];
    let bytes_read = stream.read(&mut buffer).unwrap();

    String::from_utf8_lossy(&buffer[..bytes_read]).to_string()
}

fn write_http_json(stream: &mut std::net::TcpStream, body: &Value) {
    let body = body.to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );

    stream.write_all(response.as_bytes()).unwrap();
    stream.flush().unwrap();
}
