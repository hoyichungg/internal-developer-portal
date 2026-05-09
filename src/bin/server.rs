extern crate rust_web_server;

#[rocket::main]
async fn main() {
    dotenvy::dotenv().ok();
    let app_config = rust_web_server::config::AppConfig::from_env();

    if std::env::var("CONNECTOR_EMBEDDED_WORKER_ENABLED").as_deref() == Ok("true") {
        rust_web_server::rocket_routes::connectors::spawn_connector_background_worker();
    }

    let _ = rust_web_server::server_app::build(app_config)
        .launch()
        .await;
}
