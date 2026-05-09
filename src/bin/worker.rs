extern crate rust_web_server;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    rust_web_server::rocket_routes::connectors::run_connector_worker_forever().await;
}
