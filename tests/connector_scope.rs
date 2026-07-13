// Keep the test-binary name free of Windows installer keywords such as
// "update"; otherwise UAC installer detection can reject the unsigned test
// executable with OS error 740 before the test harness starts.
use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};

pub mod common;
use common::CookieAuthRequest;

#[test]
fn connector_scope_updates_move_existing_imported_records_atomically() {
    let client = Client::new();
    let admin = common::create_admin_auth(&client);
    let team_member = common::create_test_auth(&client, "member");
    let outsider = common::create_test_auth(&client, "member");
    let source = common::unique_name("scope_move");

    let maintainer = post_data(
        &client,
        &admin.cookie,
        "/maintainers",
        json!({
            "display_name": common::unique_name("Scope move team"),
            "email": format!("{}@example.test", common::unique_name("scope-move")),
        }),
    );
    let maintainer_id = maintainer["id"].as_i64().unwrap();
    post_data(
        &client,
        &admin.cookie,
        &format!("/maintainers/{maintainer_id}/members"),
        json!({ "user_id": team_member.user_id, "role": "viewer" }),
    );
    post_data(
        &client,
        &admin.cookie,
        "/connectors",
        json!({
            "source": source,
            "kind": "sample",
            "display_name": "Scope move connector",
            "status": "active",
            "scope_type": "global",
            "owner_user_id": null,
            "maintainer_id": null,
        }),
    );

    let work = post_data(
        &client,
        &admin.cookie,
        &format!("/connectors/{source}/work-cards/import"),
        json!({
            "items": [{
                "external_id": "work-1",
                "title": "Existing imported work",
                "status": "in_progress",
                "priority": "high",
                "assignee": null,
                "due_at": null,
                "url": null,
            }]
        }),
    )["data"][0]
        .clone();
    let notification = post_data(
        &client,
        &admin.cookie,
        &format!("/connectors/{source}/notifications/import"),
        json!({
            "items": [{
                "external_id": "message-1",
                "title": "Existing imported message",
                "body": null,
                "severity": "warning",
                "is_read": false,
                "url": null,
            }]
        }),
    )["data"][0]
        .clone();
    let starts_at = chrono::Utc::now() + chrono::Duration::hours(1);
    let ends_at = starts_at + chrono::Duration::minutes(30);
    let calendar_event = post_data(
        &client,
        &admin.cookie,
        &format!("/connectors/{source}/calendar-events/import"),
        json!({
            "items": [{
                "external_id": "event-1",
                "title": "Existing imported meeting",
                "body": null,
                "organizer": "Portal team",
                "location": "Teams",
                "starts_at": starts_at,
                "ends_at": ends_at,
                "time_zone": "UTC",
                "is_all_day": false,
                "is_cancelled": false,
                "web_url": null,
                "join_url": null,
            }]
        }),
    )["data"][0]
        .clone();

    let moved = put_data(
        &client,
        &admin.cookie,
        &format!("/connectors/{source}/scope"),
        json!({
            "scope_type": "maintainer",
            "owner_user_id": null,
            "maintainer_id": maintainer_id,
        }),
    );
    assert_eq!(moved["scope_type"], "maintainer");

    for (path, record) in [
        ("work-cards", &work),
        ("notifications", &notification),
        ("calendar-events", &calendar_event),
    ] {
        let record_id = record["id"].as_i64().unwrap();
        let updated = get_data(&client, &admin.cookie, &format!("/{path}/{record_id}"));
        assert_eq!(updated["maintainer_id"], maintainer_id);
        assert_eq!(updated["owner_user_id"], Value::Null);
        assert_detail_status(
            &client,
            &team_member.cookie,
            &format!("/{path}/{record_id}"),
            StatusCode::OK,
        );
        assert_detail_status(
            &client,
            &outsider.cookie,
            &format!("/{path}/{record_id}"),
            StatusCode::NOT_FOUND,
        );
    }
}

fn post_data(client: &Client, token: &str, path: &str, body: Value) -> Value {
    let response = client
        .post(format!("{}{}", common::APP_HOST, path))
        .cookie_auth(token)
        .json(&body)
        .send()
        .unwrap();
    assert!(
        matches!(response.status(), StatusCode::OK | StatusCode::CREATED),
        "unexpected POST {path} status: {}",
        response.status()
    );
    response.json::<Value>().unwrap()["data"].clone()
}

fn put_data(client: &Client, token: &str, path: &str, body: Value) -> Value {
    let response = client
        .put(format!("{}{}", common::APP_HOST, path))
        .cookie_auth(token)
        .json(&body)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
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

fn assert_detail_status(client: &Client, token: &str, path: &str, expected: StatusCode) {
    let response = client
        .get(format!("{}{}", common::APP_HOST, path))
        .cookie_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), expected, "unexpected GET {path} status");
}
