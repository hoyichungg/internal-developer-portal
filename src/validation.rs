use rocket::serde::Serialize;

use crate::api::ApiError;

#[derive(Clone, Debug, Serialize)]
pub struct FieldViolation {
    pub field: &'static str,
    pub message: &'static str,
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
        errors.push(FieldViolation {
            field,
            message: "is required",
        });
    }
}

pub fn max_len(errors: &mut Vec<FieldViolation>, field: &'static str, value: &str, max: usize) {
    if value.len() > max {
        errors.push(FieldViolation {
            field,
            message: "is too long",
        });
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
        errors.push(FieldViolation {
            field,
            message: "must be positive",
        });
    }
}

pub fn one_of(
    errors: &mut Vec<FieldViolation>,
    field: &'static str,
    value: &str,
    allowed: &[&str],
) {
    if !allowed.contains(&value) {
        errors.push(FieldViolation {
            field,
            message: "is not supported",
        });
    }
}

pub fn optional_url(errors: &mut Vec<FieldViolation>, field: &'static str, value: &Option<String>) {
    if let Some(value) = value {
        if !(value.starts_with("http://") || value.starts_with("https://")) {
            errors.push(FieldViolation {
                field,
                message: "must be a valid URL",
            });
        }
    }
}

pub fn email(errors: &mut Vec<FieldViolation>, field: &'static str, value: &str) {
    if !value.contains('@') || !value.contains('.') {
        errors.push(FieldViolation {
            field,
            message: "must be a valid email address",
        });
    }
}
