use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use super::shared::require_url;

const WORK_ITEMS_BATCH_SIZE: usize = 200;
const DEFAULT_MAX_ITEMS: u64 = 1_000;
const MAX_ALLOWED_ITEMS: u64 = 10_000;

#[derive(Deserialize)]
struct AzureDevOpsConfig {
    adapter: Option<String>,
    wiql_url: Option<String>,
    work_items_url: Option<String>,
    base_url: Option<String>,
    organization: Option<String>,
    project: Option<String>,
    api_version: Option<String>,
    personal_access_token: Option<String>,
    pat: Option<String>,
    wiql: Option<String>,
    web_url_base: Option<String>,
    max_items: Option<u64>,
    timeout_seconds: Option<u64>,
}

pub(super) async fn fetch_azure_devops_work_cards(config_json: &str) -> Result<Value, String> {
    let config = serde_json::from_str::<AzureDevOpsConfig>(config_json)
        .map_err(|error| format!("azure_devops config is not valid JSON: {error}"))?;

    if config.adapter.as_deref() != Some("azure_devops") {
        return Err("azure_devops config must set adapter to azure_devops".to_owned());
    }

    let wiql_url = config.wiql_url.clone().unwrap_or_else(|| {
        azure_devops_url(
            config
                .base_url
                .as_deref()
                .unwrap_or("https://dev.azure.com"),
            config.organization.as_deref().unwrap_or_default(),
            config.project.as_deref().unwrap_or_default(),
            "_apis/wit/wiql",
            config.api_version.as_deref().unwrap_or("7.1"),
        )
    });
    let work_items_url = config.work_items_url.clone().unwrap_or_else(|| {
        azure_devops_url(
            config
                .base_url
                .as_deref()
                .unwrap_or("https://dev.azure.com"),
            config.organization.as_deref().unwrap_or_default(),
            config.project.as_deref().unwrap_or_default(),
            "_apis/wit/workitemsbatch",
            config.api_version.as_deref().unwrap_or("7.1"),
        )
    });

    require_url("wiql_url", &wiql_url)?;
    require_url("work_items_url", &work_items_url)?;
    let max_items = azure_max_items(config.max_items)?;
    let wiql_url = wiql_url_with_max_items(&wiql_url, max_items)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(
            config.timeout_seconds.unwrap_or(15).max(1),
        ))
        .build()
        .map_err(|error| format!("azure_devops HTTP client could not be built: {error}"))?;
    let wiql_query = config.wiql.unwrap_or_else(default_wiql);
    let wiql_response = send_azure_request(
        client.post(&wiql_url).json(&json!({ "query": wiql_query })),
        config
            .personal_access_token
            .as_deref()
            .or(config.pat.as_deref()),
    )
    .await
    .map_err(|error| format!("azure_devops WIQL request failed: {error}"))?;

    let work_items = wiql_response
        .get("workItems")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    // Azure WIQL has no portable continuation contract. A response that fills the
    // configured `$top` may be truncated, so it must never drive reconciliation.
    let snapshot_complete = work_items.len() < max_items;
    let mut seen_ids = HashSet::new();
    let ids = work_items
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_i64))
        .filter(|id| seen_ids.insert(*id))
        .take(max_items)
        .collect::<Vec<_>>();

    if ids.is_empty() {
        return Ok(json!({
            "items": [],
            "snapshot_complete": snapshot_complete
        }));
    }

    let token = config
        .personal_access_token
        .as_deref()
        .or(config.pat.as_deref());
    let batch_count = ids.len().div_ceil(WORK_ITEMS_BATCH_SIZE);
    let mut items = Vec::with_capacity(ids.len());

    for (batch_index, batch_ids) in ids.chunks(WORK_ITEMS_BATCH_SIZE).enumerate() {
        let work_items_response = send_azure_request(
            client.post(&work_items_url).json(&json!({
                "ids": batch_ids,
                "fields": [
                    "System.Id",
                    "System.Title",
                    "System.State",
                    "System.AssignedTo",
                    "Microsoft.VSTS.Common.Priority"
                ]
            })),
            token,
        )
        .await
        .map_err(|error| {
            format!(
                "azure_devops work item batch {}/{} failed: {error}",
                batch_index + 1,
                batch_count
            )
        })?;

        let mut response_items = work_items_response
            .get("value")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("id").and_then(Value::as_i64).map(|id| (id, item)))
            .map(|(id, item)| (id, item.clone()))
            .collect::<HashMap<_, _>>();

        for id in batch_ids {
            if let Some(item) = response_items.remove(id) {
                items.push(normalize_azure_work_item(
                    &item,
                    config.web_url_base.as_deref(),
                ));
            }
        }
    }

    Ok(json!({
        "items": items,
        "snapshot_complete": snapshot_complete
    }))
}

fn azure_max_items(value: Option<u64>) -> Result<usize, String> {
    let value = value.unwrap_or(DEFAULT_MAX_ITEMS);
    if !(1..=MAX_ALLOWED_ITEMS).contains(&value) {
        return Err(format!(
            "azure_devops config max_items must be an integer from 1 to {MAX_ALLOWED_ITEMS}"
        ));
    }

    Ok(value as usize)
}

fn wiql_url_with_max_items(url: &str, max_items: usize) -> Result<String, String> {
    let mut url = reqwest::Url::parse(url)
        .map_err(|error| format!("azure_devops wiql_url is invalid: {error}"))?;
    let existing_pairs = url
        .query_pairs()
        .filter(|(key, _)| key != "$top")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();

    url.set_query(None);
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in existing_pairs {
            pairs.append_pair(&key, &value);
        }
        pairs.append_pair("$top", &max_items.to_string());
    }

    Ok(url.to_string())
}

async fn send_azure_request(
    request: reqwest::RequestBuilder,
    token: Option<&str>,
) -> Result<Value, String> {
    let request = match token {
        Some(token) if !token.trim().is_empty() => request.basic_auth("", Some(token)),
        _ => request,
    };
    let response = request
        .send()
        .await
        .map_err(|error| format!("azure_devops request failed: {error}"))?;
    let status = response.status();

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("azure_devops request returned {status}: {body}"));
    }

    response
        .json::<Value>()
        .await
        .map_err(|error| format!("azure_devops response was not valid JSON: {error}"))
}

fn normalize_azure_work_item(item: &Value, web_url_base: Option<&str>) -> Value {
    let id = item.get("id").and_then(Value::as_i64).unwrap_or_default();
    let fields = item.get("fields").unwrap_or(&Value::Null);
    let title = fields
        .get("System.Title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled work item");
    let state = fields
        .get("System.State")
        .and_then(Value::as_str)
        .unwrap_or("New");
    let priority = fields
        .get("Microsoft.VSTS.Common.Priority")
        .and_then(Value::as_i64);
    let assignee = fields
        .get("System.AssignedTo")
        .and_then(assignee_display_name);
    let url = item
        .pointer("/_links/html/href")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| web_url_base.map(|base| format!("{}/{}", base.trim_end_matches('/'), id)));

    json!({
        "external_id": id.to_string(),
        "title": title,
        "status": normalize_azure_state(state),
        "priority": normalize_azure_priority(priority),
        "assignee": assignee,
        "due_at": null,
        "url": url
    })
}

fn assignee_display_name(value: &Value) -> Option<String> {
    value
        .get("displayName")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| value.as_str().map(ToOwned::to_owned))
}

fn normalize_azure_state(state: &str) -> &'static str {
    match state.to_ascii_lowercase().as_str() {
        "done" | "closed" | "removed" => "done",
        "blocked" => "blocked",
        "active" | "committed" | "in progress" | "resolved" => "in_progress",
        _ => "todo",
    }
}

fn normalize_azure_priority(priority: Option<i64>) -> &'static str {
    match priority {
        Some(1) => "urgent",
        Some(2) => "high",
        Some(3) => "medium",
        _ => "low",
    }
}

fn azure_devops_url(
    base_url: &str,
    organization: &str,
    project: &str,
    path: &str,
    api_version: &str,
) -> String {
    let base = base_url.trim_end_matches('/');
    let path = path.trim_start_matches('/');

    format!("{base}/{organization}/{project}/{path}?api-version={api_version}")
}

fn default_wiql() -> String {
    "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project ORDER BY [System.ChangedDate] DESC"
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::fetch_azure_devops_work_cards;
    use crate::connector_adapters::shared::test_support::{MockHttpServer, MockResponse};
    use serde_json::{json, Value};

    #[rocket::async_test]
    async fn batches_more_than_two_hundred_ids_and_preserves_wiql_order() {
        let ordered_ids = (1_i64..=450).collect::<Vec<_>>();
        let mut wiql_ids = vec![1_i64];
        wiql_ids.extend(ordered_ids.iter().copied());
        wiql_ids.push(451);

        let mut responses = vec![MockResponse::json(
            json!({
                "workItems": wiql_ids
                    .iter()
                    .map(|id| json!({ "id": id }))
                    .collect::<Vec<_>>()
            })
            .to_string(),
        )];
        responses.extend(ordered_ids.chunks(200).map(|batch| {
            let items = batch
                .iter()
                .rev()
                .map(|id| azure_item(*id))
                .collect::<Vec<_>>();
            MockResponse::json(json!({ "value": items }).to_string())
        }));
        let server = MockHttpServer::start(responses);
        let config = json!({
            "adapter": "azure_devops",
            "wiql_url": server.url("/wiql?api-version=7.1&$top=9999"),
            "work_items_url": server.url("/workitemsbatch?api-version=7.1"),
            "wiql": "SELECT [System.Id] FROM WorkItems",
            "max_items": 450
        });

        let payload = fetch_azure_devops_work_cards(&config.to_string())
            .await
            .expect("all work item batches should load");
        let items = payload["items"].as_array().expect("work card items");

        assert_eq!(items.len(), 450);
        assert_eq!(payload["snapshot_complete"], false);
        let external_ids = items
            .iter()
            .map(|item| item["external_id"].as_str().expect("external id"))
            .collect::<Vec<_>>();
        let expected_ids = ordered_ids.iter().map(i64::to_string).collect::<Vec<_>>();
        assert_eq!(external_ids, expected_ids);

        let requests = server.requests();
        assert_eq!(requests.len(), 4);
        assert!(requests[0].contains("%24top=450"));
        let batch_ids = requests[1..]
            .iter()
            .map(|request| {
                request_json(request)["ids"]
                    .as_array()
                    .expect("batch ids")
                    .iter()
                    .map(|id| id.as_i64().expect("numeric id"))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(batch_ids[0], ordered_ids[0..200]);
        assert_eq!(batch_ids[1], ordered_ids[200..400]);
        assert_eq!(batch_ids[2], ordered_ids[400..450]);
    }

    #[rocket::async_test]
    async fn marks_a_wiql_result_complete_only_when_it_does_not_fill_the_limit() {
        let server = MockHttpServer::start(vec![
            MockResponse::json(r#"{"workItems":[{"id":7}]}"#),
            MockResponse::json(json!({ "value": [azure_item(7)] }).to_string()),
        ]);
        let config = json!({
            "adapter": "azure_devops",
            "wiql_url": server.url("/wiql"),
            "work_items_url": server.url("/workitemsbatch"),
            "max_items": 10
        });

        let payload = fetch_azure_devops_work_cards(&config.to_string())
            .await
            .expect("bounded WIQL result should load");

        assert_eq!(payload["snapshot_complete"], true);
        assert_eq!(payload["items"].as_array().map(Vec::len), Some(1));
    }

    #[rocket::async_test]
    async fn reports_the_failed_batch_without_returning_partial_data() {
        let ordered_ids = (1_i64..=401).collect::<Vec<_>>();
        let wiql_response = MockResponse::json(
            json!({
                "workItems": ordered_ids
                    .iter()
                    .map(|id| json!({ "id": id }))
                    .collect::<Vec<_>>()
            })
            .to_string(),
        );
        let first_batch = MockResponse::json(
            json!({
                "value": ordered_ids[0..200]
                    .iter()
                    .map(|id| azure_item(*id))
                    .collect::<Vec<_>>()
            })
            .to_string(),
        );
        let server = MockHttpServer::start(vec![
            wiql_response,
            first_batch,
            MockResponse::with_status(503, r#"{"message":"try again"}"#),
        ]);
        let config = json!({
            "adapter": "azure_devops",
            "wiql_url": server.url("/wiql"),
            "work_items_url": server.url("/workitemsbatch"),
            "max_items": 401
        });

        let error = fetch_azure_devops_work_cards(&config.to_string())
            .await
            .expect_err("a failed detail batch must fail the entire adapter run");

        assert!(
            error.contains("work item batch 2/3 failed"),
            "unexpected error: {error}"
        );
        assert!(error.contains("503 Service Unavailable"));
    }

    fn azure_item(id: i64) -> Value {
        json!({
            "id": id,
            "fields": {
                "System.Title": format!("Item {id}"),
                "System.State": "Active",
                "Microsoft.VSTS.Common.Priority": 2
            }
        })
    }

    fn request_json(request: &str) -> Value {
        let (_, body) = request
            .split_once("\r\n\r\n")
            .expect("HTTP request body separator");
        serde_json::from_str(body).expect("JSON request body")
    }
}
