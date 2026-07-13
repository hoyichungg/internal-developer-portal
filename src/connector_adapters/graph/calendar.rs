use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

use super::super::shared::{
    append_query_params, encode_url_component, field_bool, field_string, field_url,
    format_graph_datetime, normalize_naive_datetime, notification_external_id, person_display,
    require_url,
};
use super::super::ConnectorAdapterResult;
use super::oauth::graph_access_token;
use super::{fetch_graph_collection, graph_pagination_limits};

#[derive(Deserialize)]
struct MicrosoftGraphCalendarConfig {
    adapter: Option<String>,
    calendar_view_url: Option<String>,
    base_url: Option<String>,
    user_id: Option<String>,
    start_at: Option<String>,
    end_at: Option<String>,
    lookahead_hours: Option<i64>,
    top: Option<u64>,
    max_pages: Option<u64>,
    max_items: Option<u64>,
    timeout_seconds: Option<u64>,
}

pub(in crate::connector_adapters) async fn fetch_microsoft_graph_calendar_events(
    config_json: &str,
) -> Result<ConnectorAdapterResult, String> {
    let mut config_value = serde_json::from_str::<Value>(config_json)
        .map_err(|error| format!("microsoft_graph_calendar config is not valid JSON: {error}"))?;
    let config = serde_json::from_str::<MicrosoftGraphCalendarConfig>(config_json)
        .map_err(|error| format!("microsoft_graph_calendar config is not valid JSON: {error}"))?;

    if !matches!(
        config.adapter.as_deref(),
        Some("microsoft_graph_calendar" | "graph_calendar" | "outlook_calendar")
    ) {
        return Err(
            "microsoft_graph_calendar config must set adapter to microsoft_graph_calendar"
                .to_owned(),
        );
    }

    let calendar_view_url = microsoft_graph_calendar_view_url(&config);
    require_url("calendar_view_url", &calendar_view_url)?;
    let (max_pages, max_items) = graph_pagination_limits(
        "microsoft_graph_calendar",
        config.max_pages,
        config.max_items,
    )?;

    let (start_at, end_at) = graph_calendar_time_window(&config);
    let top = config.top.unwrap_or(25).clamp(1, 50).to_string();
    let request_url = append_query_params(
        &calendar_view_url,
        &[
            ("startDateTime", start_at),
            ("endDateTime", end_at),
            (
                "$select",
                "id,subject,bodyPreview,importance,isAllDay,isCancelled,showAs,webLink,organizer,location,start,end,originalStartTimeZone,originalEndTimeZone,onlineMeetingUrl,onlineMeeting"
                    .to_owned(),
            ),
            ("$orderby", "start/dateTime".to_owned()),
            ("$top", top),
        ],
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(
            config.timeout_seconds.unwrap_or(15).max(1),
        ))
        .build()
        .map_err(|error| {
            format!("microsoft_graph_calendar HTTP client could not be built: {error}")
        })?;
    let access_token = graph_access_token(
        &client,
        &mut config_value,
        "microsoft_graph_calendar",
        "https://graph.microsoft.com/Calendars.Read offline_access",
    )
    .await?;
    let collection = fetch_graph_collection(
        &client,
        &request_url,
        &access_token.token,
        None,
        "microsoft_graph_calendar",
        max_pages,
        max_items,
    )
    .await?;
    let snapshot_complete = collection.snapshot_complete;
    let items = collection
        .items
        .into_iter()
        .map(|item| normalize_graph_calendar_event(&item))
        .collect::<Vec<_>>();

    Ok(ConnectorAdapterResult {
        payload: Some(json!({
            "items": items,
            "snapshot_complete": snapshot_complete
        })),
        updated_config: access_token.updated_config,
    })
}

fn normalize_graph_calendar_event(item: &Value) -> Value {
    let subject =
        field_string(item, &["subject", "title"]).unwrap_or_else(|| "Calendar event".to_owned());
    let external_id = notification_external_id(
        "calendar",
        item,
        &[
            "external_id",
            "id",
            "event_id",
            "uid",
            "iCalUId",
            "ical_uid",
        ],
        &subject,
    );
    let title = if subject.to_ascii_lowercase().starts_with("calendar:") {
        subject
    } else {
        format!("Calendar: {subject}")
    };

    json!({
        "external_id": external_id,
        "title": title,
        "body": graph_calendar_body(item),
        "severity": graph_calendar_severity(item),
        "is_read": false,
        "url": graph_calendar_url(item),
        "organizer": person_display(item, &["organizer"]),
        "location": graph_event_location(item),
        "starts_at": graph_event_datetime_value(item, "start"),
        "ends_at": graph_event_datetime_value(item, "end"),
        "time_zone": graph_event_time_zone(item),
        "is_all_day": field_bool(item, &["isAllDay", "is_all_day"]).unwrap_or(false),
        "is_cancelled": field_bool(item, &["isCancelled", "is_cancelled"]).unwrap_or(false),
        "web_url": graph_calendar_web_url(item),
        "join_url": graph_calendar_join_url(item)
    })
}

fn graph_calendar_body(item: &Value) -> Option<String> {
    let mut details = Vec::new();

    if let Some(organizer) = person_display(item, &["organizer"]) {
        details.push(format!("Organizer: {organizer}"));
    }
    if let Some(location) = graph_event_location(item) {
        details.push(format!("Location: {location}"));
    }
    if let Some(starts_at) = graph_event_datetime(item, "start") {
        details.push(format!("Starts: {starts_at}"));
    }
    if let Some(ends_at) = graph_event_datetime(item, "end") {
        details.push(format!("Ends: {ends_at}"));
    }
    if let Some(preview) = field_string(item, &["bodyPreview", "body_preview", "preview"]) {
        details.push(format!("Preview: {preview}"));
    }

    (!details.is_empty()).then(|| details.join(" | "))
}

fn graph_event_location(item: &Value) -> Option<String> {
    item.get("location")
        .and_then(|location| {
            field_string(
                location,
                &[
                    "displayName",
                    "display_name",
                    "locationUri",
                    "location_uri",
                    "uniqueId",
                    "unique_id",
                ],
            )
        })
        .or_else(|| field_string(item, &["location", "room"]))
}

fn graph_event_datetime(item: &Value, field_name: &str) -> Option<String> {
    let value = item.get(field_name)?;
    let datetime = field_string(value, &["dateTime", "date_time"])?;
    let datetime = normalize_naive_datetime(&datetime).unwrap_or(datetime);

    match graph_event_time_zone(item) {
        Some(time_zone) => Some(format!("{datetime} {time_zone}")),
        None => Some(datetime),
    }
}

fn graph_event_datetime_value(item: &Value, field_name: &str) -> Option<String> {
    let datetime = item
        .get(field_name)
        .and_then(|value| field_string(value, &["dateTime", "date_time"]))?;
    normalize_naive_datetime(&datetime)
}

fn graph_event_time_zone(item: &Value) -> Option<String> {
    field_string(
        item,
        &[
            "originalStartTimeZone",
            "original_start_time_zone",
            "originalEndTimeZone",
            "original_end_time_zone",
        ],
    )
    .or_else(|| {
        ["start", "end"].into_iter().find_map(|field_name| {
            item.get(field_name)
                .and_then(|value| field_string(value, &["timeZone", "time_zone"]))
        })
    })
}

fn graph_calendar_web_url(item: &Value) -> Option<String> {
    field_url(item, &["webLink", "web_link", "web_url", "url"])
}

fn graph_calendar_join_url(item: &Value) -> Option<String> {
    field_url(
        item,
        &["onlineMeetingUrl", "online_meeting_url", "join_url"],
    )
    .or_else(|| {
        item.pointer("/onlineMeeting/joinUrl")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
            .map(ToOwned::to_owned)
    })
}

fn graph_calendar_url(item: &Value) -> Option<String> {
    graph_calendar_web_url(item).or_else(|| graph_calendar_join_url(item))
}

fn graph_calendar_severity(item: &Value) -> &'static str {
    if field_bool(item, &["isCancelled", "is_cancelled"]).unwrap_or(false) {
        return "warning";
    }

    let severity = field_string(item, &["severity", "importance", "priority"])
        .map(|value| value.trim().to_ascii_lowercase());
    match severity.as_deref() {
        Some("critical" | "urgent" | "blocker" | "error" | "failed" | "failure") => "critical",
        Some("warning" | "warn" | "high" | "medium") => "warning",
        _ => "info",
    }
}

fn microsoft_graph_calendar_view_url(config: &MicrosoftGraphCalendarConfig) -> String {
    if let Some(url) = config
        .calendar_view_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        return url.to_owned();
    }

    let base_url = config
        .base_url
        .as_deref()
        .unwrap_or("https://graph.microsoft.com/v1.0")
        .trim_end_matches('/');
    let user_id = config
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|user_id| !user_id.is_empty())
        .unwrap_or("me");

    if user_id.eq_ignore_ascii_case("me") {
        format!("{base_url}/me/calendarView")
    } else {
        format!(
            "{base_url}/users/{}/calendarView",
            encode_url_component(user_id)
        )
    }
}

fn graph_calendar_time_window(config: &MicrosoftGraphCalendarConfig) -> (String, String) {
    let now = Utc::now();
    let start_at = config
        .start_at
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format_graph_datetime(now));
    let end_at = config
        .end_at
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            let lookahead_hours = config.lookahead_hours.unwrap_or(24).clamp(1, 168);
            let start = DateTime::parse_from_rfc3339(&start_at)
                .map(|datetime| datetime.with_timezone(&Utc))
                .unwrap_or(now);

            format_graph_datetime(start + ChronoDuration::hours(lookahead_hours))
        });

    (start_at, end_at)
}

#[cfg(test)]
mod tests {
    use super::{fetch_microsoft_graph_calendar_events, normalize_graph_calendar_event};
    use crate::connector_adapters::shared::test_support::{MockHttpServer, MockResponse};
    use serde_json::json;

    #[rocket::async_test]
    async fn follows_graph_calendar_next_links_and_merges_pages() {
        let server = MockHttpServer::start(vec![
            MockResponse::json(
                r#"{"value":[{"id":"evt-1","subject":"First","originalStartTimeZone":"Taipei Standard Time","originalEndTimeZone":"Taipei Standard Time","start":{"dateTime":"2026-07-10T09:00:00","timeZone":"UTC"},"end":{"dateTime":"2026-07-10T09:30:00","timeZone":"UTC"},"organizer":{"emailAddress":{"name":"Taylor Lin"}},"location":{"displayName":"Teams"}}],"@odata.nextLink":"{{base_url}}/calendar?page=2"}"#,
            ),
            MockResponse::json(
                r#"{"value":[{"id":"evt-2","subject":"Second","start":{"dateTime":"2026-07-10T10:00:00","timeZone":"UTC"},"end":{"dateTime":"2026-07-10T10:30:00","timeZone":"UTC"}}]}"#,
            ),
        ]);
        let config = json!({
            "adapter": "microsoft_graph_calendar",
            "calendar_view_url": server.url("/calendar"),
            "access_token": "test-token",
            "start_at": "2026-07-10T00:00:00Z",
            "end_at": "2026-07-11T00:00:00Z",
            "time_zone": "Taipei Standard Time",
            "max_pages": 5,
            "max_items": 10
        });

        let result = fetch_microsoft_graph_calendar_events(&config.to_string())
            .await
            .expect("calendar pages should load");
        let payload = result.payload.expect("calendar payload");
        let items = payload["items"].as_array().expect("calendar items");

        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["external_id"], "evt-1");
        assert_eq!(items[0]["starts_at"], "2026-07-10T09:00:00Z");
        assert_eq!(items[0]["ends_at"], "2026-07-10T09:30:00Z");
        assert_eq!(items[0]["time_zone"], "Taipei Standard Time");
        assert_eq!(items[0]["organizer"], "Taylor Lin");
        assert_eq!(items[0]["location"], "Teams");
        assert_eq!(items[1]["external_id"], "evt-2");
        assert_eq!(payload["snapshot_complete"], true);
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        assert!(requests
            .iter()
            .all(|request| request.contains("authorization: Bearer test-token")));
        assert!(requests.first().is_some_and(|request| {
            request.contains("originalStartTimeZone") && request.contains("originalEndTimeZone")
        }));
        assert!(requests
            .iter()
            .all(|request| !request.to_ascii_lowercase().contains("\r\nprefer:")));
    }

    #[test]
    fn normalizes_offset_calendar_timestamps_to_utc_and_preserves_original_timezone() {
        let event = normalize_graph_calendar_event(&json!({
            "id": "evt-offset",
            "subject": "Offset meeting",
            "originalStartTimeZone": "Taipei Standard Time",
            "originalEndTimeZone": "Taipei Standard Time",
            "start": {
                "dateTime": "2026-07-10T17:00:00+08:00",
                "timeZone": "Taipei Standard Time"
            },
            "end": {
                "dateTime": "2026-07-10T17:30:00+08:00",
                "timeZone": "Taipei Standard Time"
            },
            "isAllDay": false,
            "isCancelled": false
        }));

        assert_eq!(event["starts_at"], "2026-07-10T09:00:00Z");
        assert_eq!(event["ends_at"], "2026-07-10T09:30:00Z");
        assert_eq!(event["time_zone"], "Taipei Standard Time");
        assert!(event["body"]
            .as_str()
            .is_some_and(|body| body.contains("2026-07-10T09:00:00Z Taipei Standard Time")));
    }
}
