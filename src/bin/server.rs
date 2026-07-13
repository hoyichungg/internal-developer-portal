extern crate internal_developer_portal;

use diesel_async::{AsyncConnection, AsyncPgConnection};
use internal_developer_portal::config::{
    validate_test_database_url, verify_test_database_connection,
};

#[rocket::main]
async fn main() {
    dotenvy::dotenv().ok();
    let app_config = match internal_developer_portal::config::AppConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("server configuration error: {error}");
            std::process::exit(78);
        }
    };
    let environment = app_config.environment.clone();
    let embedded_worker_enabled =
        std::env::var("CONNECTOR_EMBEDDED_WORKER_ENABLED").as_deref() == Ok("true");

    let rocket = match internal_developer_portal::server_app::try_build(app_config) {
        Ok(rocket) => rocket,
        Err(error) => {
            eprintln!("server configuration error: {error}");
            std::process::exit(78);
        }
    };
    let rocket = match rocket.ignite().await {
        Ok(rocket) => rocket,
        Err(error) => {
            eprintln!("server launch failed: {error}");
            std::process::exit(1);
        }
    };

    if embedded_worker_enabled {
        if let Err(error) = verify_embedded_worker_database(&environment).await {
            eprintln!("server configuration error: {error}");
            std::process::exit(78);
        }
        internal_developer_portal::rocket_routes::connectors::spawn_connector_background_worker();
    }

    if let Err(error) = rocket.launch().await {
        eprintln!("server launch failed: {error}");
        std::process::exit(1);
    }
}

async fn verify_embedded_worker_database(environment: &str) -> Result<(), String> {
    if environment != "test" {
        return Ok(());
    }

    let database_url = std::env::var("DATABASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "embedded connector worker requires DATABASE_URL when enabled".to_owned())?;
    let target = validate_test_database_url(environment, &database_url, "DATABASE_URL")
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "test database safety target is missing".to_owned())?;
    let mut connection = AsyncPgConnection::establish(&database_url)
        .await
        .map_err(|_| "cannot connect to the embedded worker test database".to_owned())?;
    verify_test_database_connection(&mut connection, &target, "DATABASE_URL")
        .await
        .map_err(|error| error.to_string())
}
