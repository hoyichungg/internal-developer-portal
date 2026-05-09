use rocket::serde::Deserialize;
use rocket_db_pools::Connection;
use serde_json::Value;

use crate::api::{ok, ApiError, ApiResult};
use crate::auth::{require_admin, AuthenticatedUser};
use crate::models::{AuditLog, NewAuditLog};
use crate::repositories::AuditLogRepository;
use crate::rocket_routes::DbConn;
use crate::validation::{validate_request, FieldViolation, Validate};

#[derive(Deserialize)]
pub struct AuditLogQuery {
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub actor_user_id: Option<i32>,
}

impl Validate for AuditLogQuery {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        crate::validation::max_optional_len(&mut errors, "resource_type", &self.resource_type, 64);
        crate::validation::max_optional_len(&mut errors, "resource_id", &self.resource_id, 128);

        errors
    }
}

#[rocket::get("/audit-logs?<resource_type>&<resource_id>&<actor_user_id>")]
pub async fn get_audit_logs(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    resource_type: Option<String>,
    resource_id: Option<String>,
    actor_user_id: Option<i32>,
) -> ApiResult<Vec<AuditLog>> {
    require_admin(&auth)?;
    let query = validate_request(AuditLogQuery {
        resource_type,
        resource_id,
        actor_user_id,
    })?;
    let audit_logs = AuditLogRepository::find_multiple(
        &mut db,
        100,
        query.resource_type.as_deref(),
        query.resource_id.as_deref(),
        query.actor_user_id,
    )
    .await?;

    ok(audit_logs)
}

pub async fn record_audit_log(
    db: &mut Connection<DbConn>,
    auth: &AuthenticatedUser,
    action: &str,
    resource_type: &str,
    resource_id: impl ToString,
    metadata: Value,
) -> Result<(), ApiError> {
    let metadata = serde_json::to_string(&metadata).ok();

    AuditLogRepository::create(
        db,
        NewAuditLog {
            actor_user_id: Some(auth.user.id),
            action: action.to_owned(),
            resource_type: resource_type.to_owned(),
            resource_id: Some(resource_id.to_string()),
            metadata,
        },
    )
    .await?;

    Ok(())
}

pub async fn record_system_audit_log(
    db: &mut diesel_async::AsyncPgConnection,
    action: &str,
    resource_type: &str,
    resource_id: impl ToString,
    metadata: Value,
) -> Result<(), ApiError> {
    let metadata = serde_json::to_string(&metadata).ok();

    AuditLogRepository::create(
        db,
        NewAuditLog {
            actor_user_id: None,
            action: action.to_owned(),
            resource_type: resource_type.to_owned(),
            resource_id: Some(resource_id.to_string()),
            metadata,
        },
    )
    .await?;

    Ok(())
}
