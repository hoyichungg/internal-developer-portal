use std::env;
use std::fmt::{self, Display, Formatter};

use diesel::{sql_query, QueryableByName};
use diesel_async::{AsyncPgConnection, RunQueryDsl};

const DEFAULT_AUTH_TOKEN_TTL_SECONDS: i64 = 86_400;
const DEFAULT_AUTH_MAX_ACTIVE_SESSIONS_PER_USER: i64 = 20;
const DEFAULT_AUTH_LOGIN_MAX_FAILURES: i32 = 5;
const DEFAULT_AUTH_LOGIN_ACCOUNT_MAX_FAILURES: i32 = 50;
const DEFAULT_AUTH_LOGIN_WINDOW_SECONDS: i64 = 900;
const DEFAULT_AUTH_LOGIN_LOCKOUT_SECONDS: i64 = 900;
const DEFAULT_AUTH_OIDC_TRANSACTION_TTL_SECONDS: i64 = 600;
const DEFAULT_AUTH_ENTRA_JWKS_CACHE_SECONDS: i64 = 300;
const DEFAULT_AUTH_ENTRA_CLOCK_SKEW_SECONDS: i64 = 120;
const MIN_CONNECTOR_SECRET_KEY_BYTES: usize = 32;
const MIN_AUTH_SECRET_KEY_BYTES: usize = 32;
const SUPPORTED_ENVIRONMENTS: [&str; 3] = ["development", "test", "production"];

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub environment: String,
    pub auth_token_ttl_seconds: i64,
    pub auth_max_active_sessions_per_user: i64,
    pub auth_cookie_secure: bool,
    pub auth_login_max_failures: i32,
    pub auth_login_account_max_failures: i32,
    pub auth_login_window_seconds: i64,
    pub auth_login_lockout_seconds: i64,
    pub auth_password_login_enabled: bool,
    pub entra: Option<EntraConfig>,
}

#[derive(Clone)]
pub struct EntraConfig {
    pub tenant_id: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub transaction_key: String,
    pub redirect_uri: String,
    pub issuer: String,
    pub authorization_url: String,
    pub token_url: String,
    pub jwks_url: String,
    pub jit_provisioning: bool,
    pub required_role: Option<String>,
    pub transaction_ttl_seconds: i64,
    pub jwks_cache_seconds: i64,
    pub clock_skew_seconds: i64,
}

impl fmt::Debug for EntraConfig {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EntraConfig")
            .field("tenant_id", &self.tenant_id)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field("transaction_key", &"[REDACTED]")
            .field("redirect_uri", &self.redirect_uri)
            .field("issuer", &self.issuer)
            .field("authorization_url", &self.authorization_url)
            .field("token_url", &self.token_url)
            .field("jwks_url", &self.jwks_url)
            .field("jit_provisioning", &self.jit_provisioning)
            .field("required_role", &self.required_role)
            .field("transaction_ttl_seconds", &self.transaction_ttl_seconds)
            .field("jwks_cache_seconds", &self.jwks_cache_seconds)
            .field("clock_skew_seconds", &self.clock_skew_seconds)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ConfigError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestDatabaseTarget {
    database_name: String,
}

impl TestDatabaseTarget {
    pub fn database_name(&self) -> &str {
        &self.database_name
    }
}

#[derive(QueryableByName)]
struct CurrentDatabaseRow {
    #[diesel(sql_type = diesel::sql_types::Text)]
    current_database: String,
}

/// Validates a database URL only when the process explicitly runs in the test
/// environment. Non-test environments retain their existing URL handling.
pub fn validate_test_database_url(
    environment: &str,
    database_url: &str,
    source: &str,
) -> Result<Option<TestDatabaseTarget>, ConfigError> {
    if environment != "test" {
        return Ok(None);
    }

    let database_name = database_name_from_url(database_url).map_err(|message| {
        ConfigError::new(format!(
            "test environment requires {source} to be a PostgreSQL URL with an explicit database name: {message}"
        ))
    })?;
    if !has_standalone_test_segment(&database_name) {
        return Err(ConfigError::new(format!(
            "test environment requires {source} database name to contain a standalone 'test' segment"
        )));
    }

    Ok(Some(TestDatabaseTarget { database_name }))
}

pub async fn verify_test_database_connection(
    connection: &mut AsyncPgConnection,
    target: &TestDatabaseTarget,
    source: &str,
) -> Result<(), ConfigError> {
    let actual = sql_query("SELECT current_database() AS current_database")
        .get_result::<CurrentDatabaseRow>(connection)
        .await
        .map_err(|_| {
            ConfigError::new(format!(
                "could not verify the database selected by {source} before test writes"
            ))
        })?;

    validate_actual_test_database_name(target, &actual.current_database, source)
}

fn validate_actual_test_database_name(
    target: &TestDatabaseTarget,
    actual_database_name: &str,
    source: &str,
) -> Result<(), ConfigError> {
    if actual_database_name != target.database_name {
        return Err(ConfigError::new(format!(
            "database selected by {source} does not match its configured test database name"
        )));
    }
    if !has_standalone_test_segment(actual_database_name) {
        return Err(ConfigError::new(format!(
            "database selected by {source} does not contain a standalone 'test' segment"
        )));
    }

    Ok(())
}

fn database_name_from_url(database_url: &str) -> Result<String, &'static str> {
    let url = reqwest::Url::parse(database_url.trim()).map_err(|_| "the URL is invalid")?;
    if !matches!(url.scheme(), "postgres" | "postgresql") {
        return Err("the URL scheme must be postgres or postgresql");
    }

    let encoded_path = url.path().strip_prefix('/').unwrap_or(url.path());
    let path_database_name = if encoded_path.is_empty() {
        None
    } else {
        Some(percent_decode_utf8(encoded_path).ok_or("the database name encoding is invalid")?)
    };
    let query_database_names = url
        .query_pairs()
        .filter(|(name, _)| name == "dbname")
        .map(|(_, value)| value.into_owned())
        .collect::<Vec<_>>();
    if query_database_names.len() > 1
        || (path_database_name.is_some() && !query_database_names.is_empty())
    {
        return Err("the database name must be specified exactly once");
    }

    let database_name = path_database_name
        .or_else(|| query_database_names.into_iter().next())
        .filter(|value| !value.is_empty())
        .ok_or("the database name is missing")?;
    if database_name.chars().any(char::is_control)
        || database_name.contains(char::REPLACEMENT_CHARACTER)
    {
        return Err("the database name is invalid");
    }

    Ok(database_name)
}

fn percent_decode_utf8(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).and_then(|byte| hex_value(*byte))?;
            let low = bytes.get(index + 2).and_then(|byte| hex_value(*byte))?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn has_standalone_test_segment(database_name: &str) -> bool {
    database_name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|segment| segment.eq_ignore_ascii_case("test"))
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_source(|name| match env::var(name) {
            Ok(value) => Ok(Some(value)),
            Err(env::VarError::NotPresent) => Ok(None),
            Err(env::VarError::NotUnicode(_)) => Err(ConfigError::new(format!(
                "{name} must contain valid Unicode"
            ))),
        })
    }

    fn from_source<F>(read: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Result<Option<String>, ConfigError>,
    {
        let environment = read("APP_ENV")?.unwrap_or_else(|| "development".to_owned());
        if !SUPPORTED_ENVIRONMENTS.contains(&environment.as_str()) {
            return Err(ConfigError::new(format!(
                "APP_ENV must be one of: {}",
                SUPPORTED_ENVIRONMENTS.join(", ")
            )));
        }

        let auth_token_ttl_seconds = match read("AUTH_TOKEN_TTL_SECONDS")? {
            Some(value) => value.parse::<i64>().map_err(|_| {
                ConfigError::new("AUTH_TOKEN_TTL_SECONDS must be a positive integer")
            })?,
            None => DEFAULT_AUTH_TOKEN_TTL_SECONDS,
        };
        if auth_token_ttl_seconds <= 0 {
            return Err(ConfigError::new(
                "AUTH_TOKEN_TTL_SECONDS must be greater than 0",
            ));
        }

        let auth_max_active_sessions_per_user = parse_bounded_positive_i64(
            "AUTH_MAX_ACTIVE_SESSIONS_PER_USER",
            read("AUTH_MAX_ACTIVE_SESSIONS_PER_USER")?,
            DEFAULT_AUTH_MAX_ACTIVE_SESSIONS_PER_USER,
            1,
            100,
        )?;

        let auth_cookie_secure = match read("AUTH_COOKIE_SECURE")? {
            Some(value) => parse_bool("AUTH_COOKIE_SECURE", &value)?,
            None => environment == "production",
        };
        if environment == "production" && !auth_cookie_secure {
            return Err(ConfigError::new(
                "AUTH_COOKIE_SECURE must be true in production",
            ));
        }

        let auth_login_max_failures = parse_positive_i32(
            "AUTH_LOGIN_MAX_FAILURES",
            read("AUTH_LOGIN_MAX_FAILURES")?,
            DEFAULT_AUTH_LOGIN_MAX_FAILURES,
        )?;
        let auth_login_account_max_failures = parse_positive_i32(
            "AUTH_LOGIN_ACCOUNT_MAX_FAILURES",
            read("AUTH_LOGIN_ACCOUNT_MAX_FAILURES")?,
            DEFAULT_AUTH_LOGIN_ACCOUNT_MAX_FAILURES,
        )?;
        if i64::from(auth_login_account_max_failures) < i64::from(auth_login_max_failures) * 2 {
            return Err(ConfigError::new(
                "AUTH_LOGIN_ACCOUNT_MAX_FAILURES must be at least twice AUTH_LOGIN_MAX_FAILURES",
            ));
        }
        let auth_login_window_seconds = parse_positive_i64(
            "AUTH_LOGIN_WINDOW_SECONDS",
            read("AUTH_LOGIN_WINDOW_SECONDS")?,
            DEFAULT_AUTH_LOGIN_WINDOW_SECONDS,
        )?;
        let auth_login_lockout_seconds = parse_positive_i64(
            "AUTH_LOGIN_LOCKOUT_SECONDS",
            read("AUTH_LOGIN_LOCKOUT_SECONDS")?,
            DEFAULT_AUTH_LOGIN_LOCKOUT_SECONDS,
        )?;
        let auth_password_login_enabled = match read("AUTH_PASSWORD_LOGIN_ENABLED")? {
            Some(value) => parse_bool("AUTH_PASSWORD_LOGIN_ENABLED", &value)?,
            None => true,
        };
        let entra_enabled = match read("AUTH_ENTRA_ENABLED")? {
            Some(value) => parse_bool("AUTH_ENTRA_ENABLED", &value)?,
            None => false,
        };
        let entra = if entra_enabled {
            let tenant_id =
                required_setting("AUTH_ENTRA_TENANT_ID", read("AUTH_ENTRA_TENANT_ID")?)?;
            let client_id =
                required_setting("AUTH_ENTRA_CLIENT_ID", read("AUTH_ENTRA_CLIENT_ID")?)?;
            let tenant_id = validate_uuid("AUTH_ENTRA_TENANT_ID", &tenant_id)?;
            let client_id = validate_uuid("AUTH_ENTRA_CLIENT_ID", &client_id)?;

            let redirect_uri =
                required_setting("AUTH_ENTRA_REDIRECT_URI", read("AUTH_ENTRA_REDIRECT_URI")?)?;
            if redirect_uri.len() > 2_048 {
                return Err(ConfigError::new(
                    "AUTH_ENTRA_REDIRECT_URI must be at most 2048 bytes",
                ));
            }
            validate_auth_url("AUTH_ENTRA_REDIRECT_URI", &redirect_uri, &environment, true)?;

            let transaction_key = required_setting(
                "AUTH_OIDC_TRANSACTION_KEY",
                read("AUTH_OIDC_TRANSACTION_KEY")?,
            )?;
            validate_auth_secret("AUTH_OIDC_TRANSACTION_KEY", &transaction_key)?;
            let client_secret = read("AUTH_ENTRA_CLIENT_SECRET")?
                .filter(|value| !value.trim().is_empty())
                .map(|value| value.trim().to_owned());
            if client_secret
                .as_ref()
                .is_some_and(|client_secret| client_secret.len() > 4_096)
            {
                return Err(ConfigError::new(
                    "AUTH_ENTRA_CLIENT_SECRET must be at most 4096 bytes",
                ));
            }
            if environment == "production" && client_secret.is_none() {
                return Err(ConfigError::new(
                    "production Entra login requires AUTH_ENTRA_CLIENT_SECRET",
                ));
            }

            let authority = format!("https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0");
            let issuer_default = format!("https://login.microsoftonline.com/{tenant_id}/v2.0");
            let issuer = optional_setting(read("AUTH_ENTRA_ISSUER")?).unwrap_or(issuer_default);
            let authorization_url = optional_setting(read("AUTH_ENTRA_AUTHORIZATION_URL")?)
                .unwrap_or_else(|| format!("{authority}/authorize"));
            let token_url = optional_setting(read("AUTH_ENTRA_TOKEN_URL")?)
                .unwrap_or_else(|| format!("{authority}/token"));
            let jwks_url = optional_setting(read("AUTH_ENTRA_JWKS_URL")?).unwrap_or_else(|| {
                format!("https://login.microsoftonline.com/{tenant_id}/discovery/v2.0/keys")
            });
            if issuer.len() > 255 {
                return Err(ConfigError::new(
                    "AUTH_ENTRA_ISSUER must be at most 255 bytes",
                ));
            }
            for (name, value) in [
                ("AUTH_ENTRA_ISSUER", issuer.as_str()),
                ("AUTH_ENTRA_AUTHORIZATION_URL", authorization_url.as_str()),
                ("AUTH_ENTRA_TOKEN_URL", token_url.as_str()),
                ("AUTH_ENTRA_JWKS_URL", jwks_url.as_str()),
            ] {
                if value.len() > 2_048 {
                    return Err(ConfigError::new(format!(
                        "{name} must be at most 2048 bytes"
                    )));
                }
                validate_auth_url(name, value, &environment, false)?;
            }

            let jit_provisioning = match read("AUTH_ENTRA_JIT_PROVISIONING")? {
                Some(value) => parse_bool("AUTH_ENTRA_JIT_PROVISIONING", &value)?,
                None => false,
            };
            let required_role = optional_setting(read("AUTH_ENTRA_REQUIRED_ROLE")?);
            if required_role
                .as_ref()
                .is_some_and(|role| role.len() > 128 || role.chars().any(char::is_control))
            {
                return Err(ConfigError::new(
                    "AUTH_ENTRA_REQUIRED_ROLE must be at most 128 bytes and contain no control characters",
                ));
            }
            if environment == "production" && jit_provisioning && required_role.is_none() {
                return Err(ConfigError::new(
                    "production Entra JIT provisioning requires AUTH_ENTRA_REQUIRED_ROLE",
                ));
            }
            let transaction_ttl_seconds = parse_bounded_positive_i64(
                "AUTH_OIDC_TRANSACTION_TTL_SECONDS",
                read("AUTH_OIDC_TRANSACTION_TTL_SECONDS")?,
                DEFAULT_AUTH_OIDC_TRANSACTION_TTL_SECONDS,
                60,
                1_800,
            )?;
            let jwks_cache_seconds = parse_bounded_positive_i64(
                "AUTH_ENTRA_JWKS_CACHE_SECONDS",
                read("AUTH_ENTRA_JWKS_CACHE_SECONDS")?,
                DEFAULT_AUTH_ENTRA_JWKS_CACHE_SECONDS,
                30,
                86_400,
            )?;
            let clock_skew_seconds = parse_bounded_positive_i64(
                "AUTH_ENTRA_CLOCK_SKEW_SECONDS",
                read("AUTH_ENTRA_CLOCK_SKEW_SECONDS")?,
                DEFAULT_AUTH_ENTRA_CLOCK_SKEW_SECONDS,
                1,
                300,
            )?;

            Some(EntraConfig {
                tenant_id,
                client_id,
                client_secret,
                transaction_key,
                redirect_uri,
                issuer,
                authorization_url,
                token_url,
                jwks_url,
                jit_provisioning,
                required_role,
                transaction_ttl_seconds,
                jwks_cache_seconds,
                clock_skew_seconds,
            })
        } else {
            None
        };

        if !auth_password_login_enabled && entra.is_none() {
            return Err(ConfigError::new(
                "at least one login method must be enabled",
            ));
        }

        if environment == "test" {
            if let Some(database_url) =
                read("DATABASE_URL")?.filter(|database_url| !database_url.trim().is_empty())
            {
                validate_test_database_url(&environment, &database_url, "DATABASE_URL")?;
            }
        }

        if environment == "production" {
            let database_url = read("DATABASE_URL")?;
            let rocket_databases = read("ROCKET_DATABASES")?;
            if !has_non_empty_value(database_url.as_deref())
                && !has_non_empty_value(rocket_databases.as_deref())
            {
                return Err(ConfigError::new(
                    "production requires DATABASE_URL or ROCKET_DATABASES",
                ));
            }

            let connector_secret_key = read("CONNECTOR_SECRET_KEY")?
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| ConfigError::new("production requires CONNECTOR_SECRET_KEY"))?;
            validate_connector_secret_key(&connector_secret_key)?;
        }

        Ok(Self {
            environment,
            auth_token_ttl_seconds,
            auth_max_active_sessions_per_user,
            auth_cookie_secure,
            auth_login_max_failures,
            auth_login_account_max_failures,
            auth_login_window_seconds,
            auth_login_lockout_seconds,
            auth_password_login_enabled,
            entra,
        })
    }
}

fn parse_bool(name: &str, value: &str) -> Result<bool, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(ConfigError::new(format!(
            "{name} must be one of: true, false, 1, 0, yes, no"
        ))),
    }
}

fn parse_positive_i32(name: &str, value: Option<String>, default: i32) -> Result<i32, ConfigError> {
    match value {
        Some(value) => value
            .parse::<i32>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| ConfigError::new(format!("{name} must be a positive integer"))),
        None => Ok(default),
    }
}

fn parse_positive_i64(name: &str, value: Option<String>, default: i64) -> Result<i64, ConfigError> {
    match value {
        Some(value) => value
            .parse::<i64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| ConfigError::new(format!("{name} must be a positive integer"))),
        None => Ok(default),
    }
}

fn parse_bounded_positive_i64(
    name: &str,
    value: Option<String>,
    default: i64,
    minimum: i64,
    maximum: i64,
) -> Result<i64, ConfigError> {
    let value = parse_positive_i64(name, value, default)?;
    if !(minimum..=maximum).contains(&value) {
        return Err(ConfigError::new(format!(
            "{name} must be between {minimum} and {maximum}"
        )));
    }

    Ok(value)
}

fn required_setting(name: &str, value: Option<String>) -> Result<String, ConfigError> {
    optional_setting(value).ok_or_else(|| ConfigError::new(format!("{name} is required")))
}

fn optional_setting(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn validate_uuid(name: &str, value: &str) -> Result<String, ConfigError> {
    uuid::Uuid::parse_str(value)
        .map(|uuid| uuid.hyphenated().to_string())
        .map_err(|_| ConfigError::new(format!("{name} must be a UUID")))
}

fn validate_auth_url(
    name: &str,
    value: &str,
    environment: &str,
    redirect_uri: bool,
) -> Result<(), ConfigError> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| ConfigError::new(format!("{name} must be an absolute HTTP(S) URL")))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(ConfigError::new(format!(
            "{name} must be an absolute HTTP(S) URL"
        )));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ConfigError::new(format!(
            "{name} must not contain credentials"
        )));
    }
    if environment == "production" && url.scheme() != "https" {
        return Err(ConfigError::new(format!(
            "{name} must use HTTPS in production"
        )));
    }
    if url.query().is_some() || url.fragment().is_some() {
        let kind = if redirect_uri { "redirect URI" } else { "URL" };
        return Err(ConfigError::new(format!(
            "{name} {kind} must not contain a query or fragment"
        )));
    }
    if redirect_uri && url.path() != "/auth/entra/callback" {
        return Err(ConfigError::new(format!(
            "{name} path must be exactly /auth/entra/callback"
        )));
    }

    Ok(())
}

fn validate_auth_secret(name: &str, secret: &str) -> Result<(), ConfigError> {
    if secret.len() < MIN_AUTH_SECRET_KEY_BYTES {
        return Err(ConfigError::new(format!(
            "{name} must be at least {MIN_AUTH_SECRET_KEY_BYTES} bytes"
        )));
    }
    if secret.len() > 4_096 {
        return Err(ConfigError::new(format!(
            "{name} must be at most 4096 bytes"
        )));
    }

    let distinct = secret
        .as_bytes()
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let normalized = secret.to_ascii_lowercase();
    if distinct.len() < 8
        || ["change-me", "changeme", "replace-me", "replace_me"]
            .iter()
            .any(|placeholder| normalized.contains(placeholder))
    {
        return Err(ConfigError::new(format!(
            "{name} must be a non-placeholder, high-entropy value"
        )));
    }

    Ok(())
}

fn has_non_empty_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

fn validate_connector_secret_key(secret_key: &str) -> Result<(), ConfigError> {
    if secret_key.len() < MIN_CONNECTOR_SECRET_KEY_BYTES {
        return Err(ConfigError::new(format!(
            "CONNECTOR_SECRET_KEY must be at least {MIN_CONNECTOR_SECRET_KEY_BYTES} bytes in production"
        )));
    }

    let normalized = secret_key.to_ascii_lowercase();
    let is_placeholder = [
        "change-me",
        "changeme",
        "replace-me",
        "replace_me",
        "dev-connector-secret-key",
    ]
    .iter()
    .any(|placeholder| normalized.contains(placeholder));
    let distinct_bytes = secret_key
        .as_bytes()
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();

    if is_placeholder || distinct_bytes.len() < 8 {
        return Err(ConfigError::new(
            "CONNECTOR_SECRET_KEY must be a non-placeholder, high-entropy value in production",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn config_from(values: &[(&str, &str)]) -> Result<AppConfig, ConfigError> {
        let values = values
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect::<HashMap<_, _>>();

        AppConfig::from_source(|name| Ok(values.get(name).cloned()))
    }

    #[test]
    fn defaults_are_only_used_when_optional_values_are_absent() {
        let config = config_from(&[]).expect("development defaults should be valid");

        assert_eq!(config.environment, "development");
        assert_eq!(config.auth_token_ttl_seconds, 86_400);
        assert_eq!(config.auth_max_active_sessions_per_user, 20);
        assert!(!config.auth_cookie_secure);
        assert_eq!(config.auth_login_max_failures, 5);
        assert_eq!(config.auth_login_account_max_failures, 50);
        assert_eq!(config.auth_login_window_seconds, 900);
        assert_eq!(config.auth_login_lockout_seconds, 900);
        assert!(config.auth_password_login_enabled);
        assert!(config.entra.is_none());
    }

    #[test]
    fn rejects_unsupported_environment() {
        let error = config_from(&[("APP_ENV", "prod")]).expect_err("prod is not supported");

        assert_eq!(
            error.to_string(),
            "APP_ENV must be one of: development, test, production"
        );
    }

    #[test]
    fn rejects_malformed_or_non_positive_auth_token_ttl() {
        for value in ["not-a-number", "0", "-1"] {
            let error = config_from(&[("AUTH_TOKEN_TTL_SECONDS", value)])
                .expect_err("invalid TTL must not silently use the default");
            assert!(error.to_string().contains("AUTH_TOKEN_TTL_SECONDS"));
        }
    }

    #[test]
    fn rejects_invalid_session_security_settings() {
        for (name, value) in [
            ("AUTH_LOGIN_MAX_FAILURES", "0"),
            ("AUTH_LOGIN_ACCOUNT_MAX_FAILURES", "0"),
            ("AUTH_LOGIN_WINDOW_SECONDS", "not-a-number"),
            ("AUTH_LOGIN_LOCKOUT_SECONDS", "-1"),
            ("AUTH_COOKIE_SECURE", "sometimes"),
            ("AUTH_MAX_ACTIVE_SESSIONS_PER_USER", "0"),
            ("AUTH_MAX_ACTIVE_SESSIONS_PER_USER", "-1"),
            ("AUTH_MAX_ACTIVE_SESSIONS_PER_USER", "101"),
            ("AUTH_MAX_ACTIVE_SESSIONS_PER_USER", "not-a-number"),
        ] {
            let error = config_from(&[(name, value)])
                .expect_err("invalid auth security settings must fail fast");
            assert!(error.to_string().contains(name));
        }
    }

    #[test]
    fn rejects_account_login_threshold_that_is_too_close_to_the_client_threshold() {
        let error = config_from(&[
            ("AUTH_LOGIN_MAX_FAILURES", "5"),
            ("AUTH_LOGIN_ACCOUNT_MAX_FAILURES", "9"),
        ])
        .expect_err("the account-wide threshold must resist simple lockout attacks");

        assert_eq!(
            error.to_string(),
            "AUTH_LOGIN_ACCOUNT_MAX_FAILURES must be at least twice AUTH_LOGIN_MAX_FAILURES"
        );
    }

    #[test]
    fn accepts_bounded_active_session_limit() {
        for value in ["1", "20", "100"] {
            let config = config_from(&[("AUTH_MAX_ACTIVE_SESSIONS_PER_USER", value)])
                .expect("session limits inside the supported range should load");
            assert_eq!(
                config.auth_max_active_sessions_per_user,
                value.parse::<i64>().unwrap()
            );
        }
    }

    #[test]
    fn rejects_disabling_every_login_method() {
        let error = config_from(&[("AUTH_PASSWORD_LOGIN_ENABLED", "false")])
            .expect_err("startup must retain a login method");

        assert_eq!(
            error.to_string(),
            "at least one login method must be enabled"
        );
    }

    #[test]
    fn loads_a_tenant_specific_entra_configuration_without_exposing_secrets() {
        let config = config_from(&[
            ("AUTH_PASSWORD_LOGIN_ENABLED", "false"),
            ("AUTH_ENTRA_ENABLED", "true"),
            (
                "AUTH_ENTRA_TENANT_ID",
                "11111111-1111-4111-8111-111111111111",
            ),
            (
                "AUTH_ENTRA_CLIENT_ID",
                "22222222-2222-4222-8222-222222222222",
            ),
            (
                "AUTH_ENTRA_REDIRECT_URI",
                "http://127.0.0.1:8000/auth/entra/callback",
            ),
            (
                "AUTH_OIDC_TRANSACTION_KEY",
                "S7yN2vQ9kL4mX8pR1tW6cF3hJ5dB0zAa",
            ),
            ("AUTH_ENTRA_CLIENT_SECRET", "test-client-secret"),
            ("AUTH_ENTRA_JIT_PROVISIONING", "true"),
            ("AUTH_ENTRA_REQUIRED_ROLE", "Portal.Member"),
        ])
        .expect("valid Entra config should load");

        let entra = config.entra.expect("Entra should be enabled");
        assert_eq!(entra.required_role.as_deref(), Some("Portal.Member"));
        assert_eq!(entra.transaction_ttl_seconds, 600);
        let debug = format!("{entra:?}");
        assert!(!debug.contains("test-client-secret"));
        assert!(!debug.contains("S7yN2vQ9kL4mX8pR1tW6cF3hJ5dB0zAa"));
    }

    #[test]
    fn entra_redirect_uri_must_target_the_fixed_callback_route() {
        for redirect_uri in [
            "http://127.0.0.1:8000/auth/entra/callback/",
            "http://127.0.0.1:8000/wrong",
            "http://127.0.0.1:8000/portal/auth/entra/callback",
        ] {
            let error = config_from(&[
                ("AUTH_ENTRA_ENABLED", "true"),
                (
                    "AUTH_ENTRA_TENANT_ID",
                    "11111111-1111-4111-8111-111111111111",
                ),
                (
                    "AUTH_ENTRA_CLIENT_ID",
                    "22222222-2222-4222-8222-222222222222",
                ),
                ("AUTH_ENTRA_REDIRECT_URI", redirect_uri),
                (
                    "AUTH_OIDC_TRANSACTION_KEY",
                    "S7yN2vQ9kL4mX8pR1tW6cF3hJ5dB0zAa",
                ),
            ])
            .expect_err("the Entra redirect URI must target the fixed callback route");

            assert_eq!(
                error.to_string(),
                "AUTH_ENTRA_REDIRECT_URI path must be exactly /auth/entra/callback"
            );
        }
    }

    #[test]
    fn production_entra_requires_https_and_a_confidential_client_secret() {
        let base = [
            ("APP_ENV", "production"),
            ("DATABASE_URL", "postgres://portal@postgres/portal"),
            ("CONNECTOR_SECRET_KEY", "X8JvY7gRZ3fU4nQ9cM2kL6sW1pT5dH0a"),
            ("AUTH_ENTRA_ENABLED", "true"),
            (
                "AUTH_ENTRA_TENANT_ID",
                "11111111-1111-4111-8111-111111111111",
            ),
            (
                "AUTH_ENTRA_CLIENT_ID",
                "22222222-2222-4222-8222-222222222222",
            ),
            (
                "AUTH_OIDC_TRANSACTION_KEY",
                "S7yN2vQ9kL4mX8pR1tW6cF3hJ5dB0zAa",
            ),
        ];
        let mut insecure = base.to_vec();
        insecure.push((
            "AUTH_ENTRA_REDIRECT_URI",
            "http://portal.example.test/auth/entra/callback",
        ));
        let error = config_from(&insecure).expect_err("production callback must use HTTPS");
        assert!(error.to_string().contains("AUTH_ENTRA_REDIRECT_URI"));

        let mut missing_secret = base.to_vec();
        missing_secret.push((
            "AUTH_ENTRA_REDIRECT_URI",
            "https://portal.example.test/auth/entra/callback",
        ));
        let error = config_from(&missing_secret)
            .expect_err("production Entra must authenticate its token exchange");
        assert!(error.to_string().contains("AUTH_ENTRA_CLIENT_SECRET"));
    }

    #[test]
    fn production_requires_secure_session_cookies() {
        let error = config_from(&[
            ("APP_ENV", "production"),
            ("DATABASE_URL", "postgres://portal@postgres/portal"),
            ("CONNECTOR_SECRET_KEY", "X8JvY7gRZ3fU4nQ9cM2kL6sW1pT5dH0a"),
            ("AUTH_COOKIE_SECURE", "false"),
        ])
        .expect_err("production cookies must be Secure");

        assert_eq!(
            error.to_string(),
            "AUTH_COOKIE_SECURE must be true in production"
        );
    }

    #[test]
    fn production_requires_database_configuration() {
        let error = config_from(&[
            ("APP_ENV", "production"),
            ("CONNECTOR_SECRET_KEY", "X8JvY7gRZ3fU4nQ9cM2kL6sW1pT5dH0a"),
        ])
        .expect_err("production must have a database configuration");

        assert_eq!(
            error.to_string(),
            "production requires DATABASE_URL or ROCKET_DATABASES"
        );
    }

    #[test]
    fn production_rejects_weak_or_placeholder_connector_keys() {
        for value in [
            "too-short",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "change-me-to-a-stable-high-entropy-secret",
        ] {
            let error = config_from(&[
                ("APP_ENV", "production"),
                ("DATABASE_URL", "postgres://portal@postgres/portal"),
                ("CONNECTOR_SECRET_KEY", value),
            ])
            .expect_err("weak production keys must be rejected");
            assert!(error.to_string().contains("CONNECTOR_SECRET_KEY"));
        }
    }

    #[test]
    fn production_accepts_rocket_database_config_and_a_strong_key() {
        let config = config_from(&[
            ("APP_ENV", "production"),
            (
                "ROCKET_DATABASES",
                r#"{postgres={url="postgres://portal@postgres/portal"}}"#,
            ),
            ("CONNECTOR_SECRET_KEY", "X8JvY7gRZ3fU4nQ9cM2kL6sW1pT5dH0a"),
        ])
        .expect("valid production config should load");

        assert_eq!(config.environment, "production");
    }

    #[test]
    fn test_database_urls_require_a_decoded_standalone_test_segment() {
        for database_url in [
            "postgres://portal@localhost/portal_integration_test",
            "postgresql://portal@localhost/portal-test-db",
            "postgres://portal@localhost/PORTAL%5FINTEGRATION%5FTEST",
            "postgres://portal@localhost/?dbname=portal_integration_test",
        ] {
            let target = validate_test_database_url("test", database_url, "DATABASE_URL")
                .expect("safe test URL should pass")
                .expect("test environment returns a target");
            assert!(has_standalone_test_segment(target.database_name()));
        }

        for database_url in [
            "postgres://portal@localhost/app_db",
            "postgres://portal@localhost/contest",
            "postgres://portal@localhost/portal_testdata",
            "postgres://portal@localhost/latest",
        ] {
            let error = validate_test_database_url("test", database_url, "DATABASE_URL")
                .expect_err("non-test database names must fail closed");
            assert!(error.to_string().contains("standalone 'test' segment"));
        }
    }

    #[test]
    fn test_database_url_errors_do_not_disclose_credentials() {
        let secret = "super-secret-password";
        let error = validate_test_database_url(
            "test",
            &format!("postgres://portal:{secret}@localhost/app_db"),
            "DATABASE_URL",
        )
        .expect_err("unsafe test URL must be rejected");

        assert!(!error.to_string().contains(secret));
        assert!(!error.to_string().contains("postgres://"));
    }

    #[test]
    fn test_database_url_rejects_ambiguous_or_missing_database_names() {
        for database_url in [
            "postgres://portal@localhost/",
            "postgres://portal@localhost/app_db?dbname=portal_test",
            "mysql://portal@localhost/portal_test",
            "not a URL",
        ] {
            validate_test_database_url("test", database_url, "DATABASE_URL")
                .expect_err("ambiguous or malformed test URLs must fail closed");
        }
    }

    #[test]
    fn non_test_environments_retain_existing_database_url_handling() {
        for environment in ["development", "production"] {
            assert_eq!(
                validate_test_database_url(environment, "not a URL", "DATABASE_URL")
                    .expect("non-test URL validation should be unchanged"),
                None
            );
        }
    }

    #[test]
    fn app_config_rejects_an_unsafe_configured_test_database() {
        let error = config_from(&[
            ("APP_ENV", "test"),
            ("DATABASE_URL", "postgres://portal@localhost/app_db"),
        ])
        .expect_err("APP_ENV=test must reject a configured development database");

        assert!(error.to_string().contains("DATABASE_URL"));
        config_from(&[
            ("APP_ENV", "test"),
            (
                "DATABASE_URL",
                "postgres://portal@localhost/portal_integration_test",
            ),
        ])
        .expect("a dedicated test database should be accepted");
    }

    #[test]
    fn actual_database_name_must_match_the_validated_target() {
        let target = validate_test_database_url(
            "test",
            "postgres://portal@localhost/portal_integration_test",
            "DATABASE_URL",
        )
        .unwrap()
        .unwrap();

        validate_actual_test_database_name(&target, "portal_integration_test", "DATABASE_URL")
            .expect("matching safe database should pass");
        validate_actual_test_database_name(&target, "app_db", "DATABASE_URL")
            .expect_err("an unexpected actual database must fail closed");
        validate_actual_test_database_name(&target, "portal_other_test", "DATABASE_URL")
            .expect_err("a different test database must also fail closed");
    }
}
