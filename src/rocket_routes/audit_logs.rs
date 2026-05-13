use chrono::{NaiveDate, NaiveDateTime};
use rocket::form::FromForm;
use rocket::serde::Deserialize;
use rocket_db_pools::Connection;
use serde_json::Value;

use crate::api::{ok, ApiError, ApiResult};
use crate::auth::{require_admin, AuthenticatedUser};
use crate::models::{AuditLog, NewAuditLog};
use crate::repositories::{AuditLogFilters, AuditLogRepository};
use crate::rocket_routes::DbConn;
use crate::validation::{validate_request, FieldViolation, Validate};

#[derive(Deserialize, FromForm)]
pub struct AuditLogQuery {
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub actor_user_id: Option<i32>,
    pub action: Option<String>,
    pub created_from: Option<String>,
    pub created_to: Option<String>,
}

impl Validate for AuditLogQuery {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        crate::validation::max_optional_len(&mut errors, "resource_type", &self.resource_type, 64);
        crate::validation::max_optional_len(&mut errors, "resource_id", &self.resource_id, 128);
        crate::validation::max_optional_len(&mut errors, "action", &self.action, 64);
        crate::validation::max_optional_len(&mut errors, "created_from", &self.created_from, 32);
        crate::validation::max_optional_len(&mut errors, "created_to", &self.created_to, 32);

        errors
    }
}

#[rocket::get("/audit-logs?<query..>")]
pub async fn get_audit_logs(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    query: AuditLogQuery,
) -> ApiResult<Vec<AuditLog>> {
    require_admin(&auth)?;
    let query = validate_request(query)?;
    let created_from = parse_date_bound("created_from", query.created_from.as_deref(), false)?;
    let created_to = parse_date_bound("created_to", query.created_to.as_deref(), true)?;

    if let (Some(created_from), Some(created_to)) = (created_from, created_to) {
        if created_from > created_to {
            return Err(ApiError::Validation(vec![FieldViolation::new(
                "created_to",
                "must be after created_from",
            )]));
        }
    }

    let audit_logs = AuditLogRepository::find_multiple(
        &mut db,
        100,
        AuditLogFilters {
            resource_type: query.resource_type.as_deref(),
            resource_id: query.resource_id.as_deref(),
            actor_user_id: query.actor_user_id,
            action: query.action.as_deref(),
            created_from,
            created_to,
        },
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

fn parse_date_bound(
    field: &'static str,
    value: Option<&str>,
    end_of_day: bool,
) -> Result<Option<NaiveDateTime>, ApiError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let time = if end_of_day {
            chrono::NaiveTime::from_hms_opt(23, 59, 59)
        } else {
            chrono::NaiveTime::from_hms_opt(0, 0, 0)
        }
        .expect("valid static time");

        return Ok(Some(NaiveDateTime::new(date, time)));
    }

    if let Ok(date_time) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M") {
        return Ok(Some(date_time));
    }

    Err(ApiError::Validation(vec![FieldViolation::new(
        field,
        "must be YYYY-MM-DD or YYYY-MM-DDTHH:MM",
    )]))
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
