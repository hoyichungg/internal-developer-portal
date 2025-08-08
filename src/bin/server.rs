extern crate rust_webServer;
use rocket_db_pools::Database;

#[rocket::main]
async fn main() {
    let _ = rocket::build()
        .mount(
            "/",
            rocket::routes![
                rust_webServer::rocket_routes::authorization::login,
                rust_webServer::rocket_routes::rustaceans::get_rustaceans,
                rust_webServer::rocket_routes::rustaceans::view_rustacean,
                rust_webServer::rocket_routes::rustaceans::create_rustacean,
                rust_webServer::rocket_routes::rustaceans::update_rustacean,
                rust_webServer::rocket_routes::rustaceans::delete_rustacean,
                rust_webServer::rocket_routes::crates::get_crates,
                rust_webServer::rocket_routes::crates::view_crate,
                rust_webServer::rocket_routes::crates::create_crate,
                rust_webServer::rocket_routes::crates::update_crate,
                rust_webServer::rocket_routes::crates::delete_crate,
            ],
        )
        .attach(rust_webServer::rocket_routes::DbConn::init())
        .launch()
        .await;
}
