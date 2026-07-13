use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rocket::fs::NamedFile;
use rocket::serde::json::Json;
use rocket::serde::{Deserialize, Serialize};
use rocket_db_pools::Connection;
use serde_json::{json, Value};
use std::fmt::Write as _;
use std::path::Path;
use uuid::Uuid;

use crate::api::{ok, ApiResult};
use crate::auth::{require_admin, AuthenticatedUser};
use crate::crypto::{decrypt_connector_config, encrypt_connector_config};
use crate::repositories::{ConnectorConfigRepository, ConnectorRepository};
use crate::rocket_routes::audit_logs::record_audit_log;
use crate::rocket_routes::connectors::shared::{
    validate_source, validation_error, validation_error_dynamic,
};
use crate::rocket_routes::connectors::types::{
    ConnectorConfigResponse, MicrosoftOAuthAuthorizeRequest, MicrosoftOAuthAuthorizeResponse,
    MicrosoftOAuthCallbackRequest, MicrosoftOAuthCallbackResponse,
};
use crate::rocket_routes::DbConn;
use crate::validation::validate_request;

const MICROSOFT_GRAPH_CALENDAR_SCOPE: &str =
    "https://graph.microsoft.com/Calendars.Read offline_access";
const MICROSOFT_GRAPH_MAIL_SCOPE: &str = "https://graph.microsoft.com/Mail.Read offline_access";

#[derive(Deserialize)]
struct MicrosoftTokenResponse {
    access_token: Option<String>,
    token_type: Option<String>,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
    scope: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct MicrosoftOAuthState {
    source: String,
    nonce: String,
    issued_at: String,
}

#[rocket::post(
    "/connectors/<source>/oauth/microsoft/authorize",
    format = "json",
    data = "<request>"
)]
pub async fn start_microsoft_oauth(
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
    source: String,
    request: Json<MicrosoftOAuthAuthorizeRequest>,
) -> ApiResult<MicrosoftOAuthAuthorizeResponse> {
    require_admin(&auth)?;
    let source = validate_source(source)?;
    let request = validate_request(request.into_inner())?;
    ConnectorRepository::find_by_source(&mut db, &source).await?;
    let config = ConnectorConfigRepository::find_by_source(&mut db, &source).await?;
    let mut config_value = decrypt_config_json(&config.config)?;

    let adapter_name = config_field(&config_value, &["adapter"]).ok_or_else(|| {
        validation_error(
            "config",
            "must set adapter to microsoft_graph_calendar or microsoft_graph_mail",
        )
    })?;
    let default_scope = default_scope_for_adapter(&adapter_name).ok_or_else(|| {
        validation_error(
            "config",
            "adapter must be microsoft_graph_calendar or microsoft_graph_mail",
        )
    })?;
    let scope = graph_scope(&config_value).unwrap_or_else(|| default_scope.to_owned());
    let client_id = config_field(&config_value, &["client_id", "application_id", "app_id"])
        .ok_or_else(|| validation_error("config", "must set client_id before OAuth connect"))?;
    let authorization_url = microsoft_authorization_url(&config_value);
    require_http_url("authorization_url", &authorization_url)?;

    let state = encode_oauth_state(&source)?;
    let expires_at = Utc::now() + ChronoDuration::minutes(10);
    let prompt = request
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("select_account")
        .to_owned();
    let authorization_url = append_query_params(
        &authorization_url,
        &[
            ("client_id", client_id),
            ("response_type", "code".to_owned()),
            ("redirect_uri", request.redirect_uri.clone()),
            ("response_mode", "query".to_owned()),
            ("scope", scope.clone()),
            ("state", state.clone()),
            ("prompt", prompt),
        ],
    );

    let config_object = config_value
        .as_object_mut()
        .ok_or_else(|| validation_error("config", "must be a JSON object"))?;
    config_object.insert(
        "oauth_provider".to_owned(),
        Value::String("microsoft".to_owned()),
    );
    config_object.insert("oauth_state".to_owned(), Value::String(state.clone()));
    config_object.insert(
        "oauth_state_expires_at".to_owned(),
        Value::String(format_oauth_datetime(expires_at)),
    );
    config_object.insert(
        "oauth_redirect_uri".to_owned(),
        Value::String(request.redirect_uri.clone()),
    );
    config_object.remove("oauth_last_error");

    let encrypted_config = encrypt_connector_config_json(&config_value)?;
    ConnectorConfigRepository::update_config(&mut db, &source, encrypted_config).await?;
    record_audit_log(
        &mut db,
        &auth,
        "oauth_authorize",
        "connector",
        &source,
        json!({
            "provider": "microsoft",
            "scope": scope,
        }),
    )
    .await?;

    ok(MicrosoftOAuthAuthorizeResponse {
        authorization_url,
        state,
        redirect_uri: request.redirect_uri,
        scope,
        expires_at,
    })
}

#[rocket::post(
    "/connectors/oauth/microsoft/callback",
    format = "json",
    data = "<request>"
)]
pub async fn finish_microsoft_oauth(
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
    request: Json<MicrosoftOAuthCallbackRequest>,
) -> ApiResult<MicrosoftOAuthCallbackResponse> {
    require_admin(&auth)?;
    let request = validate_request(request.into_inner())?;
    if let Some(error) = request
        .error
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let description = request
            .error_description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("no error description");

        return Err(validation_error_dynamic(
            "oauth",
            format!("Microsoft OAuth returned {error}: {description}"),
        ));
    }
    let code = request
        .code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| validation_error("code", "is required"))?
        .to_owned();
    let state = decode_oauth_state(&request.state)?;
    let source = validate_source(state.source)?;
    ConnectorRepository::find_by_source(&mut db, &source).await?;
    let config = ConnectorConfigRepository::find_by_source(&mut db, &source).await?;
    let mut config_value = decrypt_config_json(&config.config)?;

    validate_pending_oauth_state(&config_value, &request.state, &request.redirect_uri)?;
    let client_id = config_field(&config_value, &["client_id", "application_id", "app_id"])
        .ok_or_else(|| validation_error("config", "must set client_id before OAuth callback"))?;
    let client_secret = config_field(&config_value, &["client_secret"]);
    let adapter_name = config_field(&config_value, &["adapter"]).ok_or_else(|| {
        validation_error(
            "config",
            "must set adapter to microsoft_graph_calendar or microsoft_graph_mail",
        )
    })?;
    let default_scope = default_scope_for_adapter(&adapter_name).ok_or_else(|| {
        validation_error(
            "config",
            "adapter must be microsoft_graph_calendar or microsoft_graph_mail",
        )
    })?;
    let scope = graph_scope(&config_value).unwrap_or_else(|| default_scope.to_owned());
    let token_url = microsoft_token_url(&config_value);
    require_http_url("token_url", &token_url)?;

    let token_response = exchange_authorization_code(
        &token_url,
        client_id,
        client_secret,
        code,
        request.redirect_uri.clone(),
        scope,
    )
    .await?;
    apply_token_response(&mut config_value, token_response)?;

    let encrypted_config = encrypt_connector_config_json(&config_value)?;
    let config =
        ConnectorConfigRepository::update_config(&mut db, &source, encrypted_config).await?;
    record_audit_log(
        &mut db,
        &auth,
        "oauth_connect",
        "connector",
        &source,
        json!({
            "provider": "microsoft",
        }),
    )
    .await?;

    ok(MicrosoftOAuthCallbackResponse {
        source,
        config: ConnectorConfigResponse::from(config),
    })
}

#[rocket::get("/oauth/microsoft/callback")]
pub async fn microsoft_oauth_callback_page() -> Option<NamedFile> {
    NamedFile::open(Path::new("frontend/dist/index.html"))
        .await
        .ok()
}

async fn exchange_authorization_code(
    token_url: &str,
    client_id: String,
    client_secret: Option<String>,
    code: String,
    redirect_uri: String,
    scope: String,
) -> Result<MicrosoftTokenResponse, crate::api::ApiError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| validation_error_dynamic("oauth", error.to_string()))?;
    let mut form = vec![
        ("client_id", client_id),
        ("grant_type", "authorization_code".to_owned()),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("scope", scope),
    ];
    if let Some(client_secret) = client_secret {
        form.push(("client_secret", client_secret));
    }

    let response = client
        .post(token_url)
        .form(&form)
        .send()
        .await
        .map_err(|error| validation_error_dynamic("oauth", error.to_string()))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(validation_error_dynamic(
            "oauth",
            format!("Microsoft token endpoint returned {status}: {body}"),
        ));
    }

    let token_response =
        serde_json::from_str::<MicrosoftTokenResponse>(&body).map_err(|error| {
            validation_error_dynamic(
                "oauth",
                format!("Microsoft token response was not valid JSON: {error}"),
            )
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

        return Err(validation_error_dynamic(
            "oauth",
            format!("Microsoft token endpoint returned {error}: {description}"),
        ));
    }

    Ok(token_response)
}

fn apply_token_response(
    config: &mut Value,
    token_response: MicrosoftTokenResponse,
) -> Result<(), crate::api::ApiError> {
    let access_token = token_response
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| validation_error("oauth", "token response did not include access_token"))?
        .to_owned();
    let refresh_token = token_response
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            validation_error(
                "oauth",
                "token response did not include refresh_token; confirm offline_access was consented",
            )
        })?
        .to_owned();
    let expires_in = token_response.expires_in.unwrap_or(3600).max(60);
    let now = Utc::now();
    let expires_at = format_oauth_datetime(now + ChronoDuration::seconds(expires_in));

    let config_object = config
        .as_object_mut()
        .ok_or_else(|| validation_error("config", "must be a JSON object"))?;
    config_object.insert("access_token".to_owned(), Value::String(access_token));
    config_object.insert("refresh_token".to_owned(), Value::String(refresh_token));
    config_object.insert(
        "access_token_expires_at".to_owned(),
        Value::String(expires_at),
    );
    config_object.insert(
        "token_refreshed_at".to_owned(),
        Value::String(format_oauth_datetime(now)),
    );
    config_object.insert(
        "oauth_connected_at".to_owned(),
        Value::String(format_oauth_datetime(now)),
    );
    if let Some(token_type) = token_response
        .token_type
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        config_object.insert("token_type".to_owned(), Value::String(token_type));
    }
    if let Some(scope) = token_response
        .scope
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        config_object.insert("scope".to_owned(), Value::String(scope));
    }
    config_object.remove("oauth_state");
    config_object.remove("oauth_state_expires_at");
    config_object.remove("oauth_redirect_uri");
    config_object.remove("oauth_last_error");

    Ok(())
}

fn validate_pending_oauth_state(
    config: &Value,
    state: &str,
    redirect_uri: &str,
) -> Result<(), crate::api::ApiError> {
    let stored_state = config_field(config, &["oauth_state"])
        .ok_or_else(|| validation_error("state", "does not match a pending OAuth connection"))?;
    if stored_state != state {
        return Err(validation_error(
            "state",
            "does not match a pending OAuth connection",
        ));
    }

    let stored_redirect_uri = config_field(config, &["oauth_redirect_uri"])
        .ok_or_else(|| validation_error("redirect_uri", "does not match pending OAuth state"))?;
    if stored_redirect_uri != redirect_uri {
        return Err(validation_error(
            "redirect_uri",
            "does not match pending OAuth state",
        ));
    }

    let expires_at = config_field(config, &["oauth_state_expires_at"])
        .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
        .map(|value| value.with_timezone(&Utc))
        .ok_or_else(|| validation_error("state", "is missing or has expired"))?;
    if expires_at <= Utc::now() {
        return Err(validation_error("state", "has expired"));
    }

    Ok(())
}

fn encode_oauth_state(source: &str) -> Result<String, crate::api::ApiError> {
    let state = MicrosoftOAuthState {
        source: source.to_owned(),
        nonce: Uuid::new_v4().to_string(),
        issued_at: format_oauth_datetime(Utc::now()),
    };
    let encoded = serde_json::to_vec(&state)
        .map_err(|error| validation_error_dynamic("state", error.to_string()))?;

    Ok(URL_SAFE_NO_PAD.encode(encoded))
}

fn decode_oauth_state(state: &str) -> Result<MicrosoftOAuthState, crate::api::ApiError> {
    let decoded = URL_SAFE_NO_PAD
        .decode(state)
        .map_err(|_| validation_error("state", "is invalid"))?;

    serde_json::from_slice::<MicrosoftOAuthState>(&decoded)
        .map_err(|_| validation_error("state", "is invalid"))
}

fn decrypt_config_json(config: &str) -> Result<Value, crate::api::ApiError> {
    let config = decrypt_connector_config(config)
        .map_err(|error| validation_error_dynamic("config", error))?;

    serde_json::from_str::<Value>(&config)
        .map_err(|error| validation_error_dynamic("config", error.to_string()))
}

fn encrypt_connector_config_json(config: &Value) -> Result<String, crate::api::ApiError> {
    let config = serde_json::to_string(config)
        .map_err(|error| validation_error_dynamic("config", error.to_string()))?;

    encrypt_connector_config(&config).map_err(|error| validation_error_dynamic("config", error))
}

fn microsoft_authorization_url(config: &Value) -> String {
    config_field(
        config,
        &["authorization_url", "authorize_url", "oauth_authorize_url"],
    )
    .unwrap_or_else(|| {
        let tenant = graph_tenant(config);
        format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
            encode_url_component(&tenant)
        )
    })
}

fn microsoft_token_url(config: &Value) -> String {
    config_field(config, &["token_url", "oauth_token_url"]).unwrap_or_else(|| {
        let tenant = graph_tenant(config);
        format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            encode_url_component(&tenant)
        )
    })
}

fn graph_tenant(config: &Value) -> String {
    config_field(config, &["tenant_id", "tenant", "directory_id"])
        .unwrap_or_else(|| "organizations".to_owned())
}

fn default_scope_for_adapter(adapter: &str) -> Option<&'static str> {
    match adapter {
        "microsoft_graph_calendar" | "graph_calendar" | "outlook_calendar" => {
            Some(MICROSOFT_GRAPH_CALENDAR_SCOPE)
        }
        "microsoft_graph_mail" | "graph_mail" | "outlook_mail" => Some(MICROSOFT_GRAPH_MAIL_SCOPE),
        _ => None,
    }
}

fn graph_scope(config: &Value) -> Option<String> {
    config_field(config, &["scope", "scopes"]).or_else(|| {
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

fn config_field(config: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| config.get(*name))
        .and_then(|value| match value {
            Value::String(value) => Some(value.trim().to_owned()),
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
        .filter(|value| !value.is_empty())
}

fn require_http_url(field: &'static str, value: &str) -> Result<(), crate::api::ApiError> {
    if value.starts_with("http://") || value.starts_with("https://") {
        Ok(())
    } else {
        Err(validation_error(field, "must be an absolute HTTP URL"))
    }
}

fn format_oauth_datetime(datetime: DateTime<Utc>) -> String {
    datetime.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
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
