use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

use super::shared::{
    field_i32, field_string, normalize_lifecycle, normalize_naive_datetime, require_url,
    stable_slug,
};

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

pub(super) async fn fetch_monitoring_service_health(config_json: &str) -> Result<Value, String> {
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
