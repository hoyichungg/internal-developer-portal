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

#[cfg(test)]
pub(super) mod test_support {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    pub(crate) struct MockResponse {
        status: u16,
        body: String,
    }

    impl MockResponse {
        pub(crate) fn json(body: impl Into<String>) -> Self {
            Self {
                status: 200,
                body: body.into(),
            }
        }

        pub(crate) fn with_status(status: u16, body: impl Into<String>) -> Self {
            Self {
                status,
                body: body.into(),
            }
        }
    }

    pub(crate) struct MockHttpServer {
        base_url: String,
        requests: Arc<Mutex<Vec<String>>>,
        handle: Option<JoinHandle<()>>,
    }

    impl MockHttpServer {
        pub(crate) fn start(responses: Vec<MockResponse>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock HTTP server");
            listener
                .set_nonblocking(true)
                .expect("set mock listener nonblocking");
            let base_url = format!(
                "http://{}",
                listener.local_addr().expect("read mock server address")
            );
            let response_base_url = base_url.clone();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let thread_requests = Arc::clone(&requests);
            let handle = thread::spawn(move || {
                for response in responses {
                    let deadline = Instant::now() + Duration::from_secs(3);
                    let mut stream = loop {
                        match listener.accept() {
                            Ok((stream, _)) => break stream,
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                if Instant::now() >= deadline {
                                    return;
                                }
                                thread::sleep(Duration::from_millis(5));
                            }
                            Err(_) => return,
                        }
                    };

                    let Ok(request) = read_request(&mut stream) else {
                        return;
                    };
                    thread_requests
                        .lock()
                        .expect("lock mock requests")
                        .push(request);

                    let body = response.body.replace("{{base_url}}", &response_base_url);
                    let reason = match response.status {
                        200 => "OK",
                        400 => "Bad Request",
                        401 => "Unauthorized",
                        403 => "Forbidden",
                        404 => "Not Found",
                        429 => "Too Many Requests",
                        500 => "Internal Server Error",
                        503 => "Service Unavailable",
                        _ => "Mock Response",
                    };
                    let headers = format!(
                        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        response.status,
                        reason,
                        body.len()
                    );
                    if stream.write_all(headers.as_bytes()).is_err() {
                        return;
                    }
                    if stream.write_all(body.as_bytes()).is_err() {
                        return;
                    }
                }
            });

            Self {
                base_url,
                requests,
                handle: Some(handle),
            }
        }

        pub(crate) fn url(&self, path: &str) -> String {
            format!("{}{}", self.base_url, path)
        }

        pub(crate) fn requests(&self) -> Vec<String> {
            self.requests.lock().expect("lock mock requests").clone()
        }
    }

    impl Drop for MockHttpServer {
        fn drop(&mut self) {
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn read_request(stream: &mut TcpStream) -> std::io::Result<String> {
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4_096];

        loop {
            let read = stream.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);

            let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let header_size = header_end + 4;
            let headers = String::from_utf8_lossy(&bytes[..header_size]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);

            if bytes.len() >= header_size + content_length {
                break;
            }
        }

        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}
