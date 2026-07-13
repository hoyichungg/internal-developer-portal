mod calendar;
mod mail;
mod oauth;

use std::collections::HashSet;

use serde_json::Value;

const DEFAULT_MAX_PAGES: u64 = 20;
const DEFAULT_MAX_ITEMS: u64 = 1_000;
const MAX_ALLOWED_PAGES: u64 = 100;
const MAX_ALLOWED_ITEMS: u64 = 10_000;

pub(super) use self::calendar::fetch_microsoft_graph_calendar_events;
pub(super) use self::mail::fetch_microsoft_graph_mail_messages;

#[derive(Debug)]
pub(super) struct GraphCollection {
    pub items: Vec<Value>,
    pub snapshot_complete: bool,
}

pub(super) fn graph_pagination_limits(
    adapter: &str,
    max_pages: Option<u64>,
    max_items: Option<u64>,
) -> Result<(usize, usize), String> {
    let max_pages = max_pages.unwrap_or(DEFAULT_MAX_PAGES);
    let max_items = max_items.unwrap_or(DEFAULT_MAX_ITEMS);

    if !(1..=MAX_ALLOWED_PAGES).contains(&max_pages) {
        return Err(format!(
            "{adapter} config max_pages must be an integer from 1 to {MAX_ALLOWED_PAGES}"
        ));
    }
    if !(1..=MAX_ALLOWED_ITEMS).contains(&max_items) {
        return Err(format!(
            "{adapter} config max_items must be an integer from 1 to {MAX_ALLOWED_ITEMS}"
        ));
    }

    Ok((max_pages as usize, max_items as usize))
}

pub(super) async fn fetch_graph_collection(
    client: &reqwest::Client,
    initial_url: &str,
    token: &str,
    prefer: Option<&str>,
    adapter: &str,
    max_pages: usize,
    max_items: usize,
) -> Result<GraphCollection, String> {
    let initial_url = reqwest::Url::parse(initial_url)
        .map_err(|error| format!("{adapter} request URL is invalid: {error}"))?;
    let mut current_url = initial_url.clone();
    let mut visited = HashSet::new();
    let mut items = Vec::new();

    for page_number in 1..=max_pages {
        let canonical_request_url = canonical_url(&current_url);
        if !visited.insert(canonical_request_url) {
            return Err(format!(
                "{adapter} pagination loop detected at {}",
                current_url.as_str()
            ));
        }

        let mut request = client.get(current_url.clone()).bearer_auth(token);
        if let Some(prefer) = prefer {
            request = request.header("Prefer", prefer);
        }

        let response = request
            .send()
            .await
            .map_err(|error| format!("{adapter} page {page_number} request failed: {error}"))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(format!(
                "{adapter} page {page_number} request returned {status}: {body}"
            ));
        }

        let response = serde_json::from_str::<Value>(&body).map_err(|error| {
            format!("{adapter} page {page_number} response was not valid JSON: {error}")
        })?;
        let page_items = graph_collection_items(&response, adapter, page_number)?;
        let remaining = max_items.saturating_sub(items.len());
        let page_was_truncated = page_items.len() > remaining;
        items.extend(page_items.into_iter().take(remaining));

        let next_url = graph_next_link(&response, &current_url, &initial_url, adapter)?;
        if let Some(next_url) = next_url.as_ref() {
            if visited.contains(&canonical_url(next_url)) {
                return Err(format!(
                    "{adapter} pagination loop detected at {}",
                    next_url.as_str()
                ));
            }
        }

        if page_was_truncated {
            return Ok(GraphCollection {
                items,
                snapshot_complete: false,
            });
        }

        let Some(next_url) = next_url else {
            return Ok(GraphCollection {
                items,
                snapshot_complete: true,
            });
        };
        if items.len() >= max_items || page_number >= max_pages {
            return Ok(GraphCollection {
                items,
                snapshot_complete: false,
            });
        }
        current_url = next_url;
    }

    Ok(GraphCollection {
        items,
        snapshot_complete: false,
    })
}

fn graph_collection_items(
    response: &Value,
    adapter: &str,
    page_number: usize,
) -> Result<Vec<Value>, String> {
    if let Some(items) = response.as_array() {
        return Ok(items.clone());
    }

    for field in ["value", "items", "events", "messages"] {
        if let Some(value) = response.get(field) {
            let items = value.as_array().ok_or_else(|| {
                format!("{adapter} page {page_number} response field {field} must be an array")
            })?;
            return Ok(items.clone());
        }
    }

    Err(format!(
        "{adapter} page {page_number} response must include an explicit collection array in value, items, events, or messages"
    ))
}

fn graph_next_link(
    response: &Value,
    current_url: &reqwest::Url,
    initial_url: &reqwest::Url,
    adapter: &str,
) -> Result<Option<reqwest::Url>, String> {
    let next_link = ["@odata.nextLink", "odata.nextLink", "nextLink"]
        .into_iter()
        .find_map(|field| response.get(field));
    let Some(next_link) = next_link else {
        return Ok(None);
    };
    if next_link.is_null() {
        return Ok(None);
    }
    let next_link = next_link
        .as_str()
        .map(str::trim)
        .ok_or_else(|| format!("{adapter} pagination nextLink must be a non-empty URL string"))?;
    if next_link.is_empty() {
        return Err(format!(
            "{adapter} pagination nextLink must be a non-empty URL string"
        ));
    }

    let mut next_url = current_url
        .join(next_link)
        .map_err(|error| format!("{adapter} pagination nextLink is not a valid URL: {error}"))?;
    next_url.set_fragment(None);

    if !same_origin(initial_url, &next_url) {
        return Err(format!(
            "{adapter} pagination nextLink origin {} does not match initial request origin {}",
            url_origin(&next_url),
            url_origin(initial_url)
        ));
    }

    Ok(Some(next_url))
}

fn canonical_url(url: &reqwest::Url) -> String {
    let mut url = url.clone();
    url.set_fragment(None);
    url.to_string()
}

fn same_origin(left: &reqwest::Url, right: &reqwest::Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn url_origin(url: &reqwest::Url) -> String {
    match url.port_or_known_default() {
        Some(port) => format!(
            "{}://{}:{port}",
            url.scheme(),
            url.host_str().unwrap_or("<missing-host>")
        ),
        None => format!(
            "{}://{}",
            url.scheme(),
            url.host_str().unwrap_or("<missing-host>")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::fetch_graph_collection;
    use crate::connector_adapters::shared::test_support::{MockHttpServer, MockResponse};

    #[rocket::async_test]
    async fn rejects_cross_origin_graph_next_link() {
        let server = MockHttpServer::start(vec![MockResponse::json(
            r#"{"value":[],"@odata.nextLink":"https://evil.example.test/page/2"}"#,
        )]);
        let client = reqwest::Client::new();

        let error = fetch_graph_collection(
            &client,
            &server.url("/page/1"),
            "test-token",
            None,
            "graph_test",
            10,
            100,
        )
        .await
        .expect_err("cross-origin nextLink must be rejected");

        assert!(
            error.contains("nextLink origin"),
            "unexpected error: {error}"
        );
        assert!(
            error.contains("does not match"),
            "unexpected error: {error}"
        );
    }

    #[rocket::async_test]
    async fn rejects_graph_next_link_loop() {
        let server = MockHttpServer::start(vec![MockResponse::json(
            r#"{"value":[],"@odata.nextLink":"{{base_url}}/loop"}"#,
        )]);
        let client = reqwest::Client::new();

        let error = fetch_graph_collection(
            &client,
            &server.url("/loop"),
            "test-token",
            None,
            "graph_test",
            10,
            100,
        )
        .await
        .expect_err("repeated nextLink must be rejected");

        assert!(
            error.contains("pagination loop detected"),
            "unexpected error: {error}"
        );
    }

    #[rocket::async_test]
    async fn max_pages_stops_before_requesting_another_page() {
        let server = MockHttpServer::start(vec![MockResponse::json(
            r#"{"value":[{"id":"one"}],"@odata.nextLink":"{{base_url}}/page/2"}"#,
        )]);
        let client = reqwest::Client::new();

        let collection = fetch_graph_collection(
            &client,
            &server.url("/page/1"),
            "test-token",
            None,
            "graph_test",
            1,
            100,
        )
        .await
        .expect("first page should be returned at the configured page limit");

        assert_eq!(collection.items.len(), 1);
        assert!(!collection.snapshot_complete);
        assert_eq!(server.requests().len(), 1);
    }

    #[rocket::async_test]
    async fn rejects_success_response_without_an_explicit_collection() {
        let server = MockHttpServer::start(vec![MockResponse::json(r#"{"status":"ok"}"#)]);
        let client = reqwest::Client::new();

        let error = fetch_graph_collection(
            &client,
            &server.url("/page/1"),
            "test-token",
            None,
            "graph_test",
            10,
            100,
        )
        .await
        .expect_err("an unknown 200 response shape must not become a complete empty snapshot");

        assert!(
            error.contains("must include an explicit collection array"),
            "unexpected error: {error}"
        );
        assert_eq!(server.requests().len(), 1);
    }

    #[rocket::async_test]
    async fn accepts_an_explicit_empty_collection_as_complete() {
        let server = MockHttpServer::start(vec![MockResponse::json(r#"{"value":[]}"#)]);
        let client = reqwest::Client::new();

        let collection = fetch_graph_collection(
            &client,
            &server.url("/page/1"),
            "test-token",
            None,
            "graph_test",
            10,
            100,
        )
        .await
        .expect("an explicit empty collection is a valid complete snapshot");

        assert!(collection.items.is_empty());
        assert!(collection.snapshot_complete);
        assert_eq!(server.requests().len(), 1);
    }
}
