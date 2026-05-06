use std::env;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub environment: String,
    pub auth_token_ttl_seconds: i64,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            environment: env::var("APP_ENV").unwrap_or_else(|_| "development".to_owned()),
            auth_token_ttl_seconds: env::var("AUTH_TOKEN_TTL_SECONDS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(86_400),
        }
    }
}
