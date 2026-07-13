#![allow(dead_code)]

mod common;

use chrono::{Duration, SecondsFormat, Utc};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde_json::{json, Value};

use common::CookieAuthRequest;

#[test]
fn my_work_is_assignee_scoped_access_scoped_filterable_and_paginated() {
    let client = Client::new();
    let admin = common::create_admin_auth(&client);
    let alice = common::create_test_auth(&client, "member");
    let bob = common::create_test_auth(&client, "member");
    let global_source = common::unique_name("my_work_global");
    let hidden_source = common::unique_name("my_work_hidden");

    create_connector(&client, &admin.cookie, &global_source, "global", None);
    create_connector(
        &client,
        &admin.cookie,
        &hidden_source,
        "user",
        Some(bob.user_id),
    );

    let today_start = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let overdue = today_start - Duration::hours(2);
    let due_soon = today_start + Duration::days(2);
    let source_updated = Utc::now() - Duration::minutes(10);
    let imported = import_work_cards(
        &client,
        &admin.cookie,
        &global_source,
        json!([
            work_item(
                "alice-blocked",
                "Alice blocked bug",
                "blocked",
                "urgent",
                alice.user_id,
                Some(overdue),
                Some(source_updated),
                "Portal",
                "Bug"
            ),
            work_item(
                "alice-next",
                "Alice upcoming story",
                "in_progress",
                "high",
                alice.user_id,
                Some(due_soon),
                Some(source_updated - Duration::hours(1)),
                "Portal",
                "User Story"
            ),
            work_item(
                "alice-done-overdue",
                "Alice completed old task",
                "done",
                "low",
                alice.user_id,
                Some(overdue - Duration::days(1)),
                Some(source_updated - Duration::hours(2)),
                "Operations",
                "Task"
            ),
            work_item(
                "bob-work",
                "Bob work",
                "todo",
                "medium",
                bob.user_id,
                None,
                Some(source_updated),
                "Portal",
                "Task"
            ),
            json!({
                "external_id": "unmapped-work",
                "title": "Unmapped work",
                "status": "todo",
                "priority": "low",
                "assignee": "Someone with no mapping",
                "project": "Portal",
                "work_item_type": "Task",
                "assignee_source_id": "aad.unmapped",
                "assignee_user_id": null,
                "due_at": null,
                "source_updated_at": source_updated.to_rfc3339_opts(SecondsFormat::Secs, true),
                "url": null
            })
        ]),
    );
    let hidden = import_work_cards(
        &client,
        &admin.cookie,
        &hidden_source,
        json!([work_item(
            "alice-hidden",
            "Alice assignment outside her record scope",
            "blocked",
            "urgent",
            alice.user_id,
            Some(overdue),
            Some(source_updated),
            "Secret",
            "Bug"
        )]),
    );

    let default_page = get_my_work(&client, &alice.cookie, "");
    assert_eq!(default_page["total"], 3);
    assert_eq!(default_page["page"], 1);
    assert_eq!(default_page["page_size"], 25);
    assert_eq!(default_page["items"][0]["external_id"], "alice-blocked");
    assert_ids_exclude(
        &default_page["items"],
        &["bob-work", "unmapped-work", "alice-hidden"],
    );
    assert_eq!(default_page["items"][0]["project"], "Portal");
    assert_eq!(default_page["items"][0]["work_item_type"], "Bug");
    assert_eq!(default_page["items"][0]["assignee_user_id"], alice.user_id);
    assert_eq!(
        default_page["items"][0]["source_updated_at"],
        source_updated.to_rfc3339_opts(SecondsFormat::Secs, true)
    );
    assert_eq!(
        default_page["facets"]["statuses"],
        json!(["blocked", "done", "in_progress"])
    );
    assert_eq!(
        default_page["facets"]["projects"],
        json!(["Operations", "Portal"])
    );
    assert_eq!(
        default_page["facets"]["work_item_types"],
        json!(["Bug", "Task", "User Story"])
    );
    assert_eq!(default_page["facets"]["sources"], json!([global_source]));

    let overdue_page = get_my_work(&client, &alice.cookie, "?due=overdue");
    assert_eq!(overdue_page["total"], 1, "done work must not be overdue");
    assert_eq!(overdue_page["items"][0]["external_id"], "alice-blocked");

    let filtered = get_my_work(
        &client,
        &alice.cookie,
        &format!(
            "?status=in_progress&project=Portal&work_item_type=User%20Story&due=next_7_days&source={global_source}&sort=source_updated_desc&page=1&page_size=1"
        ),
    );
    assert_eq!(filtered["total"], 1);
    assert_eq!(filtered["items"][0]["external_id"], "alice-next");
    assert_eq!(filtered["page_size"], 1);

    let second_page = get_my_work(&client, &alice.cookie, "?sort=due_asc&page=2&page_size=1");
    assert_eq!(second_page["total"], 3);
    assert_eq!(second_page["page"], 2);
    assert_eq!(second_page["items"].as_array().unwrap().len(), 1);

    for query in [
        "?due=upcoming",
        "?sort=updated_desc",
        "?page=0",
        "?page=1000001",
        "?page_size=101",
    ] {
        let response = client
            .get(format!("{}{query}", my_work_url()))
            .cookie_auth(&alice.cookie)
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "query {query}");
        assert_eq!(
            response.json::<Value>().unwrap()["error"]["code"],
            "validation_failed"
        );
    }

    let legacy = client
        .get(format!("{}/work-cards", common::APP_HOST))
        .cookie_auth(&admin.cookie)
        .send()
        .unwrap();
    assert_eq!(legacy.status(), StatusCode::OK);
    let legacy = legacy.json::<Value>().unwrap()["data"].clone();
    assert!(legacy
        .as_array()
        .unwrap()
        .iter()
        .any(|item| { item["external_id"] == "alice-blocked" && item["project"] == "Portal" }));

    for card in imported
        .as_array()
        .unwrap()
        .iter()
        .chain(hidden.as_array().unwrap())
    {
        delete_work_card(&client, &admin.cookie, card["id"].as_i64().unwrap());
    }
    delete_connector(&client, &admin.cookie, &hidden_source);
    delete_connector(&client, &admin.cookie, &global_source);
    common::delete_test_user(bob.user_id);
    common::delete_test_user(alice.user_id);
    common::delete_test_user(admin.user_id);
}

fn my_work_url() -> String {
    format!("{}/me/work-cards", common::APP_HOST)
}

fn get_my_work(client: &Client, cookie: &str, query: &str) -> Value {
    let response = client
        .get(format!("{}{query}", my_work_url()))
        .cookie_auth(cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().unwrap()["data"].clone()
}

#[allow(clippy::too_many_arguments)]
fn work_item(
    external_id: &str,
    title: &str,
    status: &str,
    priority: &str,
    assignee_user_id: i32,
    due_at: Option<chrono::DateTime<Utc>>,
    source_updated_at: Option<chrono::DateTime<Utc>>,
    project: &str,
    work_item_type: &str,
) -> Value {
    json!({
        "external_id": external_id,
        "title": title,
        "status": status,
        "priority": priority,
        "assignee": "Explicitly mapped portal user",
        "project": project,
        "work_item_type": work_item_type,
        "assignee_source_id": format!("aad.{assignee_user_id}"),
        "assignee_user_id": assignee_user_id,
        "due_at": due_at.map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true)),
        "source_updated_at": source_updated_at
            .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true)),
        "url": null
    })
}

fn create_connector(
    client: &Client,
    cookie: &str,
    source: &str,
    scope_type: &str,
    owner_user_id: Option<i32>,
) {
    let response = client
        .post(format!("{}/connectors", common::APP_HOST))
        .cookie_auth(cookie)
        .json(&json!({
            "source": source,
            "kind": "sample",
            "display_name": format!("My Work connector {source}"),
            "status": "active",
            "scope_type": scope_type,
            "owner_user_id": owner_user_id,
            "maintainer_id": null
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

fn import_work_cards(client: &Client, cookie: &str, source: &str, items: Value) -> Value {
    let response = client
        .post(format!(
            "{}/connectors/{source}/work-cards/import",
            common::APP_HOST
        ))
        .cookie_auth(cookie)
        .json(&json!({ "items": items }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.json::<Value>().unwrap();
    assert_eq!(body["data"]["run"]["status"], "success", "{body}");
    body["data"]["data"].clone()
}

fn assert_ids_exclude(items: &Value, excluded: &[&str]) {
    for external_id in excluded {
        assert!(
            !items
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["external_id"] == *external_id),
            "unexpected external id {external_id}"
        );
    }
}

fn delete_work_card(client: &Client, cookie: &str, id: i64) {
    let response = client
        .delete(format!("{}/work-cards/{id}", common::APP_HOST))
        .cookie_auth(cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

fn delete_connector(client: &Client, cookie: &str, source: &str) {
    let response = client
        .delete(format!("{}/connectors/{source}", common::APP_HOST))
        .cookie_auth(cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}
