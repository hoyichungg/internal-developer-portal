use diesel::sql_query;
use diesel_async::RunQueryDsl;
use rocket::serde::Serialize;
use rocket_db_pools::Connection;
use utoipa::ToSchema;

use crate::api::{ok, ApiError, ApiResult};
use crate::rocket_routes::DbConn;

const SERVICE_NAME: &str = "internal-developer-portal-api";

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
}

#[derive(Serialize, ToSchema)]
pub struct ReadinessChecks {
    pub database: &'static str,
}

#[derive(Serialize, ToSchema)]
pub struct ReadinessResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub checks: ReadinessChecks,
}

/// Backwards-compatible health endpoint. Unlike the original implementation,
/// this now has readiness semantics and verifies PostgreSQL before returning 200.
#[rocket::get("/health")]
pub async fn health(mut db: Connection<DbConn>) -> ApiResult<HealthResponse> {
    require_database(&mut db).await?;
    ok(HealthResponse {
        status: "ok",
        service: SERVICE_NAME,
    })
}

/// Process liveness only. This endpoint deliberately performs no dependency I/O.
#[rocket::get("/livez")]
pub async fn livez() -> ApiResult<HealthResponse> {
    ok(HealthResponse {
        status: "ok",
        service: SERVICE_NAME,
    })
}

/// Traffic readiness. A successful response proves a pooled connection can run
/// a query against PostgreSQL.
#[rocket::get("/readyz")]
pub async fn readyz(mut db: Connection<DbConn>) -> ApiResult<ReadinessResponse> {
    require_database(&mut db).await?;
    ok(ReadinessResponse {
        status: "ok",
        service: SERVICE_NAME,
        checks: ReadinessChecks { database: "ok" },
    })
}

async fn require_database(db: &mut Connection<DbConn>) -> Result<(), ApiError> {
    sql_query("SELECT 1")
        .execute(db)
        .await
        .map(|_| ())
        .map_err(ApiError::DatabaseUnavailable)
}

#[cfg(test)]
mod tests {
    use rocket::http::Status;
    use rocket::local::asynchronous::Client;
    use rocket_db_pools::Database;
    use serde_json::json;

    use super::*;

    #[rocket::async_test]
    async fn liveness_stays_up_while_readiness_reports_database_failure() {
        let figment = rocket::Config::figment()
            .merge((
                "databases.postgres.url",
                "postgres://portal:portal@127.0.0.1:1/unreachable",
            ))
            .merge(("databases.postgres.timeout", 1));
        let rocket = rocket::custom(figment)
            .register("/", rocket::catchers![crate::api::service_unavailable])
            .mount("/", rocket::routes![livez, readyz])
            .attach(DbConn::init());
        let client = Client::tracked(rocket)
            .await
            .expect("health test Rocket should ignite");

        let liveness = client.get("/livez").dispatch().await;
        assert_eq!(liveness.status(), Status::Ok);

        let readiness = client.get("/readyz").dispatch().await;
        assert_eq!(readiness.status(), Status::ServiceUnavailable);
        assert_eq!(
            readiness.into_json::<serde_json::Value>().await,
            Some(json!({
                "error": {
                    "code": "service_unavailable",
                    "message": "The service is temporarily unavailable."
                }
            }))
        );
    }
}
