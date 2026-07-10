use serde_json::Value;

use crate::validation::FieldViolation;

pub(crate) fn validate_connector_config_adapter(
    errors: &mut Vec<FieldViolation>,
    target: &str,
    config: &Value,
) {
    if !config.is_object() {
        errors.push(FieldViolation::new("config", "must be a JSON object"));
        return;
    }

    let Some(adapter) = adapter_name(errors, config) else {
        return;
    };

    match adapter {
        "azure_devops" => validate_azure_devops(errors, target, config),
        "monitoring" => validate_monitoring(errors, target, config),
        "microsoft_graph_calendar" | "graph_calendar" | "outlook_calendar" => {
            validate_graph_calendar(errors, target, config)
        }
        "microsoft_graph_mail" | "graph_mail" | "outlook_mail" => {
            validate_graph_mail(errors, target, config)
        }
        "erp_private_messages" | "erp_messages_http" | "erp_http" => {
            validate_erp_private_messages(errors, target, config)
        }
        "calendar_sample" | "calendar" => validate_sample_array(
            errors,
            target,
            config,
            "events",
            &["calendar_events", "notifications"],
        ),
        "outlook_mail_sample" | "outlook" => {
            validate_sample_array(errors, target, config, "messages", &["notifications"])
        }
        "erp_messages_sample" | "erp_messages" | "erp" => {
            validate_sample_array(errors, target, config, "messages", &["notifications"])
        }
        _ => errors.push(FieldViolation::new(
            "config",
            format!("adapter {adapter} is not supported"),
        )),
    }
}

fn validate_azure_devops(errors: &mut Vec<FieldViolation>, target: &str, config: &Value) {
    require_target(errors, "azure_devops", target, "work_cards");
    require_non_empty_when_missing_all(
        errors,
        config,
        "organization",
        &["wiql_url", "work_items_url"],
        "must be set when wiql_url and work_items_url are not both provided",
    );
    require_non_empty_when_missing_all(
        errors,
        config,
        "project",
        &["wiql_url", "work_items_url"],
        "must be set when wiql_url and work_items_url are not both provided",
    );
    validate_url_fields(
        errors,
        config,
        &["wiql_url", "work_items_url", "base_url", "web_url_base"],
    );
    validate_positive_u64(errors, config, "timeout_seconds");
    validate_u64_range(errors, config, "max_items", 1, 10_000);
}

fn adapter_name<'a>(errors: &mut Vec<FieldViolation>, config: &'a Value) -> Option<&'a str> {
    let adapter = config.get("adapter")?;

    let Some(adapter) = adapter.as_str() else {
        errors.push(FieldViolation::new("config", "adapter must be a string"));
        return None;
    };

    let adapter = adapter.trim();
    if adapter.is_empty() {
        errors.push(FieldViolation::new("config", "adapter must not be empty"));
        return None;
    }

    Some(adapter)
}

fn validate_monitoring(errors: &mut Vec<FieldViolation>, target: &str, config: &Value) {
    require_target(errors, "monitoring", target, "service_health");
    require_url_any(errors, config, &["url"]);
    validate_positive_i64(errors, config, "default_maintainer_id");
    validate_positive_u64(errors, config, "timeout_seconds");
}

fn validate_graph_calendar(errors: &mut Vec<FieldViolation>, target: &str, config: &Value) {
    require_one_of_targets(
        errors,
        "microsoft_graph_calendar",
        target,
        &["calendar_events", "notifications"],
    );
    validate_url_fields(
        errors,
        config,
        &[
            "calendar_view_url",
            "base_url",
            "token_url",
            "authorization_url",
        ],
    );
    validate_positive_u64(errors, config, "timeout_seconds");
    validate_u64_range(errors, config, "top", 1, 50);
    validate_i64_range(errors, config, "lookahead_hours", 1, 168);
    validate_u64_range(errors, config, "max_pages", 1, 100);
    validate_u64_range(errors, config, "max_items", 1, 10_000);
}

fn validate_graph_mail(errors: &mut Vec<FieldViolation>, target: &str, config: &Value) {
    require_target(errors, "microsoft_graph_mail", target, "notifications");
    validate_url_fields(
        errors,
        config,
        &[
            "messages_url",
            "mail_messages_url",
            "base_url",
            "token_url",
            "authorization_url",
        ],
    );
    validate_positive_u64(errors, config, "timeout_seconds");
    validate_u64_range(errors, config, "top", 1, 50);
    validate_i64_range(errors, config, "lookback_hours", 1, 720);
    validate_u64_range(errors, config, "max_pages", 1, 100);
    validate_u64_range(errors, config, "max_items", 1, 10_000);
}

fn validate_erp_private_messages(errors: &mut Vec<FieldViolation>, target: &str, config: &Value) {
    require_target(errors, "erp_private_messages", target, "notifications");
    require_url_any(
        errors,
        config,
        &["messages_url", "private_messages_url", "url"],
    );
    validate_positive_u64(errors, config, "timeout_seconds");
    validate_u64_range(errors, config, "top", 1, 100);
    validate_u64_range(errors, config, "limit", 1, 100);
    validate_i64_range(errors, config, "lookback_hours", 1, 720);
    validate_boolean(errors, config, "snapshot_complete");
    if let Some(header) = field_string(config, &["api_key_header"]) {
        if !valid_header_name(&header) {
            errors.push(FieldViolation::new(
                "config",
                "api_key_header must be a valid HTTP header name",
            ));
        }
    }
}

fn validate_sample_array(
    errors: &mut Vec<FieldViolation>,
    target: &str,
    config: &Value,
    field: &'static str,
    targets: &[&str],
) {
    require_one_of_targets(errors, "sample notification adapter", target, targets);
    if let Some(value) = config.get(field) {
        if !value.is_array() {
            errors.push(FieldViolation::new(
                "config",
                format!("{field} must be an array when provided"),
            ));
        }
    }
}

fn require_target(errors: &mut Vec<FieldViolation>, adapter: &str, target: &str, expected: &str) {
    if target != expected {
        errors.push(FieldViolation::new(
            "target",
            format!("{adapter} config requires target {expected}"),
        ));
    }
}

fn require_one_of_targets(
    errors: &mut Vec<FieldViolation>,
    adapter: &str,
    target: &str,
    expected: &[&str],
) {
    if !expected.contains(&target) {
        errors.push(FieldViolation::new(
            "target",
            format!("{adapter} config requires target {}", expected.join(" or ")),
        ));
    }
}

fn require_url_any(errors: &mut Vec<FieldViolation>, config: &Value, fields: &[&str]) {
    if !fields.iter().any(|field| has_non_empty(config, field)) {
        errors.push(FieldViolation::new(
            "config",
            format!("must set one of {}", fields.join(", ")),
        ));
        return;
    }

    validate_url_fields(errors, config, fields);
}

fn require_non_empty_when_missing_all(
    errors: &mut Vec<FieldViolation>,
    config: &Value,
    field: &'static str,
    fallback_fields: &[&str],
    message: &str,
) {
    let all_fallbacks_present = fallback_fields
        .iter()
        .all(|fallback_field| has_non_empty(config, fallback_field));

    if !all_fallbacks_present && !has_non_empty(config, field) {
        errors.push(FieldViolation::new("config", format!("{field} {message}")));
    }
}

fn validate_url_fields(errors: &mut Vec<FieldViolation>, config: &Value, fields: &[&str]) {
    for field in fields {
        let Some(url) = field_string(config, &[*field]) else {
            continue;
        };

        if !(url.starts_with("http://") || url.starts_with("https://")) {
            errors.push(FieldViolation::new(
                "config",
                format!("{field} must be an absolute HTTP URL"),
            ));
        }
    }
}

fn validate_positive_i64(errors: &mut Vec<FieldViolation>, config: &Value, field: &'static str) {
    if let Some(value) = config.get(field) {
        match value.as_i64() {
            Some(value) if value > 0 => {}
            _ => errors.push(FieldViolation::new(
                "config",
                format!("{field} must be a positive integer"),
            )),
        }
    }
}

fn validate_positive_u64(errors: &mut Vec<FieldViolation>, config: &Value, field: &'static str) {
    if let Some(value) = config.get(field) {
        match value.as_u64() {
            Some(value) if value > 0 => {}
            _ => errors.push(FieldViolation::new(
                "config",
                format!("{field} must be a positive integer"),
            )),
        }
    }
}

fn validate_u64_range(
    errors: &mut Vec<FieldViolation>,
    config: &Value,
    field: &'static str,
    min: u64,
    max: u64,
) {
    if let Some(value) = config.get(field) {
        match value.as_u64() {
            Some(value) if (min..=max).contains(&value) => {}
            _ => errors.push(FieldViolation::new(
                "config",
                format!("{field} must be an integer from {min} to {max}"),
            )),
        }
    }
}

fn validate_i64_range(
    errors: &mut Vec<FieldViolation>,
    config: &Value,
    field: &'static str,
    min: i64,
    max: i64,
) {
    if let Some(value) = config.get(field) {
        match value.as_i64() {
            Some(value) if (min..=max).contains(&value) => {}
            _ => errors.push(FieldViolation::new(
                "config",
                format!("{field} must be an integer from {min} to {max}"),
            )),
        }
    }
}

fn validate_boolean(errors: &mut Vec<FieldViolation>, config: &Value, field: &'static str) {
    if config.get(field).is_some_and(|value| !value.is_boolean()) {
        errors.push(FieldViolation::new(
            "config",
            format!("{field} must be a boolean"),
        ));
    }
}

fn has_non_empty(config: &Value, field: &str) -> bool {
    field_string(config, &[field]).is_some()
}

fn field_string(config: &Value, fields: &[&str]) -> Option<String> {
    fields
        .iter()
        .find_map(|field| config.get(*field))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn valid_header_name(value: &str) -> bool {
    value.bytes().all(|byte| {
        matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
                | b'0'..=b'9'
                | b'A'..=b'Z'
                | b'a'..=b'z'
        )
    })
}

#[cfg(test)]
mod tests {
    use super::validate_connector_config_adapter;
    use crate::validation::FieldViolation;
    use serde_json::json;

    fn validate(target: &str, config: serde_json::Value) -> Vec<FieldViolation> {
        let mut errors = Vec::new();
        validate_connector_config_adapter(&mut errors, target, &config);
        errors
    }

    #[test]
    fn accepts_graph_calendar_oauth_setup_without_tokens() {
        for target in ["calendar_events", "notifications"] {
            let errors = validate(
                target,
                json!({
                    "adapter": "microsoft_graph_calendar",
                    "tenant_id": "organizations",
                    "client_id": "client-id",
                    "scope": "https://graph.microsoft.com/Calendars.Read offline_access",
                    "lookahead_hours": 24,
                    "top": 25
                }),
            );

            assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        }
    }

    #[test]
    fn validates_connector_collection_safety_limits() {
        let graph_errors = validate(
            "notifications",
            json!({
                "adapter": "microsoft_graph_mail",
                "max_pages": 0,
                "max_items": 10_001
            }),
        );
        let azure_errors = validate(
            "work_cards",
            json!({
                "adapter": "azure_devops",
                "organization": "acme",
                "project": "portal",
                "max_items": -1
            }),
        );
        let messages = graph_errors
            .iter()
            .chain(&azure_errors)
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>();

        assert!(messages
            .iter()
            .any(|message| message.contains("max_pages must be an integer from 1 to 100")));
        assert!(
            messages
                .iter()
                .filter(|message| message.contains("max_items must be an integer from 1 to 10000"))
                .count()
                >= 2
        );
    }

    #[test]
    fn rejects_adapter_target_mismatch_and_missing_required_url() {
        let errors = validate(
            "work_cards",
            json!({
                "adapter": "erp_private_messages",
                "top": 10
            }),
        );
        let messages = errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>();

        assert!(messages
            .iter()
            .any(|message| message.contains("requires target notifications")));
        assert!(messages
            .iter()
            .any(|message| message.contains("must set one of messages_url")));
    }

    #[test]
    fn accepts_custom_json_without_adapter() {
        let errors = validate(
            "notifications",
            json!({
                "mapping": {
                    "title": "$.title"
                }
            }),
        );

        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn rejects_unknown_adapter() {
        let errors = validate(
            "notifications",
            json!({
                "adapter": "unknown_adapter"
            }),
        );
        let messages = errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>();

        assert!(messages
            .iter()
            .any(|message| message.contains("adapter unknown_adapter is not supported")));
    }

    #[test]
    fn rejects_adapter_with_wrong_shape() {
        let non_string_errors = validate(
            "notifications",
            json!({
                "adapter": 123
            }),
        );
        let empty_errors = validate(
            "notifications",
            json!({
                "adapter": " "
            }),
        );

        assert!(non_string_errors
            .iter()
            .any(|error| error.message.contains("adapter must be a string")));
        assert!(empty_errors
            .iter()
            .any(|error| error.message.contains("adapter must not be empty")));
    }

    #[test]
    fn rejects_non_object_config() {
        let errors = validate("notifications", json!(["not", "an", "object"]));
        let messages = errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>();

        assert!(messages
            .iter()
            .any(|message| message.contains("must be a JSON object")));
    }

    #[test]
    fn rejects_azure_devops_without_endpoint_or_project_context() {
        let errors = validate(
            "work_cards",
            json!({
                "adapter": "azure_devops",
                "timeout_seconds": 15
            }),
        );
        let messages = errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>();

        assert!(messages
            .iter()
            .any(|message| message.contains("organization must be set")));
        assert!(messages
            .iter()
            .any(|message| message.contains("project must be set")));
    }

    #[test]
    fn rejects_invalid_adapter_url_and_bounds() {
        let errors = validate(
            "service_health",
            json!({
                "adapter": "monitoring",
                "url": "ftp://monitoring.example.test/feed",
                "timeout_seconds": 0
            }),
        );
        let messages = errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>();

        assert!(messages
            .iter()
            .any(|message| message.contains("url must be an absolute HTTP URL")));
        assert!(messages
            .iter()
            .any(|message| message.contains("timeout_seconds must be a positive integer")));
    }

    #[test]
    fn rejects_non_boolean_erp_snapshot_declaration() {
        let errors = validate(
            "notifications",
            json!({
                "adapter": "erp_private_messages",
                "messages_url": "https://erp.example.test/messages",
                "snapshot_complete": "yes"
            }),
        );

        assert!(errors
            .iter()
            .any(|error| error.message == "snapshot_complete must be a boolean"));
    }
}
