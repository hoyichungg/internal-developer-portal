use chrono::{Duration as ChronoDuration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

use super::shared::{
    append_query_params, field_bool, field_string, field_url, format_graph_datetime,
    normalize_notification_severity, normalized_time_field, notification_external_id,
    person_display, require_url,
};

#[derive(Deserialize)]
struct ErpPrivateMessagesConfig {
    adapter: Option<String>,
    messages_url: Option<String>,
    private_messages_url: Option<String>,
    url: Option<String>,
    bearer_token: Option<String>,
    token: Option<String>,
    api_key: Option<String>,
    #[serde(rename = "x-api-key")]
    x_api_key: Option<String>,
    api_key_header: Option<String>,
    since: Option<String>,
    updated_after: Option<String>,
    received_after: Option<String>,
    lookback_hours: Option<i64>,
    unread_only: Option<bool>,
    top: Option<u64>,
    limit: Option<u64>,
    timeout_seconds: Option<u64>,
    snapshot_complete: Option<bool>,
}

pub(super) async fn fetch_erp_private_messages(config_json: &str) -> Result<Value, String> {
    let config = serde_json::from_str::<ErpPrivateMessagesConfig>(config_json)
        .map_err(|error| format!("erp_private_messages config is not valid JSON: {error}"))?;

    if !matches!(
        config.adapter.as_deref(),
        Some("erp_private_messages" | "erp_messages_http" | "erp_http")
    ) {
        return Err(
            "erp_private_messages config must set adapter to erp_private_messages".to_owned(),
        );
    }

    let request_url = erp_private_messages_url(&config)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(
            config.timeout_seconds.unwrap_or(15).max(1),
        ))
        .build()
        .map_err(|error| format!("erp_private_messages HTTP client could not be built: {error}"))?;
    let response = send_erp_private_messages_request(
        client.get(&request_url),
        config.bearer_token.as_deref().or(config.token.as_deref()),
        config.api_key.as_deref().or(config.x_api_key.as_deref()),
        config.api_key_header.as_deref(),
    )
    .await?;
    let items = erp_message_items(&response)
        .into_iter()
        .map(normalize_erp_message_notification)
        .collect::<Vec<_>>();

    Ok(json!({
        "items": items,
        // ERP endpoints vary between full lists and incremental windows. Reconciliation is
        // opt-in so a bounded/lookback response cannot accidentally archive older messages.
        "snapshot_complete": config.snapshot_complete.unwrap_or(false)
    }))
}

async fn send_erp_private_messages_request(
    request: reqwest::RequestBuilder,
    bearer_token: Option<&str>,
    api_key: Option<&str>,
    api_key_header: Option<&str>,
) -> Result<Value, String> {
    let request = match bearer_token {
        Some(token) if !token.trim().is_empty() => request.bearer_auth(token),
        _ => request,
    };
    let request = match api_key {
        Some(api_key) if !api_key.trim().is_empty() => {
            let header_name = api_key_header
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("x-api-key");
            let header_name = reqwest::header::HeaderName::from_bytes(header_name.as_bytes())
                .map_err(|_| "api_key_header must be a valid HTTP header name".to_owned())?;

            request.header(header_name, api_key)
        }
        _ => request,
    };
    let response = request
        .send()
        .await
        .map_err(|error| format!("erp_private_messages request failed: {error}"))?;
    let status = response.status();

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "erp_private_messages request returned {status}: {body}"
        ));
    }

    response
        .json::<Value>()
        .await
        .map_err(|error| format!("erp_private_messages response was not valid JSON: {error}"))
}

pub(super) fn normalize_erp_message_notification(item: &Value) -> Value {
    let title = field_string(
        item,
        &[
            "title",
            "subject",
            "summary",
            "type",
            "request_type",
            "requestType",
        ],
    )
    .unwrap_or_else(|| "ERP message".to_owned());
    let external_id = notification_external_id(
        "erp",
        item,
        &[
            "external_id",
            "id",
            "message_id",
            "messageId",
            "request_id",
            "requestId",
            "approval_id",
            "approvalId",
            "task_id",
            "ticket_id",
        ],
        &title,
    );

    json!({
        "external_id": external_id,
        "title": title,
        "body": erp_message_body(item),
        "severity": erp_message_severity(item),
        "is_read": field_bool(item, &["is_read", "isRead", "read", "seen"]).unwrap_or(false),
        "url": field_url(item, &["url", "web_url", "web_link", "webLink"])
    })
}

fn erp_message_body(item: &Value) -> Option<String> {
    let mut details = Vec::new();

    if let Some(message) = field_string(item, &["body", "message", "description", "preview"]) {
        details.push(message);
    }
    if let Some(sender) = person_display(item, &["requester", "sender", "from", "owner"]) {
        details.push(format!("From: {sender}"));
    }
    if let Some(status) = field_string(item, &["status", "state"]) {
        details.push(format!("Status: {status}"));
    }
    if let Some(due_at) = normalized_time_field(
        item,
        &["due_at", "dueAt", "deadline", "expires_at", "expiresAt"],
    ) {
        details.push(format!("Due: {due_at}"));
    }

    (!details.is_empty()).then(|| details.join(" | "))
}

fn erp_message_severity(item: &Value) -> &'static str {
    if let Some(severity) = field_string(item, &["severity"]) {
        return normalize_notification_severity(&severity, "warning");
    }
    if let Some(priority) = field_string(item, &["priority", "importance"]) {
        return normalize_notification_severity(&priority, "warning");
    }
    if let Some(status) = field_string(item, &["status", "state"]) {
        match status.trim().to_ascii_lowercase().as_str() {
            "critical" | "urgent" | "blocked" | "overdue" | "failed" | "failure" | "escalated" => {
                return "critical"
            }
            "pending" | "waiting" | "waiting_approval" | "requires_action" | "submitted"
            | "open" => return "warning",
            "closed" | "done" | "approved" | "completed" | "resolved" => return "info",
            _ => {}
        }
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

fn erp_message_items(response: &Value) -> Vec<&Value> {
    response
        .get("items")
        .or_else(|| response.get("messages"))
        .or_else(|| response.get("private_messages"))
        .or_else(|| response.get("privateMessages"))
        .or_else(|| response.pointer("/data/items"))
        .or_else(|| response.pointer("/data/messages"))
        .or_else(|| response.pointer("/data/private_messages"))
        .or_else(|| response.pointer("/data/privateMessages"))
        .or_else(|| response.get("data"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .into_iter()
        .chain(response.as_array().into_iter().flatten())
        .collect()
}

fn erp_private_messages_url(config: &ErpPrivateMessagesConfig) -> Result<String, String> {
    let base_url = config
        .messages_url
        .as_deref()
        .or(config.private_messages_url.as_deref())
        .or(config.url.as_deref())
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| {
            "erp_private_messages config must set messages_url, private_messages_url, or url"
                .to_owned()
        })?;
    require_url("messages_url", base_url)?;

    Ok(append_query_params(
        base_url,
        &erp_private_messages_query_params(config),
    ))
}

fn erp_private_messages_query_params(
    config: &ErpPrivateMessagesConfig,
) -> Vec<(&'static str, String)> {
    let mut params = Vec::new();

    let since = config
        .since
        .as_deref()
        .or(config.updated_after.as_deref())
        .or(config.received_after.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            config.lookback_hours.map(|lookback_hours| {
                format_graph_datetime(
                    Utc::now() - ChronoDuration::hours(lookback_hours.clamp(1, 720)),
                )
            })
        });

    if let Some(since) = since {
        params.push(("since", since));
    }

    if let Some(unread_only) = config.unread_only {
        params.push(("unread_only", unread_only.to_string()));
    }

    if let Some(limit) = config.limit.or(config.top) {
        params.push(("limit", limit.clamp(1, 100).to_string()));
    }

    params
}
