use chrono::{DateTime, NaiveDate, Utc};
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
        crate::validation::max_optional_len(&mut errors, "created_from", &self.created_from, 64);
        crate::validation::max_optional_len(&mut errors, "created_to", &self.created_to, 64);

        errors
    }
}

#[rocket::get("/audit-logs?<query..>")]
pub async fn get_audit_logs(
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
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
) -> Result<Option<DateTime<Utc>>, ApiError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let date_time = if end_of_day {
            date.and_hms_micro_opt(23, 59, 59, 999_999)
        } else {
            date.and_hms_opt(0, 0, 0)
        }
        .expect("valid static time");

        return Ok(Some(date_time.and_utc()));
    }

    if let Ok(date_time) = DateTime::parse_from_rfc3339(value) {
        return Ok(Some(date_time.with_timezone(&Utc)));
    }

    Err(ApiError::Validation(vec![FieldViolation::new(
        field,
        "must be YYYY-MM-DD (UTC) or RFC3339 with an explicit offset",
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

#[cfg(test)]
mod tests {
    use super::parse_date_bound;
    use chrono::{TimeZone, Timelike, Utc};

    #[test]
    fn date_only_audit_bounds_are_the_full_utc_day() {
        let start = parse_date_bound("created_from", Some("2026-07-12"), false)
            .unwrap()
            .unwrap();
        let end = parse_date_bound("created_to", Some("2026-07-12"), true)
            .unwrap()
            .unwrap();

        assert_eq!(start, Utc.with_ymd_and_hms(2026, 7, 12, 0, 0, 0).unwrap());
        assert_eq!(end.date_naive(), start.date_naive());
        assert_eq!((end.hour(), end.minute(), end.second()), (23, 59, 59));
        assert_eq!(end.timestamp_subsec_micros(), 999_999);
    }

    #[test]
    fn audit_rfc3339_offsets_are_normalized_and_naive_datetimes_are_rejected() {
        let parsed = parse_date_bound("created_from", Some("2026-07-12T20:30:00+08:00"), false)
            .unwrap()
            .unwrap();

        assert_eq!(
            parsed,
            Utc.with_ymd_and_hms(2026, 7, 12, 12, 30, 0).unwrap()
        );
        assert!(parse_date_bound("created_from", Some("2026-07-12T20:30"), false,).is_err());
    }
}
