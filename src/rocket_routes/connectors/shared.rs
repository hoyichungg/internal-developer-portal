use crate::api::ApiError;
use crate::validation::{validate_request, FieldViolation, Validate};

pub(crate) fn validate_source(source: String) -> Result<String, ApiError> {
    struct ConnectorSource {
        source: String,
    }

    impl Validate for ConnectorSource {
        fn validate(&self) -> Vec<FieldViolation> {
            let mut errors = Vec::new();

            crate::validation::required(&mut errors, "source", &self.source);
            crate::validation::max_len(&mut errors, "source", &self.source, 64);

            errors
        }
    }

    validate_request(ConnectorSource { source }).map(|request| request.source)
}

pub(crate) fn validate_target(target: String) -> Result<String, ApiError> {
    struct ConnectorTarget {
        target: String,
    }

    impl Validate for ConnectorTarget {
        fn validate(&self) -> Vec<FieldViolation> {
            let mut errors = Vec::new();

            crate::validation::required(&mut errors, "target", &self.target);
            crate::validation::max_len(&mut errors, "target", &self.target, 64);
            crate::validation::one_of(
                &mut errors,
                "target",
                &self.target,
                &[
                    "service_health",
                    "work_cards",
                    "notifications",
                    "calendar_events",
                ],
            );

            errors
        }
    }

    validate_request(ConnectorTarget { target }).map(|request| request.target)
}

pub(crate) fn api_error_message(error: &ApiError) -> String {
    match error {
        ApiError::BadRequest => "bad request".to_owned(),
        ApiError::Validation(errors) => errors
            .iter()
            .map(|error| format!("{} {}", error.field, error.message))
            .collect::<Vec<_>>()
            .join(", "),
        ApiError::Unauthorized => "authentication is required".to_owned(),
        ApiError::Forbidden => "permission denied".to_owned(),
        ApiError::NotFound => "resource was not found".to_owned(),
        ApiError::Database(error) => error.to_string(),
        ApiError::DatabaseUnavailable(error) => error.to_string(),
        ApiError::ServiceUnavailable => "service is temporarily unavailable".to_owned(),
        ApiError::Internal => "internal server error".to_owned(),
    }
}

pub(crate) fn count_as_i32(count: usize) -> i32 {
    count.min(i32::MAX as usize) as i32
}

pub(crate) fn validation_error(field: &'static str, message: &'static str) -> ApiError {
    ApiError::Validation(vec![FieldViolation::new(field, message)])
}

pub(crate) fn validation_error_dynamic(field: &'static str, message: String) -> ApiError {
    ApiError::Validation(vec![FieldViolation::new(field, message)])
}
