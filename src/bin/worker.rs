extern crate internal_developer_portal;

use diesel_async::{AsyncConnection, AsyncPgConnection};
use internal_developer_portal::config::{
    validate_test_database_url, verify_test_database_connection, AppConfig,
};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    if let Err(error) = verify_test_worker_database().await {
        eprintln!("worker configuration error: {error}");
        std::process::exit(78);
    }
    if let Err(error) =
        internal_developer_portal::rocket_routes::connectors::run_connector_worker_forever().await
    {
        eprintln!("worker configuration error: {error}");
        std::process::exit(78);
    }
}

async fn verify_test_worker_database() -> Result<(), String> {
    if std::env::var("APP_ENV").as_deref() != Ok("test") {
        return Ok(());
    }
    if !worker_enabled()? {
        return Ok(());
    }

    let config = AppConfig::from_env().map_err(|error| error.to_string())?;
    let database_url = std::env::var("DATABASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "connector worker requires DATABASE_URL when enabled".to_owned())?;
    let target = validate_test_database_url(&config.environment, &database_url, "DATABASE_URL")
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "test database safety target is missing".to_owned())?;
    let mut connection = AsyncPgConnection::establish(&database_url)
        .await
        .map_err(|_| "cannot connect to the configured test database".to_owned())?;
    verify_test_database_connection(&mut connection, &target, "DATABASE_URL")
        .await
        .map_err(|error| error.to_string())
}

fn worker_enabled() -> Result<bool, String> {
    match std::env::var("CONNECTOR_WORKER_ENABLED") {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Ok(true),
            "0" | "false" | "no" => Ok(false),
            _ => Err(
                "CONNECTOR_WORKER_ENABLED must be one of: true, false, 1, 0, yes, no".to_owned(),
            ),
        },
        Err(std::env::VarError::NotPresent) => Ok(true),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err("CONNECTOR_WORKER_ENABLED must contain valid Unicode".to_owned())
        }
    }
}
