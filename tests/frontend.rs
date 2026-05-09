use reqwest::{blocking::Client, StatusCode};

pub mod common;

#[test]
fn test_frontend_shell_is_served() {
    let client = Client::new();
    let response = client.get(common::APP_HOST).send().unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.text().unwrap();
    assert!(body.contains("Internal Developer Portal"));
    assert!(body.contains(r#"<div id="root">"#));
}
