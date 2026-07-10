use std::env;
use std::fmt::{self, Display, Formatter};

const DEFAULT_AUTH_TOKEN_TTL_SECONDS: i64 = 86_400;
const MIN_CONNECTOR_SECRET_KEY_BYTES: usize = 32;
const SUPPORTED_ENVIRONMENTS: [&str; 3] = ["development", "test", "production"];

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub environment: String,
    pub auth_token_ttl_seconds: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    fn new(message: impl Into<String>) -> Self {
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
        })
    }
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
}
