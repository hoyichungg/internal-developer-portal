use chrono::{DateTime, NaiveDateTime};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Deserialize)]
struct AdapterConfig {
    adapter: Option<String>,
}

#[derive(Deserialize)]
struct AzureDevOpsConfig {
    adapter: Option<String>,
    wiql_url: Option<String>,
    work_items_url: Option<String>,
    base_url: Option<String>,
    organization: Option<String>,
    project: Option<String>,
    api_version: Option<String>,
    personal_access_token: Option<String>,
    pat: Option<String>,
    wiql: Option<String>,
    web_url_base: Option<String>,
    timeout_seconds: Option<u64>,
}

#[derive(Deserialize)]
struct MonitoringConfig {
    adapter: Option<String>,
    url: Option<String>,
    default_maintainer_id: Option<i32>,
    bearer_token: Option<String>,
    token: Option<String>,
    api_key: Option<String>,
    timeout_seconds: Option<u64>,
}

#[derive(Clone, Copy)]
enum SampleNotificationKind {
    Calendar,
    OutlookMail,
    ErpMessages,
}

pub async fn fetch_connector_payload(
    target: &str,
    config_json: &str,
) -> Result<Option<Value>, String> {
    let adapter = serde_json::from_str::<AdapterConfig>(config_json)
        .map_err(|error| format!("connector config is not valid JSON: {error}"))?
        .adapter;

    match adapter.as_deref() {
        Some("azure_devops") if target == "work_cards" => {
            fetch_azure_devops_work_cards(config_json).await.map(Some)
        }
        Some("azure_devops") => Err(format!(
            "azure_devops adapter does not support target {target}"
        )),
        Some("monitoring") if target == "service_health" => {
            fetch_monitoring_service_health(config_json).await.map(Some)
        }
        Some("monitoring") => Err(format!(
            "monitoring adapter does not support target {target}"
        )),
        Some("calendar_sample" | "calendar") if target == "notifications" => {
            fetch_sample_notifications(config_json, SampleNotificationKind::Calendar).map(Some)
        }
        Some("calendar_sample" | "calendar") => Err(format!(
            "calendar_sample adapter does not support target {target}"
        )),
        Some("outlook_mail_sample" | "outlook_mail" | "outlook") if target == "notifications" => {
            fetch_sample_notifications(config_json, SampleNotificationKind::OutlookMail).map(Some)
        }
        Some("outlook_mail_sample" | "outlook_mail" | "outlook") => Err(format!(
            "outlook_mail_sample adapter does not support target {target}"
        )),
        Some("erp_messages_sample" | "erp_messages" | "erp") if target == "notifications" => {
            fetch_sample_notifications(config_json, SampleNotificationKind::ErpMessages).map(Some)
        }
        Some("erp_messages_sample" | "erp_messages" | "erp") => Err(format!(
            "erp_messages_sample adapter does not support target {target}"
        )),
        Some(adapter) => Err(format!("connector adapter {adapter} is not supported")),
        None => Ok(None),
    }
}

async fn fetch_azure_devops_work_cards(config_json: &str) -> Result<Value, String> {
    let config = serde_json::from_str::<AzureDevOpsConfig>(config_json)
        .map_err(|error| format!("azure_devops config is not valid JSON: {error}"))?;

    if config.adapter.as_deref() != Some("azure_devops") {
        return Err("azure_devops config must set adapter to azure_devops".to_owned());
    }

    let wiql_url = config.wiql_url.clone().unwrap_or_else(|| {
        azure_devops_url(
            config
                .base_url
                .as_deref()
                .unwrap_or("https://dev.azure.com"),
            config.organization.as_deref().unwrap_or_default(),
            config.project.as_deref().unwrap_or_default(),
            "_apis/wit/wiql",
            config.api_version.as_deref().unwrap_or("7.1"),
        )
    });
    let work_items_url = config.work_items_url.clone().unwrap_or_else(|| {
        azure_devops_url(
            config
                .base_url
                .as_deref()
                .unwrap_or("https://dev.azure.com"),
            config.organization.as_deref().unwrap_or_default(),
            config.project.as_deref().unwrap_or_default(),
            "_apis/wit/workitemsbatch",
            config.api_version.as_deref().unwrap_or("7.1"),
        )
    });

    require_url("wiql_url", &wiql_url)?;
    require_url("work_items_url", &work_items_url)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(
            config.timeout_seconds.unwrap_or(15).max(1),
        ))
        .build()
        .map_err(|error| format!("azure_devops HTTP client could not be built: {error}"))?;
    let wiql_query = config.wiql.unwrap_or_else(default_wiql);
    let wiql_response = send_azure_request(
        client.post(&wiql_url).json(&json!({ "query": wiql_query })),
        config
            .personal_access_token
            .as_deref()
            .or(config.pat.as_deref()),
    )
    .await?;

    let ids = wiql_response
        .get("workItems")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("id").and_then(Value::as_i64))
        .collect::<Vec<_>>();

    if ids.is_empty() {
        return Ok(json!({ "items": [] }));
    }

    let work_items_response = send_azure_request(
        client.post(&work_items_url).json(&json!({
            "ids": ids,
            "fields": [
                "System.Id",
                "System.Title",
                "System.State",
                "System.AssignedTo",
                "Microsoft.VSTS.Common.Priority"
            ]
        })),
        config
            .personal_access_token
            .as_deref()
            .or(config.pat.as_deref()),
    )
    .await?;

    let items = work_items_response
        .get("value")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| normalize_azure_work_item(item, config.web_url_base.as_deref()))
        .collect::<Vec<_>>();

    Ok(json!({ "items": items }))
}

async fn fetch_monitoring_service_health(config_json: &str) -> Result<Value, String> {
    let config = serde_json::from_str::<MonitoringConfig>(config_json)
        .map_err(|error| format!("monitoring config is not valid JSON: {error}"))?;

    if config.adapter.as_deref() != Some("monitoring") {
        return Err("monitoring config must set adapter to monitoring".to_owned());
    }

    let url = config
        .url
        .as_deref()
        .ok_or_else(|| "monitoring config must set url".to_owned())?;
    require_url("url", url)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(
            config.timeout_seconds.unwrap_or(15).max(1),
        ))
        .build()
        .map_err(|error| format!("monitoring HTTP client could not be built: {error}"))?;
    let response = send_monitoring_request(
        client.get(url),
        config.bearer_token.as_deref().or(config.token.as_deref()),
        config.api_key.as_deref(),
    )
    .await?;
    let items = monitoring_items(&response)
        .into_iter()
        .map(|item| normalize_monitoring_service(item, config.default_maintainer_id))
        .collect::<Vec<_>>();

    Ok(json!({ "items": items }))
}

fn fetch_sample_notifications(
    config_json: &str,
    kind: SampleNotificationKind,
) -> Result<Value, String> {
    let config = serde_json::from_str::<Value>(config_json)
        .map_err(|error| format!("{} config is not valid JSON: {error}", kind.adapter_name()))?;
    let items = match sample_notification_items(&config, kind) {
        Some(items) => items
            .into_iter()
            .map(|item| normalize_sample_notification(kind, item))
            .collect(),
        None => default_sample_notifications(kind),
    };

    Ok(json!({ "items": items }))
}

async fn send_azure_request(
    request: reqwest::RequestBuilder,
    token: Option<&str>,
) -> Result<Value, String> {
    let request = match token {
        Some(token) if !token.trim().is_empty() => request.basic_auth("", Some(token)),
        _ => request,
    };
    let response = request
        .send()
        .await
        .map_err(|error| format!("azure_devops request failed: {error}"))?;
    let status = response.status();

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("azure_devops request returned {status}: {body}"));
    }

    response
        .json::<Value>()
        .await
        .map_err(|error| format!("azure_devops response was not valid JSON: {error}"))
}

async fn send_monitoring_request(
    request: reqwest::RequestBuilder,
    bearer_token: Option<&str>,
    api_key: Option<&str>,
) -> Result<Value, String> {
    let request = match bearer_token {
        Some(token) if !token.trim().is_empty() => request.bearer_auth(token),
        _ => request,
    };
    let request = match api_key {
        Some(api_key) if !api_key.trim().is_empty() => request.header("x-api-key", api_key),
        _ => request,
    };
    let response = request
        .send()
        .await
        .map_err(|error| format!("monitoring request failed: {error}"))?;
    let status = response.status();

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("monitoring request returned {status}: {body}"));
    }

    response
        .json::<Value>()
        .await
        .map_err(|error| format!("monitoring response was not valid JSON: {error}"))
}

impl SampleNotificationKind {
    fn adapter_name(self) -> &'static str {
        match self {
            SampleNotificationKind::Calendar => "calendar_sample",
            SampleNotificationKind::OutlookMail => "outlook_mail_sample",
            SampleNotificationKind::ErpMessages => "erp_messages_sample",
        }
    }

    fn item_keys(self) -> &'static [&'static str] {
        match self {
            SampleNotificationKind::Calendar => &["items", "events", "meetings"],
            SampleNotificationKind::OutlookMail => &["items", "messages", "mail"],
            SampleNotificationKind::ErpMessages => &["items", "messages", "private_messages"],
        }
    }
}

fn normalize_azure_work_item(item: &Value, web_url_base: Option<&str>) -> Value {
    let id = item.get("id").and_then(Value::as_i64).unwrap_or_default();
    let fields = item.get("fields").unwrap_or(&Value::Null);
    let title = fields
        .get("System.Title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled work item");
    let state = fields
        .get("System.State")
        .and_then(Value::as_str)
        .unwrap_or("New");
    let priority = fields
        .get("Microsoft.VSTS.Common.Priority")
        .and_then(Value::as_i64);
    let assignee = fields
        .get("System.AssignedTo")
        .and_then(assignee_display_name);
    let url = item
        .pointer("/_links/html/href")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| web_url_base.map(|base| format!("{}/{}", base.trim_end_matches('/'), id)));

    json!({
        "external_id": id.to_string(),
        "title": title,
        "status": normalize_azure_state(state),
        "priority": normalize_azure_priority(priority),
        "assignee": assignee,
        "due_at": null,
        "url": url
    })
}

fn sample_notification_items(config: &Value, kind: SampleNotificationKind) -> Option<Vec<&Value>> {
    for key in kind.item_keys() {
        if let Some(items) = config.get(*key).and_then(Value::as_array) {
            return Some(items.iter().collect());
        }
    }

    None
}

fn normalize_sample_notification(kind: SampleNotificationKind, item: &Value) -> Value {
    match kind {
        SampleNotificationKind::Calendar => normalize_calendar_notification(item),
        SampleNotificationKind::OutlookMail => normalize_outlook_mail_notification(item),
        SampleNotificationKind::ErpMessages => normalize_erp_message_notification(item),
    }
}

fn normalize_calendar_notification(item: &Value) -> Value {
    let title = field_string(item, &["title", "subject", "summary", "name"])
        .unwrap_or_else(|| "Calendar event".to_owned());
    let external_id = notification_external_id(
        "calendar",
        item,
        &["external_id", "id", "event_id", "uid", "ical_uid"],
        &title,
    );
    let severity = field_string(item, &["severity", "importance", "priority"])
        .map(|value| normalize_notification_severity(&value, "info"))
        .unwrap_or("info");

    json!({
        "external_id": external_id,
        "title": title,
        "body": calendar_body(item),
        "severity": severity,
        "is_read": field_bool(item, &["is_read", "read", "seen"]).unwrap_or(false),
        "url": field_url(item, &["url", "web_url", "web_link", "webLink", "join_url", "online_meeting_url"])
    })
}

fn normalize_outlook_mail_notification(item: &Value) -> Value {
    let title = field_string(item, &["title", "subject"])
        .unwrap_or_else(|| "Outlook mail message".to_owned());
    let external_id = notification_external_id(
        "mail",
        item,
        &["external_id", "id", "message_id", "internet_message_id"],
        &title,
    );
    let severity = field_string(item, &["severity", "importance", "priority"])
        .map(|value| normalize_notification_severity(&value, "info"))
        .unwrap_or("info");

    json!({
        "external_id": external_id,
        "title": title,
        "body": mail_body(item),
        "severity": severity,
        "is_read": field_bool(item, &["is_read", "isRead", "read", "seen"]).unwrap_or(false),
        "url": field_url(item, &["url", "web_url", "web_link", "webLink"])
    })
}

fn normalize_erp_message_notification(item: &Value) -> Value {
    let title = field_string(item, &["title", "subject", "type", "request_type"])
        .unwrap_or_else(|| "ERP message".to_owned());
    let external_id = notification_external_id(
        "erp",
        item,
        &[
            "external_id",
            "id",
            "message_id",
            "request_id",
            "approval_id",
        ],
        &title,
    );

    json!({
        "external_id": external_id,
        "title": title,
        "body": field_string(item, &["body", "message", "description", "summary", "preview"]),
        "severity": erp_message_severity(item),
        "is_read": field_bool(item, &["is_read", "read", "seen"]).unwrap_or(false),
        "url": field_url(item, &["url", "web_url", "web_link", "webLink"])
    })
}

fn default_sample_notifications(kind: SampleNotificationKind) -> Vec<Value> {
    match kind {
        SampleNotificationKind::Calendar => vec![json!({
            "external_id": "calendar-platform-standup",
            "title": "Calendar: Platform standup in 15 minutes",
            "body": "Organizer: Taylor Lin | Location: Teams",
            "severity": "info",
            "is_read": false,
            "url": "https://calendar.example.test/events/platform-standup"
        })],
        SampleNotificationKind::OutlookMail => vec![json!({
            "external_id": "mail-release-brief",
            "title": "Mail: Release brief ready for review",
            "body": "From: release-bot@example.test | API deploy window moved to 15:30.",
            "severity": "warning",
            "is_read": false,
            "url": "https://outlook.example.test/mail/release-brief"
        })],
        SampleNotificationKind::ErpMessages => vec![json!({
            "external_id": "erp-access-approval",
            "title": "ERP: Deployment access approval waiting",
            "body": "Sample ERP private message. Replace this adapter with the real ERP integration when one is available.",
            "severity": "warning",
            "is_read": false,
            "url": null
        })],
    }
}

fn calendar_body(item: &Value) -> Option<String> {
    let body = field_string(item, &["body", "description", "preview"]);
    if body.is_some() {
        return body;
    }

    let mut details = Vec::new();
    if let Some(organizer) = person_display(item, &["organizer", "organizer_name", "from"]) {
        details.push(format!("Organizer: {organizer}"));
    }
    if let Some(location) = field_string(item, &["location", "room"]) {
        details.push(format!("Location: {location}"));
    }
    if let Some(starts_at) =
        normalized_time_field(item, &["starts_at", "start_at", "start_time", "start"])
    {
        details.push(format!("Starts: {starts_at}"));
    }
    if let Some(ends_at) = normalized_time_field(item, &["ends_at", "end_at", "end_time", "end"]) {
        details.push(format!("Ends: {ends_at}"));
    }

    (!details.is_empty()).then(|| details.join(" | "))
}

fn mail_body(item: &Value) -> Option<String> {
    let preview = field_string(
        item,
        &["body", "body_preview", "bodyPreview", "preview", "summary"],
    );
    let sender = person_display(item, &["from", "sender"]);

    match (sender, preview) {
        (Some(sender), Some(preview)) => Some(format!("From: {sender} | {preview}")),
        (Some(sender), None) => Some(format!("From: {sender}")),
        (None, Some(preview)) => Some(preview),
        (None, None) => None,
    }
}

fn erp_message_severity(item: &Value) -> &'static str {
    if let Some(severity) = field_string(item, &["severity"]) {
        return normalize_notification_severity(&severity, "warning");
    }
    if let Some(priority) = field_string(item, &["priority", "importance"]) {
        return normalize_notification_severity(&priority, "warning");
    }
    if field_bool(
        item,
        &["requires_approval", "approval_required", "is_pending"],
    )
    .unwrap_or(false)
    {
        return "warning";
    }

    "info"
}

fn monitoring_items(response: &Value) -> Vec<&Value> {
    response
        .get("items")
        .or_else(|| response.get("services"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .into_iter()
        .chain(response.as_array().into_iter().flatten())
        .collect()
}

fn normalize_monitoring_service(item: &Value, default_maintainer_id: Option<i32>) -> Value {
    let external_id = field_string(item, &["external_id", "id", "slug", "name"])
        .unwrap_or_else(|| "unknown-service".to_owned());
    let name = field_string(item, &["name", "display_name", "service"])
        .unwrap_or_else(|| external_id.clone());
    let slug = stable_slug(
        field_string(item, &["slug"]).as_deref(),
        &[&external_id, &name],
    );
    let maintainer_id = field_i32(item, &["maintainer_id"]).or(default_maintainer_id);
    let health_status = field_string(item, &["health_status", "status", "health", "state"])
        .map(|status| normalize_monitoring_health(&status))
        .unwrap_or("unknown");
    let lifecycle_status = field_string(item, &["lifecycle_status", "lifecycle"])
        .map(|status| normalize_lifecycle(&status))
        .unwrap_or("active");

    json!({
        "external_id": external_id,
        "maintainer_id": maintainer_id.unwrap_or_default(),
        "slug": slug,
        "name": name,
        "lifecycle_status": lifecycle_status,
        "health_status": health_status,
        "description": field_string(item, &["description", "summary"]),
        "repository_url": field_string(item, &["repository_url", "repo_url", "repository"]),
        "dashboard_url": field_string(item, &["dashboard_url", "dashboard", "url"]),
        "runbook_url": field_string(item, &["runbook_url", "runbook"]),
        "last_checked_at": field_string(item, &["last_checked_at", "checked_at", "updated_at"])
            .and_then(|value| normalize_naive_datetime(&value))
    })
}

fn assignee_display_name(value: &Value) -> Option<String> {
    value
        .get("displayName")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| value.as_str().map(ToOwned::to_owned))
}

fn normalize_azure_state(state: &str) -> &'static str {
    match state.to_ascii_lowercase().as_str() {
        "done" | "closed" | "removed" => "done",
        "blocked" => "blocked",
        "active" | "committed" | "in progress" | "resolved" => "in_progress",
        _ => "todo",
    }
}

fn normalize_azure_priority(priority: Option<i64>) -> &'static str {
    match priority {
        Some(1) => "urgent",
        Some(2) => "high",
        Some(3) => "medium",
        _ => "low",
    }
}

fn normalize_monitoring_health(status: &str) -> &'static str {
    match status.to_ascii_lowercase().as_str() {
        "ok" | "up" | "green" | "healthy" | "passing" | "available" => "healthy",
        "warn" | "warning" | "yellow" | "degraded" | "unstable" => "degraded",
        "critical" | "down" | "red" | "error" | "failed" | "failing" | "offline" | "unhealthy" => {
            "down"
        }
        _ => "unknown",
    }
}

fn normalize_notification_severity(value: &str, default: &'static str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "critical" | "urgent" | "blocker" | "error" | "failed" | "failure" => "critical",
        "warning" | "warn" | "high" | "medium" | "normal" => "warning",
        "info" | "low" | "ok" | "success" | "none" => "info",
        _ => default,
    }
}

fn normalize_lifecycle(status: &str) -> &'static str {
    match status.to_ascii_lowercase().as_str() {
        "deprecated" => "deprecated",
        "archived" | "inactive" | "retired" | "decommissioned" => "archived",
        _ => "active",
    }
}

fn azure_devops_url(
    base_url: &str,
    organization: &str,
    project: &str,
    path: &str,
    api_version: &str,
) -> String {
    let base = base_url.trim_end_matches('/');
    let path = path.trim_start_matches('/');

    format!("{base}/{organization}/{project}/{path}?api-version={api_version}")
}

fn require_url(field: &str, url: &str) -> Result<(), String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(())
    } else {
        Err(format!("{field} must be an absolute HTTP URL"))
    }
}

fn notification_external_id(prefix: &str, item: &Value, id_fields: &[&str], title: &str) -> String {
    field_string(item, id_fields).unwrap_or_else(|| {
        let slug = stable_slug(None, &[title]);
        format!("{prefix}-{slug}")
    })
}

fn field_url(item: &Value, names: &[&str]) -> Option<String> {
    field_string(item, names)
        .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
}

fn field_string(item: &Value, names: &[&str]) -> Option<String> {
    field(item, names)
        .and_then(scalar_to_string)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn field_i32(item: &Value, names: &[&str]) -> Option<i32> {
    field(item, names)
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

fn field_bool(item: &Value, names: &[&str]) -> Option<bool> {
    field(item, names).and_then(|value| match value {
        Value::Bool(value) => Some(*value),
        Value::Number(value) => value.as_i64().map(|value| value != 0),
        Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    })
}

fn person_display(item: &Value, names: &[&str]) -> Option<String> {
    field(item, names).and_then(|value| {
        scalar_to_string(value)
            .or_else(|| {
                field_string(
                    value,
                    &["display_name", "displayName", "name", "email", "address"],
                )
            })
            .or_else(|| {
                value
                    .get("emailAddress")
                    .and_then(|email| field_string(email, &["name", "address"]))
            })
    })
}

fn normalized_time_field(item: &Value, names: &[&str]) -> Option<String> {
    field_string(item, names).map(|value| normalize_naive_datetime(&value).unwrap_or(value))
}

fn field<'a>(item: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| item.get(*name))
}

fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.to_owned()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }

    slug.trim_matches('-').to_owned()
}

fn stable_slug(preferred: Option<&str>, fallbacks: &[&str]) -> String {
    preferred
        .into_iter()
        .chain(fallbacks.iter().copied())
        .map(slugify)
        .find(|slug| !slug.is_empty())
        .unwrap_or_else(|| {
            let bytes = fallbacks
                .first()
                .copied()
                .unwrap_or("service")
                .bytes()
                .take(12)
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join("");

            format!("service-{bytes}")
        })
}

fn normalize_naive_datetime(value: &str) -> Option<String> {
    let value = value.trim();

    if value.is_empty() {
        return None;
    }

    if let Ok(datetime) = DateTime::parse_from_rfc3339(value) {
        return Some(datetime.naive_utc().format("%Y-%m-%dT%H:%M:%S").to_string());
    }

    for format in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(datetime) = NaiveDateTime::parse_from_str(value, format) {
            return Some(datetime.format("%Y-%m-%dT%H:%M:%S").to_string());
        }
    }

    None
}

fn default_wiql() -> String {
    "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project ORDER BY [System.ChangedDate] DESC"
        .to_owned()
}
