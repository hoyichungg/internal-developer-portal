use chrono::{Duration, SecondsFormat, Utc};
use serde_json::{json, Value};

use super::erp::normalize_erp_message_notification;
use super::shared::{
    field_bool, field_string, field_url, normalize_notification_severity, normalized_time_field,
    notification_external_id, person_display,
};

#[derive(Clone, Copy)]
pub(super) enum SampleNotificationKind {
    Calendar,
    OutlookMail,
    ErpMessages,
}

pub(super) fn fetch_sample_notifications(
    config_json: &str,
    kind: SampleNotificationKind,
) -> Result<Value, String> {
    let config = serde_json::from_str::<Value>(config_json)
        .map_err(|error| format!("{} config is not valid JSON: {error}", kind.adapter_name()))?;
    let items = match sample_notification_items(&config, kind) {
        Some(items) => items
            .into_iter()
            .map(|item| normalize_sample_notification(kind, item))
            .collect(),
        None => default_sample_notifications(kind),
    };

    Ok(json!({
        "items": items,
        "snapshot_complete": true
    }))
}

impl SampleNotificationKind {
    fn adapter_name(self) -> &'static str {
        match self {
            SampleNotificationKind::Calendar => "calendar_sample",
            SampleNotificationKind::OutlookMail => "outlook_mail_sample",
            SampleNotificationKind::ErpMessages => "erp_messages_sample",
        }
    }

    fn item_keys(self) -> &'static [&'static str] {
        match self {
            SampleNotificationKind::Calendar => &["items", "events", "meetings"],
            SampleNotificationKind::OutlookMail => &["items", "messages", "mail"],
            SampleNotificationKind::ErpMessages => &["items", "messages", "private_messages"],
        }
    }
}

fn sample_notification_items(config: &Value, kind: SampleNotificationKind) -> Option<Vec<&Value>> {
    for key in kind.item_keys() {
        if let Some(items) = config.get(*key).and_then(Value::as_array) {
            return Some(items.iter().collect());
        }
    }

    None
}

fn normalize_sample_notification(kind: SampleNotificationKind, item: &Value) -> Value {
    match kind {
        SampleNotificationKind::Calendar => normalize_calendar_notification(item),
        SampleNotificationKind::OutlookMail => normalize_outlook_mail_notification(item),
        SampleNotificationKind::ErpMessages => normalize_erp_message_notification(item),
    }
}

fn normalize_calendar_notification(item: &Value) -> Value {
    let title = field_string(item, &["title", "subject", "summary", "name"])
        .unwrap_or_else(|| "Calendar event".to_owned());
    let external_id = notification_external_id(
        "calendar",
        item,
        &["external_id", "id", "event_id", "uid", "ical_uid"],
        &title,
    );
    let severity = field_string(item, &["severity", "importance", "priority"])
        .map(|value| normalize_notification_severity(&value, "info"))
        .unwrap_or("info");

    json!({
        "external_id": external_id,
        "title": title,
        "body": calendar_body(item),
        "severity": severity,
        "is_read": field_bool(item, &["is_read", "read", "seen"]).unwrap_or(false),
        "url": field_url(item, &["url", "web_url", "web_link", "webLink", "join_url", "online_meeting_url"]),
        "organizer": person_display(item, &["organizer", "organizer_name", "from"]),
        "location": field_string(item, &["location", "room"]),
        "starts_at": normalized_time_field(item, &["starts_at", "start_at", "start_time", "start"]),
        "ends_at": normalized_time_field(item, &["ends_at", "end_at", "end_time", "end"]),
        "time_zone": field_string(item, &["time_zone", "timeZone"]),
        "is_all_day": field_bool(item, &["is_all_day", "isAllDay"]).unwrap_or(false),
        "is_cancelled": field_bool(item, &["is_cancelled", "isCancelled"]).unwrap_or(false),
        "web_url": field_url(item, &["url", "web_url", "web_link", "webLink"]),
        "join_url": field_url(item, &["join_url", "online_meeting_url"])
    })
}

fn normalize_outlook_mail_notification(item: &Value) -> Value {
    let title = field_string(item, &["title", "subject"])
        .unwrap_or_else(|| "Outlook mail message".to_owned());
    let external_id = notification_external_id(
        "mail",
        item,
        &["external_id", "id", "message_id", "internet_message_id"],
        &title,
    );
    let severity = field_string(item, &["severity", "importance", "priority"])
        .map(|value| normalize_notification_severity(&value, "info"))
        .unwrap_or("info");

    json!({
        "external_id": external_id,
        "title": title,
        "body": mail_body(item),
        "severity": severity,
        "is_read": field_bool(item, &["is_read", "isRead", "read", "seen"]).unwrap_or(false),
        "url": field_url(item, &["url", "web_url", "web_link", "webLink"])
    })
}

fn default_sample_notifications(kind: SampleNotificationKind) -> Vec<Value> {
    match kind {
        SampleNotificationKind::Calendar => {
            let starts_at = Utc::now() + Duration::minutes(15);
            let ends_at = starts_at + Duration::minutes(30);
            vec![json!({
                "external_id": "calendar-platform-standup",
                "title": "Calendar: Platform standup",
                "body": "Organizer: Taylor Lin | Location: Teams",
                "severity": "info",
                "is_read": false,
                "url": "https://calendar.example.test/events/platform-standup",
                "organizer": "Taylor Lin",
                "location": "Teams",
                "starts_at": starts_at.to_rfc3339_opts(SecondsFormat::Secs, true),
                "ends_at": ends_at.to_rfc3339_opts(SecondsFormat::Secs, true),
                "time_zone": "UTC",
                "is_all_day": false,
                "is_cancelled": false,
                "web_url": "https://calendar.example.test/events/platform-standup",
                "join_url": "https://teams.example.test/platform-standup"
            })]
        }
        SampleNotificationKind::OutlookMail => vec![json!({
            "external_id": "mail-release-brief",
            "title": "Mail: Release brief ready for review",
            "body": "From: release-bot@example.test | API deploy window moved to 15:30.",
            "severity": "warning",
            "is_read": false,
            "url": "https://outlook.example.test/mail/release-brief"
        })],
        SampleNotificationKind::ErpMessages => vec![json!({
            "external_id": "erp-access-approval",
            "title": "ERP: Deployment access approval waiting",
            "body": "Sample ERP private message. Configure erp_private_messages for a real HTTP endpoint.",
            "severity": "warning",
            "is_read": false,
            "url": null
        })],
    }
}

fn calendar_body(item: &Value) -> Option<String> {
    let body = field_string(item, &["body", "description", "preview"]);
    if body.is_some() {
        return body;
    }

    let mut details = Vec::new();
    if let Some(organizer) = person_display(item, &["organizer", "organizer_name", "from"]) {
        details.push(format!("Organizer: {organizer}"));
    }
    if let Some(location) = field_string(item, &["location", "room"]) {
        details.push(format!("Location: {location}"));
    }
    if let Some(starts_at) =
        normalized_time_field(item, &["starts_at", "start_at", "start_time", "start"])
    {
        details.push(format!("Starts: {starts_at}"));
    }
    if let Some(ends_at) = normalized_time_field(item, &["ends_at", "end_at", "end_time", "end"]) {
        details.push(format!("Ends: {ends_at}"));
    }

    (!details.is_empty()).then(|| details.join(" | "))
}

fn mail_body(item: &Value) -> Option<String> {
    let preview = field_string(
        item,
        &["body", "body_preview", "bodyPreview", "preview", "summary"],
    );
    let sender = person_display(item, &["from", "sender"]);

    match (sender, preview) {
        (Some(sender), Some(preview)) => Some(format!("From: {sender} | {preview}")),
        (Some(sender), None) => Some(format!("From: {sender}")),
        (None, Some(preview)) => Some(preview),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{fetch_sample_notifications, SampleNotificationKind};
    use chrono::{DateTime, Duration, FixedOffset};
    use serde_json::Value;

    #[test]
    fn default_calendar_sample_uses_unambiguous_utc_instants() {
        let payload = fetch_sample_notifications("{}", SampleNotificationKind::Calendar)
            .expect("default calendar sample");
        let item = payload["items"]
            .as_array()
            .and_then(|items| items.first())
            .expect("calendar item");
        let starts_at = parse_timestamp(item, "starts_at");
        let ends_at = parse_timestamp(item, "ends_at");

        assert_eq!(starts_at.offset().local_minus_utc(), 0);
        assert_eq!(ends_at - starts_at, Duration::minutes(30));
        assert!(item["starts_at"]
            .as_str()
            .is_some_and(|value| value.ends_with('Z')));
    }

    fn parse_timestamp(item: &Value, field: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(item[field].as_str().expect("timestamp string"))
            .expect("RFC3339 timestamp")
    }
}
