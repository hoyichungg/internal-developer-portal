extern crate rust_web_server;
use rocket_db_pools::Database;

#[rocket::main]
async fn main() {
    dotenvy::dotenv().ok();
    let app_config = rust_web_server::config::AppConfig::from_env();

    let _ = rocket::build()
        .manage(app_config)
        .register(
            "/",
            rocket::catchers![
                rust_web_server::api::bad_request,
                rust_web_server::api::unauthorized,
                rust_web_server::api::forbidden,
                rust_web_server::api::not_found,
                rust_web_server::api::unprocessable_entity,
                rust_web_server::api::internal_server_error,
            ],
        )
        .mount(
            "/",
            rocket::routes![
                rust_web_server::rocket_routes::authorization::login,
                rust_web_server::rocket_routes::authorization::me,
                rust_web_server::rocket_routes::authorization::logout,
                rust_web_server::rocket_routes::health::health,
                rust_web_server::rocket_routes::maintainers::get_maintainers,
                rust_web_server::rocket_routes::maintainers::view_maintainer,
                rust_web_server::rocket_routes::maintainers::create_maintainer,
                rust_web_server::rocket_routes::maintainers::update_maintainer,
                rust_web_server::rocket_routes::maintainers::delete_maintainer,
                rust_web_server::rocket_routes::packages::get_packages,
                rust_web_server::rocket_routes::packages::view_package,
                rust_web_server::rocket_routes::packages::create_package,
                rust_web_server::rocket_routes::packages::update_package,
                rust_web_server::rocket_routes::packages::delete_package,
            ],
        )
        .attach(rust_web_server::rocket_routes::DbConn::init())
        .launch()
        .await;
}
