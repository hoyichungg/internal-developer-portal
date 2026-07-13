use rocket::fairing::AdHoc;
use rocket::figment::Figment;
use rocket::fs::FileServer;
use rocket::{Build, Rocket};
use rocket_db_pools::Database;
use std::env;

use crate::{
    api,
    config::{
        validate_test_database_url, verify_test_database_connection, AppConfig, ConfigError,
        TestDatabaseTarget,
    },
    rocket_routes,
};

const DEFAULT_LOCAL_DATABASE_URL: &str = "postgres://postgres:postgres@localhost:5432/app_db";

pub fn build(app_config: AppConfig) -> Rocket<Build> {
    try_build(app_config).unwrap_or_else(|error| panic!("server configuration error: {error}"))
}

pub fn try_build(app_config: AppConfig) -> Result<Rocket<Build>, ConfigError> {
    rocket_routes::authorization::initialize_dummy_password_hash();
    let figment = figment_with_database_url_fallback(&app_config);
    let test_database_target = effective_test_database_target(&app_config.environment, &figment)?;

    let rocket = rocket::custom(figment)
        .manage(app_config)
        .manage(rocket_routes::entra_auth::EntraOidcClient::new())
        .attach(rocket_routes::entra_auth::AuthSecurityHeaders)
        .register(
            "/",
            rocket::catchers![
                api::bad_request,
                api::unauthorized,
                api::too_many_requests,
                api::forbidden,
                api::not_found,
                api::unprocessable_entity,
                api::internal_server_error,
                api::service_unavailable,
            ],
        )
        .mount(
            "/",
            rocket::routes![
                rocket_routes::authorization::login,
                rocket_routes::authorization::me,
                rocket_routes::authorization::me_overview,
                rocket_routes::authorization::users,
                rocket_routes::authorization::logout,
                rocket_routes::authorization::revoke_all_sessions,
                rocket_routes::entra_auth::auth_config,
                rocket_routes::entra_auth::start_entra_login,
                rocket_routes::entra_auth::finish_entra_login,
                rocket_routes::calendar_events::get_calendar_events,
                rocket_routes::calendar_events::view_calendar_event,
                crate::openapi::openapi_json,
                rocket_routes::audit_logs::get_audit_logs,
                rocket_routes::connectors::get_connectors,
                rocket_routes::connectors::get_connector_operations,
                rocket_routes::connectors::view_connector,
                rocket_routes::connectors::create_connector,
                rocket_routes::connectors::update_connector,
                rocket_routes::connectors::update_connector_scope,
                rocket_routes::connectors::delete_connector,
                rocket_routes::connectors::get_connector_config,
                rocket_routes::connectors::upsert_connector_config,
                rocket_routes::connectors::start_microsoft_oauth,
                rocket_routes::connectors::finish_microsoft_oauth,
                rocket_routes::connectors::microsoft_oauth_callback_page,
                rocket_routes::connectors::get_connector_runs,
                rocket_routes::connectors::get_connector_run,
                rocket_routes::connectors::retry_connector_run,
                rocket_routes::connectors::cancel_connector_run,
                rocket_routes::connectors::run_connector,
                rocket_routes::connectors::import_calendar_events,
                rocket_routes::connectors::import_notifications,
                rocket_routes::connectors::import_service_health,
                rocket_routes::connectors::import_work_cards,
                rocket_routes::dashboard::dashboard,
                rocket_routes::health::health,
                rocket_routes::health::livez,
                rocket_routes::health::readyz,
                rocket_routes::maintainers::get_maintainers,
                rocket_routes::maintainers::view_maintainer,
                rocket_routes::maintainers::create_maintainer,
                rocket_routes::maintainers::update_maintainer,
                rocket_routes::maintainers::delete_maintainer,
                rocket_routes::maintainers::get_maintainer_members,
                rocket_routes::maintainers::upsert_maintainer_member,
                rocket_routes::maintainers::delete_maintainer_member,
                rocket_routes::services::get_services,
                rocket_routes::services::view_service,
                rocket_routes::services::service_overview,
                rocket_routes::services::create_service,
                rocket_routes::services::update_service,
                rocket_routes::services::delete_service,
                rocket_routes::packages::get_packages,
                rocket_routes::packages::view_package,
                rocket_routes::packages::create_package,
                rocket_routes::packages::update_package,
                rocket_routes::packages::delete_package,
                rocket_routes::work_cards::get_work_cards,
                rocket_routes::work_cards::get_my_work_cards,
                rocket_routes::work_cards::view_work_card,
                rocket_routes::work_cards::create_work_card,
                rocket_routes::work_cards::update_work_card,
                rocket_routes::work_cards::delete_work_card,
                rocket_routes::notifications::get_notifications,
                rocket_routes::notifications::view_notification,
                rocket_routes::notifications::mark_notification_read,
                rocket_routes::notifications::mark_notification_unread,
                rocket_routes::notifications::dismiss_notification,
                rocket_routes::notifications::snooze_notification,
                rocket_routes::notifications::restore_notification,
                rocket_routes::notifications::create_notification,
                rocket_routes::notifications::update_notification,
                rocket_routes::notifications::delete_notification,
            ],
        )
        .mount("/", FileServer::from("frontend/dist"))
        .attach(rocket_routes::DbConn::init());

    Ok(match test_database_target {
        Some(target) => rocket.manage(target).attach(AdHoc::try_on_ignite(
            "Verify test database identity",
            |rocket| async {
                let Some(target) = rocket.state::<TestDatabaseTarget>() else {
                    rocket::error!("test database safety target is missing");
                    return Err(rocket);
                };
                let Some(database) = rocket_routes::DbConn::fetch(&rocket) else {
                    rocket::error!("test database pool is unavailable during safety check");
                    return Err(rocket);
                };
                let mut connection = match database.get().await {
                    Ok(connection) => connection,
                    Err(_) => {
                        rocket::error!(
                            "test database connection is unavailable during safety check"
                        );
                        return Err(rocket);
                    }
                };

                match verify_test_database_connection(
                    &mut connection,
                    target,
                    "Rocket databases.postgres.url",
                )
                .await
                {
                    Ok(()) => Ok(rocket),
                    Err(error) => {
                        rocket::error!("test database safety check failed: {error}");
                        Err(rocket)
                    }
                }
            },
        )),
        None => rocket,
    })
}

fn effective_test_database_target(
    environment: &str,
    figment: &Figment,
) -> Result<Option<TestDatabaseTarget>, ConfigError> {
    if environment != "test" {
        return Ok(None);
    }

    let database_url = figment
        .extract_inner::<String>("databases.postgres.url")
        .map_err(|_| {
            ConfigError::new("test environment requires an effective Rocket databases.postgres.url")
        })?;
    validate_test_database_url(environment, &database_url, "Rocket databases.postgres.url")
}

fn figment_with_database_url_fallback(app_config: &AppConfig) -> Figment {
    let figment = if app_config.environment == "production" {
        // Rocket's normal request log includes the full callback query string.
        // Authorization codes are short-lived and PKCE-bound, but they still
        // must not be copied into production logs.
        rocket::Config::figment().merge(("log_level", "critical"))
    } else {
        rocket::Config::figment()
    };

    if env::var("ROCKET_DATABASES").is_ok_and(|value| !value.trim().is_empty()) {
        return figment;
    }

    match env::var("DATABASE_URL") {
        Ok(database_url) if !database_url.trim().is_empty() => {
            figment.merge(("databases.postgres.url", database_url))
        }
        _ if app_config.environment != "production" => {
            figment.merge(("databases.postgres.url", DEFAULT_LOCAL_DATABASE_URL))
        }
        _ => figment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_the_effective_rocket_database_url_in_test() {
        let safe = rocket::Config::figment().merge((
            "databases.postgres.url",
            "postgres://portal@localhost/portal_integration_test",
        ));
        let target = effective_test_database_target("test", &safe)
            .expect("safe effective Rocket database should pass")
            .expect("test environment returns a target");
        assert_eq!(target.database_name(), "portal_integration_test");

        let unsafe_figment = rocket::Config::figment().merge((
            "databases.postgres.url",
            "postgres://portal:do-not-log-this@localhost/app_db",
        ));
        let error = effective_test_database_target("test", &unsafe_figment)
            .expect_err("unsafe Rocket override must fail closed");
        assert!(error.to_string().contains("standalone 'test' segment"));
        assert!(!error.to_string().contains("do-not-log-this"));
    }

    #[test]
    fn leaves_non_test_rocket_database_handling_unchanged() {
        let figment =
            rocket::Config::figment().merge(("databases.postgres.url", "not a PostgreSQL URL"));

        assert_eq!(
            effective_test_database_target("development", &figment)
                .expect("development validation remains unchanged"),
            None
        );
        assert_eq!(
            effective_test_database_target("production", &figment)
                .expect("production validation remains unchanged"),
            None
        );
    }
}
