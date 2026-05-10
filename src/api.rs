use diesel::result::Error as DieselError;
use rocket::http::Status;
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
    Forbidden,
    NotFound,
    Database(DieselError),
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

impl ApiError {
    fn status(&self) -> Status {
        match self {
            Self::BadRequest | Self::Validation(_) => Status::BadRequest,
            Self::Unauthorized => Status::Unauthorized,
            Self::Forbidden => Status::Forbidden,
            Self::NotFound => Status::NotFound,
            Self::Database(_) | Self::Internal => Status::InternalServerError,
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
            error => Self::Database(error),
        }
    }
}

impl<'r> Responder<'r, 'static> for ApiError {
    fn respond_to(self, request: &'r Request<'_>) -> response::Result<'static> {
        if matches!(self, Self::Database(_) | Self::Internal) {
            rocket::error!("{:?}", self);
        }

        Custom(self.status(), Json(self.body())).respond_to(request)
    }
}
