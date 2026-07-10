use chrono::{Duration as ChronoDuration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

use super::super::shared::{
    append_query_params, encode_url_component, field_bool, field_string, field_url,
    format_graph_datetime, normalize_naive_datetime, notification_external_id, person_display,
    require_url,
};
use super::super::ConnectorAdapterResult;
use super::oauth::graph_access_token;
use super::{fetch_graph_collection, graph_pagination_limits};

#[derive(Deserialize)]
struct MicrosoftGraphMailConfig {
    adapter: Option<String>,
    messages_url: Option<String>,
    mail_messages_url: Option<String>,
    base_url: Option<String>,
    user_id: Option<String>,
    mail_folder_id: Option<String>,
    folder_id: Option<String>,
    folder: Option<String>,
    received_after: Option<String>,
    lookback_hours: Option<i64>,
    unread_only: Option<bool>,
    filter: Option<String>,
    orderby: Option<String>,
    top: Option<u64>,
    max_pages: Option<u64>,
    max_items: Option<u64>,
    timeout_seconds: Option<u64>,
}

pub(in crate::connector_adapters) async fn fetch_microsoft_graph_mail_messages(
    config_json: &str,
) -> Result<ConnectorAdapterResult, String> {
    let mut config_value = serde_json::from_str::<Value>(config_json)
        .map_err(|error| format!("microsoft_graph_mail config is not valid JSON: {error}"))?;
    let config = serde_json::from_str::<MicrosoftGraphMailConfig>(config_json)
        .map_err(|error| format!("microsoft_graph_mail config is not valid JSON: {error}"))?;

    if !matches!(
        config.adapter.as_deref(),
        Some("microsoft_graph_mail" | "graph_mail" | "outlook_mail")
    ) {
        return Err(
            "microsoft_graph_mail config must set adapter to microsoft_graph_mail".to_owned(),
        );
    }

    let messages_url = microsoft_graph_mail_messages_url(&config);
    require_url("messages_url", &messages_url)?;
    let (max_pages, max_items) =
        graph_pagination_limits("microsoft_graph_mail", config.max_pages, config.max_items)?;
    let request_url = append_query_params(&messages_url, &graph_mail_query_params(&config));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(
            config.timeout_seconds.unwrap_or(15).max(1),
        ))
        .build()
        .map_err(|error| format!("microsoft_graph_mail HTTP client could not be built: {error}"))?;
    let access_token = graph_access_token(
        &client,
        &mut config_value,
        "microsoft_graph_mail",
        "https://graph.microsoft.com/Mail.Read offline_access",
    )
    .await?;
    let collection = fetch_graph_collection(
        &client,
        &request_url,
        &access_token.token,
        None,
        "microsoft_graph_mail",
        max_pages,
        max_items,
    )
    .await?;
    let snapshot_complete = collection.snapshot_complete;
    let items = collection
        .items
        .into_iter()
        .map(|item| normalize_graph_mail_message(&item))
        .collect::<Vec<_>>();

    Ok(ConnectorAdapterResult {
        payload: Some(json!({
            "items": items,
            "snapshot_complete": snapshot_complete
        })),
        updated_config: access_token.updated_config,
    })
}

fn normalize_graph_mail_message(item: &Value) -> Value {
    let subject = field_string(item, &["subject", "title"])
        .unwrap_or_else(|| "Outlook mail message".to_owned());
    let external_id = notification_external_id(
        "mail",
        item,
        &[
            "external_id",
            "id",
            "message_id",
            "internetMessageId",
            "internet_message_id",
        ],
        &subject,
    );
    let title = if subject.to_ascii_lowercase().starts_with("mail:") {
        subject
    } else {
        format!("Mail: {subject}")
    };

    json!({
        "external_id": external_id,
        "title": title,
        "body": graph_mail_body(item),
        "severity": graph_mail_severity(item),
        "is_read": field_bool(item, &["isRead", "is_read", "read", "seen"]).unwrap_or(false),
        "url": graph_mail_url(item)
    })
}

fn graph_mail_body(item: &Value) -> Option<String> {
    let mut details = Vec::new();

    if let Some(sender) = person_display(item, &["from", "sender"]) {
        details.push(format!("From: {sender}"));
    }
    if let Some(received_at) = field_string(item, &["receivedDateTime", "received_at"])
        .map(|value| normalize_naive_datetime(&value).unwrap_or(value))
    {
        details.push(format!("Received: {received_at}"));
    }
    if let Some(preview) = field_string(
        item,
        &["bodyPreview", "body_preview", "preview", "summary", "body"],
    ) {
        details.push(format!("Preview: {preview}"));
    }

    (!details.is_empty()).then(|| details.join(" | "))
}

fn graph_mail_url(item: &Value) -> Option<String> {
    field_url(item, &["webLink", "web_link", "web_url", "url"])
}

fn graph_mail_severity(item: &Value) -> &'static str {
    if let Some(flag_status) = item
        .pointer("/flag/flagStatus")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !flag_status.eq_ignore_ascii_case("notFlagged") {
            return "warning";
        }
    }

    let severity = field_string(item, &["severity", "importance", "priority"])
        .map(|value| value.trim().to_ascii_lowercase());
    match severity.as_deref() {
        Some("critical" | "urgent" | "blocker" | "error" | "failed" | "failure") => "critical",
        Some("high" | "warning" | "warn" | "medium") => "warning",
        _ => "info",
    }
}

fn microsoft_graph_mail_messages_url(config: &MicrosoftGraphMailConfig) -> String {
    if let Some(url) = config
        .messages_url
        .as_deref()
        .or(config.mail_messages_url.as_deref())
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        return url.to_owned();
    }

    let base_url = config
        .base_url
        .as_deref()
        .unwrap_or("https://graph.microsoft.com/v1.0")
        .trim_end_matches('/');
    let user_id = config
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|user_id| !user_id.is_empty())
        .unwrap_or("me");
    let mailbox_root = if user_id.eq_ignore_ascii_case("me") {
        format!("{base_url}/me")
    } else {
        format!("{base_url}/users/{}", encode_url_component(user_id))
    };

    match config
        .mail_folder_id
        .as_deref()
        .or(config.folder_id.as_deref())
        .or(config.folder.as_deref())
        .map(str::trim)
        .filter(|folder| !folder.is_empty())
    {
        Some(folder) => format!(
            "{mailbox_root}/mailFolders/{}/messages",
            encode_url_component(folder)
        ),
        None => format!("{mailbox_root}/messages"),
    }
}

fn graph_mail_query_params(config: &MicrosoftGraphMailConfig) -> Vec<(&'static str, String)> {
    let mut params = vec![
        (
            "$select",
            "id,subject,bodyPreview,importance,isRead,webLink,from,sender,receivedDateTime,internetMessageId,flag"
                .to_owned(),
        ),
        ("$top", config.top.unwrap_or(25).clamp(1, 50).to_string()),
        (
            "$orderby",
            config
                .orderby
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("receivedDateTime desc")
                .to_owned(),
        ),
    ];

    let filter = config
        .filter
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            let mut filters = Vec::new();
            let received_after = config
                .received_after
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| {
                    let lookback_hours = config.lookback_hours.unwrap_or(24).clamp(1, 720);
                    format_graph_datetime(Utc::now() - ChronoDuration::hours(lookback_hours))
                });
            filters.push(format!("receivedDateTime ge {received_after}"));

            if config.unread_only.unwrap_or(true) {
                filters.push("isRead eq false".to_owned());
            }

            filters.join(" and ")
        });

    if !filter.is_empty() {
        params.push(("$filter", filter));
    }

    params
}

#[cfg(test)]
mod tests {
    use super::fetch_microsoft_graph_mail_messages;
    use crate::connector_adapters::shared::test_support::{MockHttpServer, MockResponse};
    use serde_json::json;

    #[rocket::async_test]
    async fn follows_graph_mail_next_links_and_honors_max_items() {
        let server = MockHttpServer::start(vec![
            MockResponse::json(
                r#"{"value":[{"id":"mail-1","subject":"First"}],"@odata.nextLink":"{{base_url}}/mail?page=2"}"#,
            ),
            MockResponse::json(
                r#"{"value":[{"id":"mail-2","subject":"Second"},{"id":"mail-3","subject":"Third"}]}"#,
            ),
        ]);
        let config = json!({
            "adapter": "microsoft_graph_mail",
            "messages_url": server.url("/mail"),
            "access_token": "test-token",
            "received_after": "2026-07-09T00:00:00Z",
            "max_pages": 5,
            "max_items": 2
        });

        let result = fetch_microsoft_graph_mail_messages(&config.to_string())
            .await
            .expect("mail pages should load");
        let payload = result.payload.expect("mail payload");
        let items = payload["items"].as_array().expect("mail items");

        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["external_id"], "mail-1");
        assert_eq!(items[1]["external_id"], "mail-2");
        assert_eq!(payload["snapshot_complete"], false);
        assert_eq!(server.requests().len(), 2);
    }
}
