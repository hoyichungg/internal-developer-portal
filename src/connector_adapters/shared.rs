use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::Value;
use std::fmt::Write as _;

pub(super) fn normalize_notification_severity(value: &str, default: &'static str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "critical" | "urgent" | "blocker" | "error" | "failed" | "failure" => "critical",
        "warning" | "warn" | "high" | "medium" | "normal" => "warning",
        "info" | "low" | "ok" | "success" | "none" => "info",
        _ => default,
    }
}

pub(super) fn normalize_lifecycle(status: &str) -> &'static str {
    match status.to_ascii_lowercase().as_str() {
        "deprecated" => "deprecated",
        "archived" | "inactive" | "retired" | "decommissioned" => "archived",
        _ => "active",
    }
}

pub(super) fn format_graph_datetime(datetime: DateTime<Utc>) -> String {
    datetime.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub(super) fn append_query_params(base_url: &str, params: &[(&str, String)]) -> String {
    let mut url = base_url.to_owned();
    let mut separator = if url.contains('?') {
        if url.ends_with('?') || url.ends_with('&') {
            ""
        } else {
            "&"
        }
    } else {
        "?"
    };

    for (key, value) in params {
        url.push_str(separator);
        url.push_str(&encode_url_component(key));
        url.push('=');
        url.push_str(&encode_url_component(value));
        separator = "&";
    }

    url
}

pub(super) fn encode_url_component(value: &str) -> String {
    let mut encoded = String::new();

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                let _ = write!(&mut encoded, "%{byte:02X}");
            }
        }
    }

    encoded
}

pub(super) fn require_url(field: &str, url: &str) -> Result<(), String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(())
    } else {
        Err(format!("{field} must be an absolute HTTP URL"))
    }
}

pub(super) fn notification_external_id(
    prefix: &str,
    item: &Value,
    id_fields: &[&str],
    title: &str,
) -> String {
    field_string(item, id_fields).unwrap_or_else(|| {
        let slug = stable_slug(None, &[title]);
        format!("{prefix}-{slug}")
    })
}

pub(super) fn field_url(item: &Value, names: &[&str]) -> Option<String> {
    field_string(item, names)
        .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
}

pub(super) fn field_string(item: &Value, names: &[&str]) -> Option<String> {
    field(item, names)
        .and_then(scalar_to_string)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

pub(super) fn field_i32(item: &Value, names: &[&str]) -> Option<i32> {
    field(item, names)
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

pub(super) fn field_bool(item: &Value, names: &[&str]) -> Option<bool> {
    field(item, names).and_then(|value| match value {
        Value::Bool(value) => Some(*value),
        Value::Number(value) => value.as_i64().map(|value| value != 0),
        Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    })
}

pub(super) fn person_display(item: &Value, names: &[&str]) -> Option<String> {
    field(item, names).and_then(|value| {
        scalar_to_string(value)
            .or_else(|| {
                field_string(
                    value,
                    &["display_name", "displayName", "name", "email", "address"],
                )
            })
            .or_else(|| {
                value
                    .get("emailAddress")
                    .and_then(|email| field_string(email, &["name", "address"]))
            })
    })
}

pub(super) fn normalized_time_field(item: &Value, names: &[&str]) -> Option<String> {
    field_string(item, names).map(|value| normalize_naive_datetime(&value).unwrap_or(value))
}

fn field<'a>(item: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| item.get(*name))
}

fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.to_owned()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }

    slug.trim_matches('-').to_owned()
}

pub(super) fn stable_slug(preferred: Option<&str>, fallbacks: &[&str]) -> String {
    preferred
        .into_iter()
        .chain(fallbacks.iter().copied())
        .map(slugify)
        .find(|slug| !slug.is_empty())
        .unwrap_or_else(|| {
            let bytes = fallbacks
                .first()
                .copied()
                .unwrap_or("service")
                .bytes()
                .take(12)
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join("");

            format!("service-{bytes}")
        })
}

pub(super) fn normalize_naive_datetime(value: &str) -> Option<String> {
    let value = value.trim();

    if value.is_empty() {
        return None;
    }

    if let Ok(datetime) = DateTime::parse_from_rfc3339(value) {
        return Some(datetime.naive_utc().format("%Y-%m-%dT%H:%M:%S").to_string());
    }

    for format in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(datetime) = NaiveDateTime::parse_from_str(value, format) {
            return Some(datetime.format("%Y-%m-%dT%H:%M:%S").to_string());
        }
    }

    None
}
