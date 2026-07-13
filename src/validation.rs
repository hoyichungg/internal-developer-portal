use rocket::serde::Serialize;
use utoipa::ToSchema;

use crate::api::ApiError;

/// Converts a portal username into the single identifier form used for
/// authentication, throttling, CLI lookup, and newly persisted users.
///
/// Existing users may retain their original display casing. Database lookup is
/// case-insensitive, while every new write stores this canonical value.
pub fn canonical_username(username: &str) -> String {
    username.trim().to_lowercase()
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct FieldViolation {
    pub field: &'static str,
    pub message: String,
}

impl FieldViolation {
    pub fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

pub trait Validate {
    fn validate(&self) -> Vec<FieldViolation>;
}

pub fn validate_request<T: Validate>(request: T) -> Result<T, ApiError> {
    let errors = request.validate();

    if errors.is_empty() {
        Ok(request)
    } else {
        Err(ApiError::Validation(errors))
    }
}

pub fn required(errors: &mut Vec<FieldViolation>, field: &'static str, value: &str) {
    if value.trim().is_empty() {
        errors.push(FieldViolation::new(field, "is required"));
    }
}

pub fn max_len(errors: &mut Vec<FieldViolation>, field: &'static str, value: &str, max: usize) {
    if value.len() > max {
        errors.push(FieldViolation::new(field, "is too long"));
    }
}

pub fn max_optional_len(
    errors: &mut Vec<FieldViolation>,
    field: &'static str,
    value: &Option<String>,
    max: usize,
) {
    if let Some(value) = value {
        max_len(errors, field, value, max);
    }
}

pub fn positive(errors: &mut Vec<FieldViolation>, field: &'static str, value: i32) {
    if value <= 0 {
        errors.push(FieldViolation::new(field, "must be positive"));
    }
}

pub fn one_of(
    errors: &mut Vec<FieldViolation>,
    field: &'static str,
    value: &str,
    allowed: &[&str],
) {
    if !allowed.contains(&value) {
        errors.push(FieldViolation::new(field, "is not supported"));
    }
}

pub fn optional_url(errors: &mut Vec<FieldViolation>, field: &'static str, value: &Option<String>) {
    if let Some(value) = value {
        if !(value.starts_with("http://") || value.starts_with("https://")) {
            errors.push(FieldViolation::new(field, "must be a valid URL"));
        }
    }
}

pub fn email(errors: &mut Vec<FieldViolation>, field: &'static str, value: &str) {
    if !value.contains('@') || !value.contains('.') {
        errors.push(FieldViolation::new(field, "must be a valid email address"));
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_username;

    #[test]
    fn username_canonicalization_trims_and_lowercases() {
        assert_eq!(canonical_username("  Recovery.Admin  "), "recovery.admin");
        assert_eq!(canonical_username("\u{00c9}QUIPE"), "\u{00e9}quipe");
    }
}
