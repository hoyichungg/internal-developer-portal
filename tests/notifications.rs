use chrono::{Duration, Utc};
use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};

pub mod common;
use common::CookieAuthRequest;

#[test]
fn test_notification_receipts_are_isolated_per_user() {
    let client = Client::new();
    let admin = common::create_admin_auth(&client);
    let first_user = common::create_test_auth(&client, "member");
    let second_user = common::create_test_auth(&client, "member");
    let source = common::unique_name("receipt_source");

    let response = client
        .post(format!("{}/notifications", common::APP_HOST))
        .cookie_auth(&admin.cookie)
        .json(&json!({
            "source": source,
            "external_id": common::unique_name("receipt_notification"),
            "title": "Review a private notification receipt",
            "body": "Each user should control only their own lifecycle state.",
            "severity": "warning",
            "is_read": false,
            "url": "https://erp.acme.test/messages/receipt-isolation"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let notification: Value = response.json::<Value>().unwrap()["data"].clone();
    let notification_id = notification["id"].as_i64().unwrap();

    for token in [&first_user.cookie, &second_user.cookie] {
        let view = get_notification(&client, token, notification_id);
        assert_eq!(view["is_read"], false);
        assert_eq!(view["source_is_read"], false);
        assert_eq!(view["read_at"], Value::Null);
        assert_eq!(view["dismissed_at"], Value::Null);
        assert_eq!(view["snoozed_until"], Value::Null);
    }

    let response = post_action(&client, &first_user.cookie, notification_id, "read");
    assert_eq!(response["is_read"], true);
    assert_eq!(response["source_is_read"], false);
    assert!(response["read_at"].as_str().is_some());
    assert_actionable_state(&client, &first_user.cookie, &source, notification_id, false);
    assert_actionable_state(&client, &second_user.cookie, &source, notification_id, true);
    let second_user_view = get_notification(&client, &second_user.cookie, notification_id);
    assert_eq!(second_user_view["is_read"], false);
    assert_eq!(second_user_view["read_at"], Value::Null);

    let response = post_action(&client, &first_user.cookie, notification_id, "unread");
    assert_eq!(response["is_read"], false);
    assert_eq!(response["read_at"], Value::Null);
    assert_actionable_state(&client, &first_user.cookie, &source, notification_id, true);

    let snoozed_until = Utc::now() + Duration::hours(1);
    let response = client
        .post(format!(
            "{}/notifications/{notification_id}/snooze",
            common::APP_HOST
        ))
        .cookie_auth(&first_user.cookie)
        .json(&json!({ "snoozed_until": snoozed_until }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let snoozed: Value = response.json::<Value>().unwrap()["data"].clone();
    assert!(snoozed["snoozed_until"].as_str().is_some());
    assert_actionable_state(&client, &first_user.cookie, &source, notification_id, false);
    assert_actionable_state(&client, &second_user.cookie, &source, notification_id, true);

    let restored = post_action(&client, &first_user.cookie, notification_id, "restore");
    assert_eq!(restored["dismissed_at"], Value::Null);
    assert_eq!(restored["snoozed_until"], Value::Null);
    assert_actionable_state(&client, &first_user.cookie, &source, notification_id, true);

    let dismissed = post_action(&client, &first_user.cookie, notification_id, "dismiss");
    assert!(dismissed["dismissed_at"].as_str().is_some());
    assert_actionable_state(&client, &first_user.cookie, &source, notification_id, false);
    assert_actionable_state(&client, &second_user.cookie, &source, notification_id, true);
    assert_eq!(
        get_notification(&client, &second_user.cookie, notification_id)["dismissed_at"],
        Value::Null
    );

    let response = client
        .get(format!(
            "{}/audit-logs?action=dismiss&resource_type=notification&resource_id={notification_id}",
            common::APP_HOST
        ))
        .cookie_auth(&admin.cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let audit_logs: Value = response.json::<Value>().unwrap()["data"].clone();
    assert!(audit_logs.as_array().unwrap().iter().any(|entry| {
        entry["actor_user_id"].as_i64() == Some(first_user.user_id as i64)
            && entry["resource_id"].as_str() == Some(notification_id.to_string().as_str())
    }));

    common::delete_test_notification(&client, notification);
    common::delete_test_user(second_user.user_id);
    common::delete_test_user(first_user.user_id);
    common::delete_test_user(admin.user_id);
}

fn post_action(client: &Client, token: &str, notification_id: i64, action: &str) -> Value {
    let response = client
        .post(format!(
            "{}/notifications/{notification_id}/{action}",
            common::APP_HOST
        ))
        .cookie_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    response.json::<Value>().unwrap()["data"].clone()
}

fn get_notification(client: &Client, token: &str, notification_id: i64) -> Value {
    let response = client
        .get(format!(
            "{}/notifications/{notification_id}",
            common::APP_HOST
        ))
        .cookie_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    response.json::<Value>().unwrap()["data"].clone()
}

fn assert_actionable_state(
    client: &Client,
    token: &str,
    source: &str,
    notification_id: i64,
    expected: bool,
) {
    let response = client
        .get(format!("{}/notifications", common::APP_HOST))
        .cookie_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        contains_id(&response.json::<Value>().unwrap()["data"], notification_id),
        expected
    );

    let response = client
        .get(format!("{}/dashboard?source={source}", common::APP_HOST))
        .cookie_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let dashboard: Value = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(
        contains_id(&dashboard["notifications"], notification_id),
        expected
    );
    assert_eq!(
        dashboard["summary"]["unread_notifications"].as_i64(),
        Some(i64::from(expected))
    );

    let response = client
        .get(format!("{}/me/overview", common::APP_HOST))
        .cookie_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let overview: Value = response.json::<Value>().unwrap()["data"].clone();
    assert_eq!(
        contains_id(&overview["unread_notifications"], notification_id),
        expected
    );
}

fn contains_id(items: &Value, id: i64) -> bool {
    items
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"].as_i64() == Some(id))
}
