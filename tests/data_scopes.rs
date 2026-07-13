use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};

pub mod common;
use common::CookieAuthRequest;

#[test]
fn test_connector_data_scopes_protect_records_and_operational_overviews() {
    let client = Client::new();
    let admin = common::create_admin_auth(&client);
    let personal_owner = common::create_test_auth(&client, "member");
    let team_member = common::create_test_auth(&client, "member");
    let outsider = common::create_test_auth(&client, "member");

    let maintainer = create_maintainer(&client, &admin.cookie);
    add_maintainer_member(
        &client,
        &admin.cookie,
        maintainer["id"].as_i64().unwrap(),
        team_member.user_id,
    );

    let global_source = common::unique_name("scope_global");
    let personal_source = common::unique_name("scope_personal");
    let team_source = common::unique_name("scope_team");

    let global_connector =
        create_connector(&client, &admin.cookie, &global_source, "global", None, None);
    let personal_connector = create_connector(
        &client,
        &admin.cookie,
        &personal_source,
        "user",
        Some(personal_owner.user_id),
        None,
    );
    let team_connector = create_connector(
        &client,
        &admin.cookie,
        &team_source,
        "maintainer",
        None,
        Some(maintainer["id"].as_i64().unwrap()),
    );

    assert_eq!(global_connector["scope_type"], "global");
    assert_eq!(global_connector["owner_user_id"], Value::Null);
    assert_eq!(global_connector["maintainer_id"], Value::Null);
    assert_eq!(personal_connector["scope_type"], "user");
    assert_eq!(personal_connector["owner_user_id"], personal_owner.user_id);
    assert_eq!(personal_connector["maintainer_id"], Value::Null);
    assert_eq!(team_connector["scope_type"], "maintainer");
    assert_eq!(team_connector["owner_user_id"], Value::Null);
    assert_eq!(team_connector["maintainer_id"], maintainer["id"]);

    assert_connector_visibility(
        &client,
        &admin.cookie,
        [&global_source, &personal_source, &team_source],
        [],
    );
    assert_connector_visibility(
        &client,
        &personal_owner.cookie,
        [&global_source, &personal_source],
        [&team_source],
    );
    assert_connector_visibility(
        &client,
        &team_member.cookie,
        [&global_source, &team_source],
        [&personal_source],
    );
    assert_connector_visibility(
        &client,
        &outsider.cookie,
        [&global_source],
        [&personal_source, &team_source],
    );

    for token in [
        admin.cookie.as_str(),
        personal_owner.cookie.as_str(),
        team_member.cookie.as_str(),
        outsider.cookie.as_str(),
    ] {
        assert_detail_status(
            &client,
            token,
            &format!("/connectors/{global_source}"),
            StatusCode::OK,
        );
    }

    assert_detail_status(
        &client,
        &personal_owner.cookie,
        &format!("/connectors/{personal_source}"),
        StatusCode::OK,
    );
    assert_detail_status(
        &client,
        &personal_owner.cookie,
        &format!("/connectors/{team_source}"),
        StatusCode::NOT_FOUND,
    );
    assert_detail_status(
        &client,
        &team_member.cookie,
        &format!("/connectors/{team_source}"),
        StatusCode::OK,
    );
    assert_detail_status(
        &client,
        &team_member.cookie,
        &format!("/connectors/{personal_source}"),
        StatusCode::NOT_FOUND,
    );
    for source in [&personal_source, &team_source] {
        assert_detail_status(
            &client,
            &outsider.cookie,
            &format!("/connectors/{source}"),
            StatusCode::NOT_FOUND,
        );
        assert_detail_status(
            &client,
            &admin.cookie,
            &format!("/connectors/{source}"),
            StatusCode::OK,
        );
    }

    let global_work = import_work_card(&client, &admin.cookie, &global_source);
    let personal_work = import_work_card(&client, &admin.cookie, &personal_source);
    let team_work = import_work_card(&client, &admin.cookie, &team_source);
    let global_notification = import_notification(&client, &admin.cookie, &global_source);
    let personal_notification = import_notification(&client, &admin.cookie, &personal_source);
    let team_notification = import_notification(&client, &admin.cookie, &team_source);

    assert_record_scope(&global_work, &global_connector, None, None);
    assert_record_scope(
        &personal_work,
        &personal_connector,
        Some(personal_owner.user_id as i64),
        None,
    );
    assert_record_scope(
        &team_work,
        &team_connector,
        None,
        Some(maintainer["id"].as_i64().unwrap()),
    );
    assert_record_scope(&global_notification, &global_connector, None, None);
    assert_record_scope(
        &personal_notification,
        &personal_connector,
        Some(personal_owner.user_id as i64),
        None,
    );
    assert_record_scope(
        &team_notification,
        &team_connector,
        None,
        Some(maintainer["id"].as_i64().unwrap()),
    );

    assert_record_list_visibility(
        &client,
        &personal_owner.cookie,
        "/work-cards",
        [id(&global_work), id(&personal_work)],
        [id(&team_work)],
    );
    assert_record_list_visibility(
        &client,
        &team_member.cookie,
        "/work-cards",
        [id(&global_work), id(&team_work)],
        [id(&personal_work)],
    );
    assert_record_list_visibility(
        &client,
        &outsider.cookie,
        "/work-cards",
        [id(&global_work)],
        [id(&personal_work), id(&team_work)],
    );
    assert_record_list_visibility(
        &client,
        &admin.cookie,
        "/work-cards",
        [id(&global_work), id(&personal_work), id(&team_work)],
        [],
    );

    assert_record_list_visibility(
        &client,
        &personal_owner.cookie,
        "/notifications",
        [id(&global_notification), id(&personal_notification)],
        [id(&team_notification)],
    );
    assert_record_list_visibility(
        &client,
        &team_member.cookie,
        "/notifications",
        [id(&global_notification), id(&team_notification)],
        [id(&personal_notification)],
    );
    assert_record_list_visibility(
        &client,
        &outsider.cookie,
        "/notifications",
        [id(&global_notification)],
        [id(&personal_notification), id(&team_notification)],
    );
    assert_record_list_visibility(
        &client,
        &admin.cookie,
        "/notifications",
        [
            id(&global_notification),
            id(&personal_notification),
            id(&team_notification),
        ],
        [],
    );

    for (token, allowed_work, hidden_work, allowed_notification, hidden_notification) in [
        (
            personal_owner.cookie.as_str(),
            id(&personal_work),
            id(&team_work),
            id(&personal_notification),
            id(&team_notification),
        ),
        (
            team_member.cookie.as_str(),
            id(&team_work),
            id(&personal_work),
            id(&team_notification),
            id(&personal_notification),
        ),
    ] {
        assert_detail_status(
            &client,
            token,
            &format!("/work-cards/{allowed_work}"),
            StatusCode::OK,
        );
        assert_detail_status(
            &client,
            token,
            &format!("/work-cards/{hidden_work}"),
            StatusCode::NOT_FOUND,
        );
        assert_detail_status(
            &client,
            token,
            &format!("/notifications/{allowed_notification}"),
            StatusCode::OK,
        );
        assert_detail_status(
            &client,
            token,
            &format!("/notifications/{hidden_notification}"),
            StatusCode::NOT_FOUND,
        );
    }
    for path in [
        format!("/work-cards/{}", id(&personal_work)),
        format!("/work-cards/{}", id(&team_work)),
        format!("/notifications/{}", id(&personal_notification)),
        format!("/notifications/{}", id(&team_notification)),
    ] {
        assert_detail_status(&client, &outsider.cookie, &path, StatusCode::NOT_FOUND);
        assert_detail_status(&client, &admin.cookie, &path, StatusCode::OK);
    }

    let global_failed_run = create_failed_run(&client, &admin.cookie, &global_source);
    let personal_failed_run = create_failed_run(&client, &admin.cookie, &personal_source);
    let team_failed_run = create_failed_run(&client, &admin.cookie, &team_source);

    // The same user belongs to two maintainers so an explicitly selected team
    // scope can be verified independently from the user's broader access.
    let second_maintainer = create_maintainer(&client, &admin.cookie);
    add_maintainer_member(
        &client,
        &admin.cookie,
        second_maintainer["id"].as_i64().unwrap(),
        team_member.user_id,
    );
    let second_team_source = common::unique_name("scope_team_two");
    let second_team_connector = create_connector(
        &client,
        &admin.cookie,
        &second_team_source,
        "maintainer",
        None,
        Some(second_maintainer["id"].as_i64().unwrap()),
    );
    let second_team_work = import_work_card(&client, &admin.cookie, &second_team_source);
    let second_team_notification = import_notification(&client, &admin.cookie, &second_team_source);
    let second_team_failed_run = create_failed_run(&client, &admin.cookie, &second_team_source);
    assert_record_scope(
        &second_team_work,
        &second_team_connector,
        None,
        Some(second_maintainer["id"].as_i64().unwrap()),
    );
    assert_record_scope(
        &second_team_notification,
        &second_team_connector,
        None,
        Some(second_maintainer["id"].as_i64().unwrap()),
    );
    assert_detail_status(
        &client,
        &team_member.cookie,
        &format!("/connectors/{second_team_source}"),
        StatusCode::OK,
    );
    assert_detail_status(
        &client,
        &outsider.cookie,
        &format!("/connectors/{second_team_source}"),
        StatusCode::NOT_FOUND,
    );

    assert_dashboard_source_counts(
        &client,
        &personal_owner.cookie,
        &personal_source,
        1,
        1,
        [id(&personal_work)],
        [id(&personal_notification)],
    );
    assert_dashboard_source_counts(
        &client,
        &team_member.cookie,
        &team_source,
        1,
        1,
        [id(&team_work)],
        [id(&team_notification)],
    );
    assert_dashboard_source_counts(&client, &outsider.cookie, &personal_source, 0, 0, [], []);
    assert_dashboard_source_counts(&client, &outsider.cookie, &team_source, 0, 0, [], []);
    assert_dashboard_source_counts(
        &client,
        &admin.cookie,
        &personal_source,
        1,
        1,
        [id(&personal_work)],
        [id(&personal_notification)],
    );
    assert_dashboard_source_counts(
        &client,
        &admin.cookie,
        &team_source,
        1,
        1,
        [id(&team_work)],
        [id(&team_notification)],
    );
    assert_dashboard_run_scope(
        &client,
        &personal_owner.cookie,
        &format!("source={personal_source}"),
        [id(&personal_failed_run)],
        [id(&global_failed_run), id(&team_failed_run)],
    );
    assert_dashboard_run_scope(
        &client,
        &team_member.cookie,
        &format!("source={team_source}"),
        [id(&team_failed_run)],
        [id(&global_failed_run), id(&second_team_failed_run)],
    );
    assert_dashboard_run_scope(
        &client,
        &admin.cookie,
        &format!("source={personal_source}"),
        [id(&personal_failed_run)],
        [
            id(&global_failed_run),
            id(&team_failed_run),
            id(&second_team_failed_run),
        ],
    );

    assert_dashboard_maintainer_scope(
        &client,
        &team_member.cookie,
        maintainer["id"].as_i64().unwrap(),
        id(&team_work),
        id(&team_notification),
        id(&team_failed_run),
        [id(&global_work), id(&second_team_work)],
        [id(&global_notification), id(&second_team_notification)],
        [id(&global_failed_run), id(&second_team_failed_run)],
    );
    assert_dashboard_maintainer_scope(
        &client,
        &team_member.cookie,
        second_maintainer["id"].as_i64().unwrap(),
        id(&second_team_work),
        id(&second_team_notification),
        id(&second_team_failed_run),
        [id(&global_work), id(&team_work)],
        [id(&global_notification), id(&team_notification)],
        [id(&global_failed_run), id(&team_failed_run)],
    );
    assert_dashboard_maintainer_scope(
        &client,
        &admin.cookie,
        maintainer["id"].as_i64().unwrap(),
        id(&team_work),
        id(&team_notification),
        id(&team_failed_run),
        [id(&global_work), id(&personal_work), id(&second_team_work)],
        [
            id(&global_notification),
            id(&personal_notification),
            id(&second_team_notification),
        ],
        [
            id(&global_failed_run),
            id(&personal_failed_run),
            id(&second_team_failed_run),
        ],
    );
    for token in [&personal_owner.cookie, &outsider.cookie] {
        let response = client
            .get(format!(
                "{}/dashboard?maintainer_id={}",
                common::APP_HOST,
                maintainer["id"].as_i64().unwrap()
            ))
            .cookie_auth(token)
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    assert_overview_visibility(
        &client,
        &personal_owner.cookie,
        [id(&global_work), id(&personal_work)],
        [id(&team_work), id(&second_team_work)],
        [id(&global_notification), id(&personal_notification)],
        [id(&team_notification), id(&second_team_notification)],
        [id(&global_failed_run), id(&personal_failed_run)],
        [id(&team_failed_run), id(&second_team_failed_run)],
    );
    assert_overview_visibility(
        &client,
        &team_member.cookie,
        [id(&global_work), id(&team_work), id(&second_team_work)],
        [id(&personal_work)],
        [
            id(&global_notification),
            id(&team_notification),
            id(&second_team_notification),
        ],
        [id(&personal_notification)],
        [
            id(&global_failed_run),
            id(&team_failed_run),
            id(&second_team_failed_run),
        ],
        [id(&personal_failed_run)],
    );
    assert_overview_visibility(
        &client,
        &outsider.cookie,
        [id(&global_work)],
        [id(&personal_work), id(&team_work), id(&second_team_work)],
        [id(&global_notification)],
        [
            id(&personal_notification),
            id(&team_notification),
            id(&second_team_notification),
        ],
        [id(&global_failed_run)],
        [
            id(&personal_failed_run),
            id(&team_failed_run),
            id(&second_team_failed_run),
        ],
    );
    assert_overview_visibility(
        &client,
        &admin.cookie,
        [
            id(&global_work),
            id(&personal_work),
            id(&team_work),
            id(&second_team_work),
        ],
        [],
        [
            id(&global_notification),
            id(&personal_notification),
            id(&team_notification),
            id(&second_team_notification),
        ],
        [],
        [
            id(&global_failed_run),
            id(&personal_failed_run),
            id(&team_failed_run),
            id(&second_team_failed_run),
        ],
        [],
    );

    for record in [
        &global_notification,
        &personal_notification,
        &team_notification,
        &second_team_notification,
    ] {
        delete_record(&client, &admin.cookie, "/notifications", id(record));
    }
    for record in [&global_work, &personal_work, &team_work, &second_team_work] {
        delete_record(&client, &admin.cookie, "/work-cards", id(record));
    }
    for source in [
        &global_source,
        &personal_source,
        &team_source,
        &second_team_source,
    ] {
        let response = client
            .delete(format!("{}/connectors/{source}", common::APP_HOST))
            .cookie_auth(&admin.cookie)
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
    let response = client
        .delete(format!(
            "{}/maintainers/{}",
            common::APP_HOST,
            maintainer["id"].as_i64().unwrap()
        ))
        .cookie_auth(&admin.cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = client
        .delete(format!(
            "{}/maintainers/{}",
            common::APP_HOST,
            second_maintainer["id"].as_i64().unwrap()
        ))
        .cookie_auth(&admin.cookie)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    common::delete_test_user(outsider.user_id);
    common::delete_test_user(team_member.user_id);
    common::delete_test_user(personal_owner.user_id);
    common::delete_test_user(admin.user_id);
}

fn create_maintainer(client: &Client, token: &str) -> Value {
    let unique = common::unique_name("scope_team");
    let response = client
        .post(format!("{}/maintainers", common::APP_HOST))
        .cookie_auth(token)
        .json(&json!({
            "display_name": format!("Scope team {unique}"),
            "email": format!("{unique}@example.test")
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json::<Value>().unwrap()["data"].clone()
}

fn add_maintainer_member(client: &Client, token: &str, maintainer_id: i64, user_id: i32) {
    let response = client
        .post(format!(
            "{}/maintainers/{maintainer_id}/members",
            common::APP_HOST
        ))
        .cookie_auth(token)
        .json(&json!({
            "user_id": user_id,
            "role": "viewer"
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

fn create_connector(
    client: &Client,
    token: &str,
    source: &str,
    scope_type: &str,
    owner_user_id: Option<i32>,
    maintainer_id: Option<i64>,
) -> Value {
    let response = client
        .post(format!("{}/connectors", common::APP_HOST))
        .cookie_auth(token)
        .json(&json!({
            "source": source,
            "kind": "sample",
            "display_name": format!("Scoped connector {source}"),
            "status": "active",
            "scope_type": scope_type,
            "owner_user_id": owner_user_id,
            "maintainer_id": maintainer_id
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json::<Value>().unwrap()["data"].clone()
}

fn import_work_card(client: &Client, token: &str, source: &str) -> Value {
    let response = client
        .post(format!(
            "{}/connectors/{source}/work-cards/import",
            common::APP_HOST
        ))
        .cookie_auth(token)
        .json(&json!({
            "items": [{
                "external_id": common::unique_name("scoped_work"),
                "title": format!("Scoped work from {source}"),
                "status": "in_progress",
                "priority": "high",
                "assignee": "platform-team",
                "due_at": null,
                "url": null
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.json::<Value>().unwrap();
    assert_eq!(body["data"]["run"]["status"], "success");
    body["data"]["data"][0].clone()
}

fn import_notification(client: &Client, token: &str, source: &str) -> Value {
    let response = client
        .post(format!(
            "{}/connectors/{source}/notifications/import",
            common::APP_HOST
        ))
        .cookie_auth(token)
        .json(&json!({
            "items": [{
                "external_id": common::unique_name("scoped_notification"),
                "title": format!("Scoped notification from {source}"),
                "body": "Scope isolation integration test",
                "severity": "warning",
                "is_read": false,
                "url": null
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.json::<Value>().unwrap();
    assert_eq!(body["data"]["run"]["status"], "success");
    body["data"]["data"][0].clone()
}

fn create_failed_run(client: &Client, token: &str, source: &str) -> Value {
    let response = client
        .post(format!(
            "{}/connectors/{source}/work-cards/import",
            common::APP_HOST
        ))
        .cookie_auth(token)
        .json(&json!({
            "items": [{
                "external_id": common::unique_name("invalid_scoped_work"),
                "title": "Invalid scoped work item",
                "status": "todo",
                "priority": "invalid-priority",
                "assignee": null,
                "due_at": null,
                "url": null
            }]
        }))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let run = response.json::<Value>().unwrap()["data"]["run"].clone();
    assert_eq!(run["status"], "failed");
    run
}

fn assert_connector_visibility<const V: usize, const H: usize>(
    client: &Client,
    token: &str,
    visible: [&str; V],
    hidden: [&str; H],
) {
    let connectors = get_data(client, token, "/connectors");
    for source in visible {
        assert_contains_source(&connectors, source);
    }
    for source in hidden {
        assert_not_contains_source(&connectors, source);
    }
}

fn assert_record_list_visibility<const V: usize, const H: usize>(
    client: &Client,
    token: &str,
    path: &str,
    visible: [i64; V],
    hidden: [i64; H],
) {
    let records = get_data(client, token, path);
    assert_ids(&records, visible, hidden);
}

#[allow(clippy::too_many_arguments)]
fn assert_dashboard_source_counts<const W: usize, const N: usize>(
    client: &Client,
    token: &str,
    source: &str,
    expected_work_cards: i64,
    expected_notifications: i64,
    work_card_ids: [i64; W],
    notification_ids: [i64; N],
) {
    let dashboard = get_data(client, token, &format!("/dashboard?source={source}"));
    assert_eq!(
        dashboard["summary"]["open_work_cards"].as_i64(),
        Some(expected_work_cards),
        "unexpected scoped work-card count for source {source}: {dashboard:?}"
    );
    assert_eq!(
        dashboard["summary"]["unread_notifications"].as_i64(),
        Some(expected_notifications),
        "unexpected scoped notification count for source {source}: {dashboard:?}"
    );
    assert_ids(&dashboard["work_cards"], work_card_ids, []);
    assert_ids(&dashboard["notifications"], notification_ids, []);
}

fn assert_dashboard_run_scope<const V: usize, const H: usize>(
    client: &Client,
    token: &str,
    query: &str,
    visible: [i64; V],
    hidden: [i64; H],
) {
    let dashboard = get_data(client, token, &format!("/dashboard?{query}"));
    assert_priority_run_ids(&dashboard, visible, hidden);
}

#[allow(clippy::too_many_arguments)]
fn assert_dashboard_maintainer_scope<const WH: usize, const NH: usize, const RH: usize>(
    client: &Client,
    token: &str,
    maintainer_id: i64,
    visible_work: i64,
    visible_notification: i64,
    visible_run: i64,
    hidden_work: [i64; WH],
    hidden_notifications: [i64; NH],
    hidden_runs: [i64; RH],
) {
    let dashboard = get_data(
        client,
        token,
        &format!("/dashboard?maintainer_id={maintainer_id}"),
    );
    assert_eq!(
        dashboard["scope"]["maintainer_id"].as_i64(),
        Some(maintainer_id)
    );
    assert_eq!(dashboard["summary"]["open_work_cards"].as_i64(), Some(1));
    assert_eq!(
        dashboard["summary"]["unread_notifications"].as_i64(),
        Some(1)
    );
    assert_ids(&dashboard["work_cards"], [visible_work], hidden_work);
    assert_ids(
        &dashboard["notifications"],
        [visible_notification],
        hidden_notifications,
    );
    assert_priority_run_ids(&dashboard, [visible_run], hidden_runs);
}

fn assert_priority_run_ids<const V: usize, const H: usize>(
    dashboard: &Value,
    visible: [i64; V],
    hidden: [i64; H],
) {
    let run_ids = dashboard["priority_items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["kind"].as_str() == Some("connector_run"))
        .filter_map(|item| item["record_id"].as_i64())
        .collect::<Vec<_>>();
    for id in visible {
        assert!(
            run_ids.contains(&id),
            "expected connector run id {id} in scoped dashboard, got {run_ids:?}"
        );
    }
    for id in hidden {
        assert!(
            !run_ids.contains(&id),
            "connector run id {id} leaked into scoped dashboard: {run_ids:?}"
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn assert_overview_visibility<
    const WV: usize,
    const WH: usize,
    const NV: usize,
    const NH: usize,
    const RV: usize,
    const RH: usize,
>(
    client: &Client,
    token: &str,
    visible_work: [i64; WV],
    hidden_work: [i64; WH],
    visible_notifications: [i64; NV],
    hidden_notifications: [i64; NH],
    visible_runs: [i64; RV],
    hidden_runs: [i64; RH],
) {
    let overview = get_data(client, token, "/me/overview");
    assert_ids(&overview["open_work_cards"], visible_work, hidden_work);
    assert_ids(
        &overview["unread_notifications"],
        visible_notifications,
        hidden_notifications,
    );
    assert_ids(
        &overview["failed_connector_runs"],
        visible_runs,
        hidden_runs,
    );
}

fn assert_record_scope(
    record: &Value,
    connector: &Value,
    owner_user_id: Option<i64>,
    maintainer_id: Option<i64>,
) {
    assert_eq!(record["connector_id"], connector["id"]);
    assert_eq!(record["owner_user_id"].as_i64(), owner_user_id);
    assert_eq!(record["maintainer_id"].as_i64(), maintainer_id);
    assert!(record["last_seen_run_id"].as_i64().is_some());
}

fn assert_detail_status(client: &Client, token: &str, path: &str, expected: StatusCode) {
    let response = client
        .get(format!("{}{path}", common::APP_HOST))
        .cookie_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), expected, "unexpected status for {path}");
}

fn get_data(client: &Client, token: &str, path: &str) -> Value {
    let response = client
        .get(format!("{}{path}", common::APP_HOST))
        .cookie_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "GET {path} failed");
    response.json::<Value>().unwrap()["data"].clone()
}

fn delete_record(client: &Client, token: &str, collection: &str, id: i64) {
    let response = client
        .delete(format!("{}{collection}/{id}", common::APP_HOST))
        .cookie_auth(token)
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

fn id(value: &Value) -> i64 {
    value["id"].as_i64().unwrap()
}

fn assert_ids<const V: usize, const H: usize>(
    records: &Value,
    visible: [i64; V],
    hidden: [i64; H],
) {
    let records = records.as_array().unwrap();
    for id in visible {
        assert!(
            records
                .iter()
                .any(|record| record["id"].as_i64() == Some(id)),
            "expected record id {id}, got {records:?}"
        );
    }
    for id in hidden {
        assert!(
            !records
                .iter()
                .any(|record| record["id"].as_i64() == Some(id)),
            "record id {id} leaked into {records:?}"
        );
    }
}

fn assert_contains_source(connectors: &Value, source: &str) {
    assert!(
        connectors
            .as_array()
            .unwrap()
            .iter()
            .any(|connector| connector["source"].as_str() == Some(source)),
        "expected connector source {source}, got {connectors:?}"
    );
}

fn assert_not_contains_source(connectors: &Value, source: &str) {
    assert!(
        !connectors
            .as_array()
            .unwrap()
            .iter()
            .any(|connector| connector["source"].as_str() == Some(source)),
        "connector source {source} leaked into {connectors:?}"
    );
}
