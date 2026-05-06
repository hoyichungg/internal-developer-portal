use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};

pub mod common;

#[test]
fn test_health_endpoint() {
    let client = Client::new();
    let response = client
        .get(format!("{}/health", common::APP_HOST))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().unwrap();
    assert_eq!(
        body,
        json!({
            "data": {
                "status": "ok",
                "service": "internal-developer-portal-api"
            }
        })
    );
}
