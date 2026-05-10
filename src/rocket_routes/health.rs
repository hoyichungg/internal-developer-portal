use rocket::serde::Serialize;
use utoipa::ToSchema;

use crate::api::{ok, ApiResult};

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
}

#[rocket::get("/health")]
pub async fn health() -> ApiResult<HealthResponse> {
    ok(HealthResponse {
        status: "ok",
        service: "internal-developer-portal-api",
    })
}
