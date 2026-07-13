use diesel::result::{DatabaseErrorKind, Error as DieselError};
use rocket::http::{Header, Status};
use rocket::request::Request;
use rocket::response::status::Custom;
use rocket::response::{self, Responder};
use rocket::serde::json::Json;
use rocket::serde::Serialize;
use utoipa::ToSchema;

use crate::validation::FieldViolation;

#[derive(Serialize, ToSchema)]
pub struct ApiResponse<T> {
    pub data: T,
}

#[derive(Serialize, ToSchema)]
pub struct ApiErrorResponse {
    pub error: ApiErrorBody,
}

#[derive(Serialize, ToSchema)]
pub struct ApiErrorBody {
    pub code: &'static str,
    pub message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Vec<FieldViolation>>,
}

#[derive(Debug)]
pub enum ApiError {
    BadRequest,
    Validation(Vec<FieldViolation>),
    Unauthorized,
    RateLimited { retry_after_seconds: i64 },
    AuthenticationCapacityLimited { retry_after_seconds: i64 },
    Forbidden,
    NotFound,
    Database(DieselError),
    DatabaseUnavailable(DieselError),
    ServiceUnavailable,
    Internal,
}

pub type ApiResult<T> = Result<Json<ApiResponse<T>>, ApiError>;
pub type CreatedApiResult<T> = Result<Custom<Json<ApiResponse<T>>>, ApiError>;

pub fn ok<T>(data: T) -> ApiResult<T> {
    Ok(Json(ApiResponse { data }))
}

pub fn created<T>(data: T) -> CreatedApiResult<T> {
    Ok(Custom(Status::Created, Json(ApiResponse { data })))
}

#[rocket::catch(400)]
pub fn bad_request() -> ApiError {
    ApiError::BadRequest
}

#[rocket::catch(401)]
pub fn unauthorized() -> ApiError {
    ApiError::Unauthorized
}

#[rocket::catch(429)]
pub fn too_many_requests() -> ApiError {
    ApiError::RateLimited {
        retry_after_seconds: 60,
    }
}

#[rocket::catch(403)]
pub fn forbidden() -> ApiError {
    ApiError::Forbidden
}

#[rocket::catch(404)]
pub fn not_found() -> ApiError {
    ApiError::NotFound
}

#[rocket::catch(422)]
pub fn unprocessable_entity() -> ApiError {
    ApiError::BadRequest
}

#[rocket::catch(500)]
pub fn internal_server_error() -> ApiError {
    ApiError::Internal
}

#[rocket::catch(503)]
pub fn service_unavailable() -> ApiError {
    ApiError::ServiceUnavailable
}

impl ApiError {
    pub(crate) fn status(&self) -> Status {
        match self {
            Self::BadRequest | Self::Validation(_) => Status::BadRequest,
            Self::Unauthorized => Status::Unauthorized,
            Self::RateLimited { .. } | Self::AuthenticationCapacityLimited { .. } => {
                Status::TooManyRequests
            }
            Self::Forbidden => Status::Forbidden,
            Self::NotFound => Status::NotFound,
            Self::Database(_) | Self::Internal => Status::InternalServerError,
            Self::DatabaseUnavailable(_) | Self::ServiceUnavailable => Status::ServiceUnavailable,
        }
    }

    fn body(&self) -> ApiErrorResponse {
        let (code, message, details) = match self {
            Self::BadRequest => ("bad_request", "Bad request.", None),
            Self::Validation(errors) => (
                "validation_failed",
                "Request validation failed.",
                Some(errors.clone()),
            ),
            Self::Unauthorized => ("unauthorized", "Authentication is required.", None),
            Self::RateLimited { .. } => (
                "login_throttled",
                "Too many sign-in attempts. Try again later.",
                None,
            ),
            Self::AuthenticationCapacityLimited { .. } => (
                "authentication_capacity_limited",
                "Sign-in is temporarily at capacity. Try again later.",
                None,
            ),
            Self::Forbidden => (
                "forbidden",
                "You are not allowed to perform this action.",
                None,
            ),
            Self::NotFound => ("not_found", "Resource was not found.", None),
            Self::Database(_) | Self::Internal => (
                "internal_server_error",
                "An internal server error occurred.",
                None,
            ),
            Self::DatabaseUnavailable(_) | Self::ServiceUnavailable => (
                "service_unavailable",
                "The service is temporarily unavailable.",
                None,
            ),
        };

        ApiErrorResponse {
            error: ApiErrorBody {
                code,
                message,
                details,
            },
        }
    }
}

impl From<DieselError> for ApiError {
    fn from(error: DieselError) -> Self {
        match error {
            DieselError::NotFound => Self::NotFound,
            DieselError::DatabaseError(
                kind @ (DatabaseErrorKind::ClosedConnection
                | DatabaseErrorKind::UnableToSendCommand),
                information,
            ) => Self::DatabaseUnavailable(DieselError::DatabaseError(kind, information)),
            DieselError::BrokenTransactionManager => {
                Self::DatabaseUnavailable(DieselError::BrokenTransactionManager)
            }
            error => Self::Database(error),
        }
    }
}

impl<'r> Responder<'r, 'static> for ApiError {
    fn respond_to(self, request: &'r Request<'_>) -> response::Result<'static> {
        let retry_after_seconds = match &self {
            Self::RateLimited {
                retry_after_seconds,
            }
            | Self::AuthenticationCapacityLimited {
                retry_after_seconds,
            } => Some(*retry_after_seconds),
            _ => None,
        };
        if matches!(
            self,
            Self::Database(_)
                | Self::DatabaseUnavailable(_)
                | Self::Internal
                | Self::ServiceUnavailable
        ) {
            rocket::error!("{:?}", self);
        }

        let mut response = Custom(self.status(), Json(self.body())).respond_to(request)?;
        if let Some(retry_after_seconds) = retry_after_seconds {
            response.set_header(Header::new(
                "Retry-After",
                retry_after_seconds.max(1).to_string(),
            ));
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[rocket::get("/login-throttled")]
    fn login_throttled() -> ApiError {
        ApiError::RateLimited {
            retry_after_seconds: 90,
        }
    }

    #[rocket::get("/authentication-capacity-limited")]
    fn authentication_capacity_limited() -> ApiError {
        ApiError::AuthenticationCapacityLimited {
            retry_after_seconds: 60,
        }
    }

    #[test]
    fn broken_database_connections_are_reported_as_unavailable() {
        let error = ApiError::from(DieselError::BrokenTransactionManager);

        assert!(matches!(error, ApiError::DatabaseUnavailable(_)));
        assert_eq!(error.status(), Status::ServiceUnavailable);
    }

    #[test]
    fn authentication_capacity_has_a_distinct_structured_429_contract() {
        let rocket = rocket::build().mount(
            "/",
            rocket::routes![login_throttled, authentication_capacity_limited],
        );
        let client = rocket::local::blocking::Client::tracked(rocket).unwrap();

        let login_response = client.get("/login-throttled").dispatch();
        assert_eq!(login_response.status(), Status::TooManyRequests);
        assert_eq!(login_response.headers().get_one("Retry-After"), Some("90"));
        assert_eq!(
            login_response.into_json::<serde_json::Value>().unwrap()["error"]["code"],
            "login_throttled"
        );

        let capacity_response = client.get("/authentication-capacity-limited").dispatch();
        assert_eq!(capacity_response.status(), Status::TooManyRequests);
        assert_eq!(
            capacity_response.headers().get_one("Retry-After"),
            Some("60")
        );
        let body = capacity_response.into_json::<serde_json::Value>().unwrap();
        assert_eq!(body["error"]["code"], "authentication_capacity_limited");
        assert_eq!(
            body["error"]["message"],
            "Sign-in is temporarily at capacity. Try again later."
        );
    }
}
