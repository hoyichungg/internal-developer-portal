use rocket::fs::FileServer;
use rocket::{Build, Rocket};
use rocket_db_pools::Database;

use crate::{api, config::AppConfig, rocket_routes};

pub fn build(app_config: AppConfig) -> Rocket<Build> {
    rocket::build()
        .manage(app_config)
        .register(
            "/",
            rocket::catchers![
                api::bad_request,
                api::unauthorized,
                api::forbidden,
                api::not_found,
                api::unprocessable_entity,
                api::internal_server_error,
            ],
        )
        .mount(
            "/",
            rocket::routes![
                rocket_routes::authorization::login,
                rocket_routes::authorization::me,
                rocket_routes::authorization::me_overview,
                rocket_routes::authorization::logout,
                rocket_routes::audit_logs::get_audit_logs,
                rocket_routes::connectors::get_connectors,
                rocket_routes::connectors::get_connector_operations,
                rocket_routes::connectors::view_connector,
                rocket_routes::connectors::create_connector,
                rocket_routes::connectors::update_connector,
                rocket_routes::connectors::delete_connector,
                rocket_routes::connectors::get_connector_config,
                rocket_routes::connectors::upsert_connector_config,
                rocket_routes::connectors::get_connector_runs,
                rocket_routes::connectors::get_connector_run,
                rocket_routes::connectors::retry_connector_run,
                rocket_routes::connectors::run_connector,
                rocket_routes::connectors::import_notifications,
                rocket_routes::connectors::import_service_health,
                rocket_routes::connectors::import_work_cards,
                rocket_routes::dashboard::dashboard,
                rocket_routes::health::health,
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
                rocket_routes::work_cards::view_work_card,
                rocket_routes::work_cards::create_work_card,
                rocket_routes::work_cards::update_work_card,
                rocket_routes::work_cards::delete_work_card,
                rocket_routes::notifications::get_notifications,
                rocket_routes::notifications::view_notification,
                rocket_routes::notifications::create_notification,
                rocket_routes::notifications::update_notification,
                rocket_routes::notifications::delete_notification,
            ],
        )
        .mount("/", FileServer::from("frontend/dist"))
        .attach(rocket_routes::DbConn::init())
}
