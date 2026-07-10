use std::process::Command;

const STRONG_CONNECTOR_KEY: &str = "X8JvY7gRZ3fU4nQ9cM2kL6sW1pT5dH0a";

#[test]
fn server_rejects_invalid_application_environment() {
    let output = Command::new(env!("CARGO_BIN_EXE_server"))
        .env("APP_ENV", "prod")
        .output()
        .expect("server process should start");

    assert_eq!(output.status.code(), Some(78));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("APP_ENV must be one of: development, test, production"));
}

#[test]
fn server_returns_failure_when_rocket_cannot_launch() {
    let output = Command::new(env!("CARGO_BIN_EXE_server"))
        .env("APP_ENV", "development")
        .env("ROCKET_PORT", "not-a-port")
        .env("CONNECTOR_EMBEDDED_WORKER_ENABLED", "false")
        .output()
        .expect("server process should start");

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("server launch failed"));
}

#[test]
fn enabled_worker_rejects_missing_database_url() {
    let output = Command::new(env!("CARGO_BIN_EXE_worker"))
        .env("APP_ENV", "production")
        .env("AUTH_TOKEN_TTL_SECONDS", "86400")
        .env("CONNECTOR_SECRET_KEY", STRONG_CONNECTOR_KEY)
        .env("CONNECTOR_WORKER_ENABLED", "true")
        .env("DATABASE_URL", "")
        .env("ROCKET_DATABASES", "")
        .output()
        .expect("worker process should start");

    assert_eq!(output.status.code(), Some(78));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("production requires DATABASE_URL or ROCKET_DATABASES"));
}

#[test]
fn explicitly_disabled_worker_exits_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_worker"))
        .env("APP_ENV", "production")
        .env("CONNECTOR_WORKER_ENABLED", "false")
        .env("DATABASE_URL", "")
        .env("CONNECTOR_SECRET_KEY", "")
        .output()
        .expect("worker process should start");

    assert!(output.status.success());
}

#[test]
fn worker_rejects_an_invalid_enabled_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_worker"))
        .env("CONNECTOR_WORKER_ENABLED", "ture")
        .output()
        .expect("worker process should start");

    assert_eq!(output.status.code(), Some(78));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("CONNECTOR_WORKER_ENABLED must be one of")
    );
}

#[test]
fn worker_rejects_invalid_run_lease_configuration() {
    let output = Command::new(env!("CARGO_BIN_EXE_worker"))
        .env("APP_ENV", "development")
        .env("CONNECTOR_WORKER_ENABLED", "true")
        .env("DATABASE_URL", "postgres://unused/portal")
        .env("CONNECTOR_RUN_LEASE_SECONDS", "15")
        .env("CONNECTOR_RUN_LEASE_RENEW_INTERVAL_SECONDS", "15")
        .output()
        .expect("worker process should start");

    assert_eq!(output.status.code(), Some(78));
    assert!(String::from_utf8_lossy(&output.stderr).contains(
        "CONNECTOR_RUN_LEASE_RENEW_INTERVAL_SECONDS must be less than CONNECTOR_RUN_LEASE_SECONDS"
    ));
}
