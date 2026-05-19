use chrono::{DateTime, Duration as ChronoDuration, NaiveDateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt::Write as _;
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

#[derive(Deserialize)]
struct MicrosoftGraphCalendarConfig {
    adapter: Option<String>,
    calendar_view_url: Option<String>,
    base_url: Option<String>,
    user_id: Option<String>,
    start_at: Option<String>,
    end_at: Option<String>,
    lookahead_hours: Option<i64>,
    time_zone: Option<String>,
    top: Option<u64>,
    timeout_seconds: Option<u64>,
}

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
    timeout_seconds: Option<u64>,
}

#[derive(Deserialize)]
struct GraphTokenResponse {
    access_token: Option<String>,
    token_type: Option<String>,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
    scope: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

pub struct ConnectorAdapterResult {
    pub payload: Option<Value>,
    pub updated_config: Option<String>,
}

struct GraphAccessToken {
    token: String,
    updated_config: Option<String>,
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
) -> Result<ConnectorAdapterResult, String> {
    let adapter = serde_json::from_str::<AdapterConfig>(config_json)
        .map_err(|error| format!("connector config is not valid JSON: {error}"))?
        .adapter;

    match adapter.as_deref() {
        Some("azure_devops") if target == "work_cards" => {
            fetch_azure_devops_work_cards(config_json)
                .await
                .map(adapter_payload)
        }
        Some("azure_devops") => Err(format!(
            "azure_devops adapter does not support target {target}"
        )),
        Some("monitoring") if target == "service_health" => {
            fetch_monitoring_service_health(config_json)
                .await
                .map(adapter_payload)
        }
        Some("monitoring") => Err(format!(
            "monitoring adapter does not support target {target}"
        )),
        Some("microsoft_graph_calendar" | "graph_calendar" | "outlook_calendar") => {
            if target == "notifications" {
                fetch_microsoft_graph_calendar_events(config_json).await
            } else {
                Err(format!(
                    "microsoft_graph_calendar adapter does not support target {target}"
                ))
            }
        }
        Some("microsoft_graph_mail" | "graph_mail" | "outlook_mail") => {
            if target == "notifications" {
                fetch_microsoft_graph_mail_messages(config_json).await
            } else {
                Err(format!(
                    "microsoft_graph_mail adapter does not support target {target}"
                ))
            }
        }
        Some("calendar_sample" | "calendar") if target == "notifications" => {
            fetch_sample_notifications(config_json, SampleNotificationKind::Calendar)
                .map(adapter_payload)
        }
        Some("calendar_sample" | "calendar") => Err(format!(
            "calendar_sample adapter does not support target {target}"
        )),
        Some("outlook_mail_sample" | "outlook") if target == "notifications" => {
            fetch_sample_notifications(config_json, SampleNotificationKind::OutlookMail)
                .map(adapter_payload)
        }
        Some("outlook_mail_sample" | "outlook") => Err(format!(
            "outlook_mail_sample adapter does not support target {target}"
        )),
        Some("erp_messages_sample" | "erp_messages" | "erp") if target == "notifications" => {
            fetch_sample_notifications(config_json, SampleNotificationKind::ErpMessages)
                .map(adapter_payload)
        }
        Some("erp_messages_sample" | "erp_messages" | "erp") => Err(format!(
            "erp_messages_sample adapter does not support target {target}"
        )),
        Some(adapter) => Err(format!("connector adapter {adapter} is not supported")),
        None => Ok(ConnectorAdapterResult {
            payload: None,
            updated_config: None,
        }),
    }
}

fn adapter_payload(payload: Value) -> ConnectorAdapterResult {
    ConnectorAdapterResult {
        payload: Some(payload),
        updated_config: None,
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

async fn fetch_microsoft_graph_calendar_events(
    config_json: &str,
) -> Result<ConnectorAdapterResult, String> {
    let mut config_value = serde_json::from_str::<Value>(config_json)
        .map_err(|error| format!("microsoft_graph_calendar config is not valid JSON: {error}"))?;
    let config = serde_json::from_str::<MicrosoftGraphCalendarConfig>(config_json)
        .map_err(|error| format!("microsoft_graph_calendar config is not valid JSON: {error}"))?;

    if !matches!(
        config.adapter.as_deref(),
        Some("microsoft_graph_calendar" | "graph_calendar" | "outlook_calendar")
    ) {
        return Err(
            "microsoft_graph_calendar config must set adapter to microsoft_graph_calendar"
                .to_owned(),
        );
    }

    let calendar_view_url = microsoft_graph_calendar_view_url(&config);
    require_url("calendar_view_url", &calendar_view_url)?;

    let (start_at, end_at) = graph_calendar_time_window(&config);
    let top = config.top.unwrap_or(25).clamp(1, 50).to_string();
    let request_url = append_query_params(
        &calendar_view_url,
        &[
            ("startDateTime", start_at),
            ("endDateTime", end_at),
            (
                "$select",
                "id,subject,bodyPreview,importance,isAllDay,isCancelled,showAs,webLink,organizer,location,start,end,onlineMeetingUrl,onlineMeeting"
                    .to_owned(),
            ),
            ("$orderby", "start/dateTime".to_owned()),
            ("$top", top),
        ],
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(
            config.timeout_seconds.unwrap_or(15).max(1),
        ))
        .build()
        .map_err(|error| {
            format!("microsoft_graph_calendar HTTP client could not be built: {error}")
        })?;
    let access_token = graph_access_token(
        &client,
        &mut config_value,
        "microsoft_graph_calendar",
        "https://graph.microsoft.com/Calendars.Read offline_access",
    )
    .await?;
    let response = send_graph_calendar_request(
        client.get(&request_url),
        &access_token.token,
        config.time_zone.as_deref(),
    )
    .await?;
    let items = graph_calendar_items(&response)
        .into_iter()
        .map(normalize_graph_calendar_event)
        .collect::<Vec<_>>();

    Ok(ConnectorAdapterResult {
        payload: Some(json!({ "items": items })),
        updated_config: access_token.updated_config,
    })
}

async fn fetch_microsoft_graph_mail_messages(
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
    let response = send_graph_mail_request(client.get(&request_url), &access_token.token).await?;
    let items = graph_mail_items(&response)
        .into_iter()
        .map(normalize_graph_mail_message)
        .collect::<Vec<_>>();

    Ok(ConnectorAdapterResult {
        payload: Some(json!({ "items": items })),
        updated_config: access_token.updated_config,
    })
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

async fn send_graph_calendar_request(
    request: reqwest::RequestBuilder,
    token: &str,
    time_zone: Option<&str>,
) -> Result<Value, String> {
    let request = request.bearer_auth(token);
    let request = match time_zone.map(str::trim).filter(|value| !value.is_empty()) {
        Some(time_zone) => request.header("Prefer", outlook_timezone_preference(time_zone)),
        None => request,
    };
    let response = request
        .send()
        .await
        .map_err(|error| format!("microsoft_graph_calendar request failed: {error}"))?;
    let status = response.status();

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "microsoft_graph_calendar request returned {status}: {body}"
        ));
    }

    response
        .json::<Value>()
        .await
        .map_err(|error| format!("microsoft_graph_calendar response was not valid JSON: {error}"))
}

async fn send_graph_mail_request(
    request: reqwest::RequestBuilder,
    token: &str,
) -> Result<Value, String> {
    let response = request
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| format!("microsoft_graph_mail request failed: {error}"))?;
    let status = response.status();

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "microsoft_graph_mail request returned {status}: {body}"
        ));
    }

    response
        .json::<Value>()
        .await
        .map_err(|error| format!("microsoft_graph_mail response was not valid JSON: {error}"))
}

async fn graph_access_token(
    client: &reqwest::Client,
    config: &mut Value,
    adapter_name: &str,
    default_scope: &str,
) -> Result<GraphAccessToken, String> {
    let access_token = field_string(config, &["access_token", "bearer_token", "token"]);
    let refresh_token = field_string(config, &["refresh_token"]);
    let refresh_disabled =
        field_bool(config, &["refresh_access_token"]).is_some_and(|enabled| !enabled);

    if let Some(refresh_token) = refresh_token.as_deref() {
        if !refresh_disabled && graph_access_token_needs_refresh(config, access_token.as_deref()) {
            return refresh_graph_access_token(
                client,
                config,
                adapter_name,
                default_scope,
                refresh_token.to_owned(),
            )
            .await;
        }
    }

    if let Some(token) = access_token {
        return Ok(GraphAccessToken {
            token,
            updated_config: None,
        });
    }

    if let Some(refresh_token) = refresh_token {
        return refresh_graph_access_token(
            client,
            config,
            adapter_name,
            default_scope,
            refresh_token,
        )
        .await;
    }

    Err(format!(
        "{adapter_name} config must set access_token or OAuth refresh_token credentials"
    ))
}

async fn refresh_graph_access_token(
    client: &reqwest::Client,
    config: &mut Value,
    adapter_name: &str,
    default_scope: &str,
    refresh_token: String,
) -> Result<GraphAccessToken, String> {
    let token_url = graph_token_url(config);
    require_url("token_url", &token_url)?;

    let client_id = field_string(config, &["client_id", "application_id", "app_id"])
        .ok_or_else(|| format!("{adapter_name} config must set client_id for token refresh"))?;
    let client_secret = field_string(config, &["client_secret"]);
    let scope = graph_scope(config).unwrap_or_else(|| default_scope.to_owned());

    let mut form = vec![
        ("client_id", client_id),
        ("grant_type", "refresh_token".to_owned()),
        ("refresh_token", refresh_token),
        ("scope", scope),
    ];
    if let Some(client_secret) = client_secret {
        form.push(("client_secret", client_secret));
    }

    let response = client
        .post(&token_url)
        .form(&form)
        .send()
        .await
        .map_err(|error| format!("{adapter_name} OAuth token refresh failed: {error}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(format!(
            "{adapter_name} OAuth token refresh returned {status}: {body}"
        ));
    }

    let token_response = serde_json::from_str::<GraphTokenResponse>(&body).map_err(|error| {
        format!("{adapter_name} OAuth token response was not valid JSON: {error}")
    })?;
    if let Some(error) = token_response
        .error
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let description = token_response
            .error_description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("no error description");

        return Err(format!(
            "{adapter_name} OAuth token refresh returned {error}: {description}"
        ));
    }

    let access_token = token_response
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{adapter_name} OAuth token response did not include access_token"))?
        .to_owned();
    let expires_in = token_response.expires_in.unwrap_or(3600).max(60);
    let expires_at = format_graph_datetime(Utc::now() + ChronoDuration::seconds(expires_in));
    let refreshed_at = format_graph_datetime(Utc::now());

    let Some(config_object) = config.as_object_mut() else {
        return Err(format!("{adapter_name} config must be a JSON object"));
    };
    config_object.insert(
        "access_token".to_owned(),
        Value::String(access_token.clone()),
    );
    config_object.insert(
        "access_token_expires_at".to_owned(),
        Value::String(expires_at),
    );
    config_object.insert("token_refreshed_at".to_owned(), Value::String(refreshed_at));
    if let Some(token_type) = token_response
        .token_type
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        config_object.insert("token_type".to_owned(), Value::String(token_type));
    }
    if let Some(refresh_token) = token_response
        .refresh_token
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        config_object.insert("refresh_token".to_owned(), Value::String(refresh_token));
    }
    if let Some(scope) = token_response
        .scope
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        config_object.insert("scope".to_owned(), Value::String(scope));
    }

    let updated_config = serde_json::to_string(config).map_err(|error| {
        format!("{adapter_name} refreshed config could not be encoded: {error}")
    })?;

    Ok(GraphAccessToken {
        token: access_token,
        updated_config: Some(updated_config),
    })
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

fn graph_calendar_items(response: &Value) -> Vec<&Value> {
    response
        .get("value")
        .or_else(|| response.get("items"))
        .or_else(|| response.get("events"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .into_iter()
        .chain(response.as_array().into_iter().flatten())
        .collect()
}

fn graph_mail_items(response: &Value) -> Vec<&Value> {
    response
        .get("value")
        .or_else(|| response.get("items"))
        .or_else(|| response.get("messages"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .into_iter()
        .chain(response.as_array().into_iter().flatten())
        .collect()
}

fn normalize_graph_calendar_event(item: &Value) -> Value {
    let subject =
        field_string(item, &["subject", "title"]).unwrap_or_else(|| "Calendar event".to_owned());
    let external_id = notification_external_id(
        "calendar",
        item,
        &[
            "external_id",
            "id",
            "event_id",
            "uid",
            "iCalUId",
            "ical_uid",
        ],
        &subject,
    );
    let title = if subject.to_ascii_lowercase().starts_with("calendar:") {
        subject
    } else {
        format!("Calendar: {subject}")
    };

    json!({
        "external_id": external_id,
        "title": title,
        "body": graph_calendar_body(item),
        "severity": graph_calendar_severity(item),
        "is_read": false,
        "url": graph_calendar_url(item)
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

fn graph_calendar_body(item: &Value) -> Option<String> {
    let mut details = Vec::new();

    if let Some(organizer) = person_display(item, &["organizer"]) {
        details.push(format!("Organizer: {organizer}"));
    }
    if let Some(location) = graph_event_location(item) {
        details.push(format!("Location: {location}"));
    }
    if let Some(starts_at) = graph_event_datetime(item, "start") {
        details.push(format!("Starts: {starts_at}"));
    }
    if let Some(ends_at) = graph_event_datetime(item, "end") {
        details.push(format!("Ends: {ends_at}"));
    }
    if let Some(preview) = field_string(item, &["bodyPreview", "body_preview", "preview"]) {
        details.push(format!("Preview: {preview}"));
    }

    (!details.is_empty()).then(|| details.join(" | "))
}

fn graph_event_location(item: &Value) -> Option<String> {
    item.get("location")
        .and_then(|location| {
            field_string(
                location,
                &[
                    "displayName",
                    "display_name",
                    "locationUri",
                    "location_uri",
                    "uniqueId",
                    "unique_id",
                ],
            )
        })
        .or_else(|| field_string(item, &["location", "room"]))
}

fn graph_event_datetime(item: &Value, field_name: &str) -> Option<String> {
    let value = item.get(field_name)?;
    let datetime = field_string(value, &["dateTime", "date_time"])?;
    let datetime = normalize_naive_datetime(&datetime).unwrap_or(datetime);

    match field_string(value, &["timeZone", "time_zone"]) {
        Some(time_zone) => Some(format!("{datetime} {time_zone}")),
        None => Some(datetime),
    }
}

fn graph_calendar_url(item: &Value) -> Option<String> {
    field_url(item, &["webLink", "web_link", "web_url", "url"])
        .or_else(|| {
            field_url(
                item,
                &["onlineMeetingUrl", "online_meeting_url", "join_url"],
            )
        })
        .or_else(|| {
            item.pointer("/onlineMeeting/joinUrl")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
                .map(ToOwned::to_owned)
        })
}

fn graph_calendar_severity(item: &Value) -> &'static str {
    if field_bool(item, &["isCancelled", "is_cancelled"]).unwrap_or(false) {
        return "warning";
    }

    let severity = field_string(item, &["severity", "importance", "priority"])
        .map(|value| value.trim().to_ascii_lowercase());
    match severity.as_deref() {
        Some("critical" | "urgent" | "blocker" | "error" | "failed" | "failure") => "critical",
        Some("warning" | "warn" | "high" | "medium") => "warning",
        _ => "info",
    }
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

fn microsoft_graph_calendar_view_url(config: &MicrosoftGraphCalendarConfig) -> String {
    if let Some(url) = config
        .calendar_view_url
        .as_deref()
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

    if user_id.eq_ignore_ascii_case("me") {
        format!("{base_url}/me/calendarView")
    } else {
        format!(
            "{base_url}/users/{}/calendarView",
            encode_url_component(user_id)
        )
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

fn graph_calendar_time_window(config: &MicrosoftGraphCalendarConfig) -> (String, String) {
    let now = Utc::now();
    let start_at = config
        .start_at
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format_graph_datetime(now));
    let end_at = config
        .end_at
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            let lookahead_hours = config.lookahead_hours.unwrap_or(24).clamp(1, 168);
            let start = DateTime::parse_from_rfc3339(&start_at)
                .map(|datetime| datetime.with_timezone(&Utc))
                .unwrap_or(now);

            format_graph_datetime(start + ChronoDuration::hours(lookahead_hours))
        });

    (start_at, end_at)
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

fn graph_access_token_needs_refresh(config: &Value, access_token: Option<&str>) -> bool {
    if access_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return true;
    }

    let Some(expires_at) = field_string(
        config,
        &["access_token_expires_at", "token_expires_at", "expires_at"],
    ) else {
        return true;
    };

    parse_graph_token_expires_at(&expires_at)
        .map(|expires_at| expires_at <= Utc::now() + ChronoDuration::minutes(5))
        .unwrap_or(true)
}

fn parse_graph_token_expires_at(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|datetime| datetime.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%dT%H:%M:%S")
                .ok()
                .map(|datetime| DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc))
        })
}

fn graph_token_url(config: &Value) -> String {
    if let Some(url) = field_string(config, &["token_url", "oauth_token_url"]) {
        return url;
    }

    let tenant = field_string(config, &["tenant_id", "tenant", "directory_id"])
        .unwrap_or_else(|| "organizations".to_owned());

    format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
        encode_url_component(&tenant)
    )
}

fn graph_scope(config: &Value) -> Option<String> {
    field_string(config, &["scope", "scopes"]).or_else(|| {
        config
            .get("scopes")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|value| !value.is_empty())
    })
}

fn format_graph_datetime(datetime: DateTime<Utc>) -> String {
    datetime.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn append_query_params(base_url: &str, params: &[(&str, String)]) -> String {
    let mut url = base_url.to_owned();
    let mut separator = if url.contains('?') {
        if url.ends_with('?') || url.ends_with('&') {
            ""
        } else {
            "&"
        }
    } else {
        "?"
    };

    for (key, value) in params {
        url.push_str(separator);
        url.push_str(&encode_url_component(key));
        url.push('=');
        url.push_str(&encode_url_component(value));
        separator = "&";
    }

    url
}

fn encode_url_component(value: &str) -> String {
    let mut encoded = String::new();

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                let _ = write!(&mut encoded, "%{byte:02X}");
            }
        }
    }

    encoded
}

fn outlook_timezone_preference(time_zone: &str) -> String {
    let sanitized = time_zone
        .trim()
        .replace(['\r', '\n'], "")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");

    format!("outlook.timezone=\"{sanitized}\"")
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
