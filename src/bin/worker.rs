extern crate internal_developer_portal;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    if let Err(error) =
        internal_developer_portal::rocket_routes::connectors::run_connector_worker_forever().await
    {
        eprintln!("worker configuration error: {error}");
        std::process::exit(78);
    }
}
