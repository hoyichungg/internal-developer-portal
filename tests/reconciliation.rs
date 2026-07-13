use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};

pub mod common;
use common::CookieAuthRequest;

#[test]
fn complete_snapshots_archive_missing_records_without_archiving_on_partial_or_bounded_runs() {
    let client = Client::new();
    let admin = common::create_admin_auth(&client);
    let work_source = common::unique_name("reconcile_work");
    let notification_source = common::unique_name("reconcile_notification");
    let calendar_source = common::unique_name("reconcile_calendar");

    let first_work = import_work_cards(
        &client,
        &admin.cookie,
        &work_source,
        Some(true),
        vec![
            work_item("work-a", "Work A", "high"),
            work_item("work-b", "Work B", "medium"),
        ],
    );
    assert_eq!(first_work["run"]["status"], "success");
    assert_eq!(first_work["run"]["snapshot_complete"], true);
    assert_eq!(first_work["run"]["archived_count"], 0);
    let work_b_id = first_work["data"][1]["id"].as_i64().unwrap();

    let partial_work = import_work_cards(
        &client,
        &admin.cookie,
        &work_source,
        Some(true),
        vec![
            work_item("work-a", "Work A updated", "high"),
            work_item("work-invalid", "Invalid work", "not-a-priority"),
        ],
    );
    assert_eq!(partial_work["run"]["status"], "partial_success");
    assert_eq!(partial_work["run"]["snapshot_complete"], true);
    assert_eq!(partial_work["run"]["archived_count"], 0);
    assert_list_contains(&client, &admin.cookie, "/work-cards", work_b_id, true);

    let bounded_work = import_work_cards(
        &client,
        &admin.cookie,
        &work_source,
        Some(false),
        vec![work_item("work-a", "Work A bounded", "high")],
    );
    assert_eq!(bounded_work["run"]["status"], "success");
    assert_eq!(bounded_work["run"]["snapshot_complete"], false);
    assert_eq!(bounded_work["run"]["archived_count"], 0);
    assert_list_contains(&client, &admin.cookie, "/work-cards", work_b_id, true);

    let final_work = import_work_cards(
        &client,
        &admin.cookie,
        &work_source,
        Some(true),
        vec![work_item("work-a", "Work A final", "high")],
    );
    assert_eq!(final_work["run"]["status"], "success");
    assert_eq!(final_work["run"]["archived_count"], 1);
    assert_list_contains(&client, &admin.cookie, "/work-cards", work_b_id, false);

    let archived_work = get_data(&client, &admin.cookie, &format!("/work-cards/{work_b_id}"));
    assert!(archived_work["archived_at"].is_string());

    let first_notifications = import_notifications(
        &client,
        &admin.cookie,
        &notification_source,
        Some(true),
        vec![
            notification("message-a", "Message A"),
            notification("message-b", "Message B"),
        ],
    );
    assert_eq!(first_notifications["run"]["status"], "success");
    let message_b_id = first_notifications["data"][1]["id"].as_i64().unwrap();

    let final_notifications = import_notifications(
        &client,
        &admin.cookie,
        &notification_source,
        Some(true),
        vec![notification("message-a", "Message A updated")],
    );
    assert_eq!(final_notifications["run"]["status"], "success");
    assert_eq!(final_notifications["run"]["archived_count"], 1);
    assert_list_contains(
        &client,
        &admin.cookie,
        "/notifications",
        message_b_id,
        false,
    );

    let starts_at = chrono::Utc::now() + chrono::Duration::hours(1);
    let ends_at = starts_at + chrono::Duration::minutes(30);
    let first_calendar = post_import(
        &client,
        &admin.cookie,
        &format!("/connectors/{calendar_source}/calendar-events/import"),
        Some(true),
        vec![
            calendar_event("event-a", "Event A", starts_at, ends_at),
            calendar_event("event-b", "Event B", starts_at, ends_at),
        ],
    );
    let event_a_id = first_calendar["data"][0]["id"].as_i64().unwrap();
    let event_b_id = first_calendar["data"][1]["id"].as_i64().unwrap();
    let final_calendar = post_import(
        &client,
        &admin.cookie,
        &format!("/connectors/{calendar_source}/calendar-events/import"),
        Some(true),
        vec![calendar_event(
            "event-a",
            "Event A updated",
            starts_at,
            ends_at,
        )],
    );
    assert_eq!(final_calendar["run"]["status"], "success");
    assert_eq!(final_calendar["run"]["archived_count"], 1);
    assert_detail_status(
        &client,
        &admin.cookie,
        &format!("/calendar-events/{event_a_id}"),
        StatusCode::OK,
    );
    assert_detail_status(
        &client,
        &admin.cookie,
        &format!("/calendar-events/{event_b_id}"),
        StatusCode::NOT_FOUND,
    );
}

fn import_work_cards(
    client: &Client,
    token: &str,
    source: &str,
    snapshot_complete: Option<bool>,
    items: Vec<Value>,
) -> Value {
    post_import(
        client,
        token,
        &format!("/connectors/{source}/work-cards/import"),
        snapshot_complete,
        items,
    )
}

fn import_notifications(
    client: &Client,
    token: &str,
    source: &str,
    snapshot_complete: Option<bool>,
    items: Vec<Value>,
) -> Value {
    post_import(
        client,
        token,
        &format!("/connectors/{source}/notifications/import"),
        snapshot_complete,
        items,
    )
}

fn post_import(
    client: &Client,
    token: &str,
    path: &str,
    snapshot_complete: Option<bool>,
    items: Vec<Value>,
) -> Value {
    let response = client
        .post(format!("{}{}", common::APP_HOST, path))
        .cookie_auth(token)
        .json(&json!({
            "items": items,
            "snapshot_complete": snapshot_complete,
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json::<Value>().unwrap()["data"].clone()
}

fn get_data(client: &Client, token: &str, path: &str) -> Value {
    let response = client
        .get(format!("{}{}", common::APP_HOST, path))
        .cookie_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().unwrap()["data"].clone()
}

fn assert_list_contains(client: &Client, token: &str, path: &str, id: i64, expected: bool) {
    let items = get_data(client, token, path);
    let contains = items
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"].as_i64() == Some(id));
    assert_eq!(
        contains, expected,
        "unexpected visibility for {path} id {id}"
    );
}

fn assert_detail_status(client: &Client, token: &str, path: &str, expected: StatusCode) {
    let response = client
        .get(format!("{}{}", common::APP_HOST, path))
        .cookie_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), expected, "unexpected GET {path} status");
}

fn work_item(external_id: &str, title: &str, priority: &str) -> Value {
    json!({
        "external_id": external_id,
        "title": title,
        "status": "in_progress",
        "priority": priority,
        "assignee": "platform-team",
        "due_at": null,
        "url": null,
    })
}

fn notification(external_id: &str, title: &str) -> Value {
    json!({
        "external_id": external_id,
        "title": title,
        "body": "Reconciliation integration test",
        "severity": "warning",
        "is_read": false,
        "url": null,
    })
}

fn calendar_event(
    external_id: &str,
    title: &str,
    starts_at: chrono::DateTime<chrono::Utc>,
    ends_at: chrono::DateTime<chrono::Utc>,
) -> Value {
    json!({
        "external_id": external_id,
        "title": title,
        "body": null,
        "organizer": "Portal team",
        "location": "Teams",
        "starts_at": starts_at,
        "ends_at": ends_at,
        "time_zone": "UTC",
        "is_all_day": false,
        "is_cancelled": false,
        "web_url": "https://calendar.example.test/event",
        "join_url": "https://teams.example.test/event",
    })
}
