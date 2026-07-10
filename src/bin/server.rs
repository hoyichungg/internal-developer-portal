extern crate internal_developer_portal;

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

    if std::env::var("CONNECTOR_EMBEDDED_WORKER_ENABLED").as_deref() == Ok("true") {
        internal_developer_portal::rocket_routes::connectors::spawn_connector_background_worker();
    }

    if let Err(error) = internal_developer_portal::server_app::build(app_config)
        .launch()
        .await
    {
        eprintln!("server launch failed: {error}");
        std::process::exit(1);
    }
}
