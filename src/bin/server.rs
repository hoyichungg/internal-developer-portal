extern crate internal_developer_portal;

#[rocket::main]
async fn main() {
    dotenvy::dotenv().ok();
    let app_config = internal_developer_portal::config::AppConfig::from_env();

    if std::env::var("CONNECTOR_EMBEDDED_WORKER_ENABLED").as_deref() == Ok("true") {
        internal_developer_portal::rocket_routes::connectors::spawn_connector_background_worker();
    }

    let _ = internal_developer_portal::server_app::build(app_config)
        .launch()
        .await;
}
