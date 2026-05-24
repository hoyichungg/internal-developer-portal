use chrono::{DateTime, Duration as ChronoDuration, NaiveDateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::super::shared::{
    encode_url_component, field_bool, field_string, format_graph_datetime, require_url,
};

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

pub(super) struct GraphAccessToken {
    pub(super) token: String,
    pub(super) updated_config: Option<String>,
}

pub(super) async fn graph_access_token(
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
