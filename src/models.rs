use crate::schema::*;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use rocket::serde::Serialize;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::validation::{
    email, max_len, max_optional_len, one_of, optional_url, positive, required, FieldViolation,
    Validate,
};

fn default_manual_source() -> String {
    "manual".to_owned()
}

fn default_global_scope() -> String {
    "global".to_owned()
}

fn default_connector_enabled() -> bool {
    true
}

fn default_connector_config() -> String {
    "{}".to_owned()
}

fn default_connector_sample_payload() -> String {
    r#"{"items":[]}"#.to_owned()
}

pub(crate) fn schedule_interval_seconds(schedule: &str) -> Option<i64> {
    let schedule = schedule.trim().to_ascii_lowercase();

    match schedule.as_str() {
        "@hourly" => Some(60 * 60),
        "@daily" => Some(24 * 60 * 60),
        _ => schedule
            .strip_prefix("@every ")
            .and_then(parse_duration_seconds)
            .or_else(|| parse_duration_seconds(&schedule)),
    }
}

pub(crate) const MIN_CONNECTOR_SCHEDULE_INTERVAL_SECONDS: i64 = 60;

pub(crate) fn effective_schedule_interval_seconds(schedule: &str) -> Option<i64> {
    schedule_interval_seconds(schedule)
        .map(|seconds| seconds.max(MIN_CONNECTOR_SCHEDULE_INTERVAL_SECONDS))
}

fn parse_duration_seconds(value: &str) -> Option<i64> {
    let (number, multiplier) = if let Some(number) = value.strip_suffix('s') {
        (number, 1)
    } else if let Some(number) = value.strip_suffix('m') {
        (number, 60)
    } else if let Some(number) = value.strip_suffix('h') {
        (number, 60 * 60)
    } else {
        return None;
    };

    number
        .parse::<i64>()
        .ok()
        .filter(|seconds| *seconds > 0)
        .and_then(|seconds| seconds.checked_mul(multiplier))
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct Connector {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub source: String,
    pub kind: String,
    pub display_name: String,
    pub status: String,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
    #[serde(skip_deserializing)]
    pub updated_at: DateTime<Utc>,
    pub scope_type: String,
    pub owner_user_id: Option<i32>,
    pub maintainer_id: Option<i32>,
}

#[derive(Insertable, Deserialize, ToSchema)]
#[diesel(table_name=connectors)]
pub struct NewConnector {
    pub source: String,
    pub kind: String,
    pub display_name: String,
    pub status: String,
    #[serde(default = "default_global_scope")]
    pub scope_type: String,
    pub owner_user_id: Option<i32>,
    pub maintainer_id: Option<i32>,
}

impl Validate for NewConnector {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        required(&mut errors, "source", &self.source);
        max_len(&mut errors, "source", &self.source, 64);
        required(&mut errors, "kind", &self.kind);
        max_len(&mut errors, "kind", &self.kind, 64);
        required(&mut errors, "display_name", &self.display_name);
        max_len(&mut errors, "display_name", &self.display_name, 128);
        required(&mut errors, "status", &self.status);
        max_len(&mut errors, "status", &self.status, 32);
        one_of(
            &mut errors,
            "status",
            &self.status,
            &["active", "paused", "error"],
        );
        validate_data_scope(
            &mut errors,
            &self.scope_type,
            self.owner_user_id,
            self.maintainer_id,
        );

        errors
    }
}

#[derive(Deserialize, ToSchema)]
pub struct ConnectorScopeUpdate {
    pub scope_type: String,
    pub owner_user_id: Option<i32>,
    pub maintainer_id: Option<i32>,
}

impl Validate for ConnectorScopeUpdate {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();
        validate_data_scope(
            &mut errors,
            &self.scope_type,
            self.owner_user_id,
            self.maintainer_id,
        );
        errors
    }
}

fn validate_data_scope(
    errors: &mut Vec<FieldViolation>,
    scope_type: &str,
    owner_user_id: Option<i32>,
    maintainer_id: Option<i32>,
) {
    required(errors, "scope_type", scope_type);
    max_len(errors, "scope_type", scope_type, 16);
    one_of(
        errors,
        "scope_type",
        scope_type,
        &["global", "maintainer", "user"],
    );

    let valid_shape = match scope_type {
        "global" => owner_user_id.is_none() && maintainer_id.is_none(),
        "user" => owner_user_id.is_some() && maintainer_id.is_none(),
        "maintainer" => owner_user_id.is_none() && maintainer_id.is_some(),
        _ => true,
    };
    if !valid_shape {
        errors.push(FieldViolation::new(
            "scope_type",
            "must match exactly one owner: global has none, user has owner_user_id, maintainer has maintainer_id",
        ));
    }
    if owner_user_id.is_some_and(|id| id <= 0) {
        errors.push(FieldViolation::new("owner_user_id", "must be positive"));
    }
    if maintainer_id.is_some_and(|id| id <= 0) {
        errors.push(FieldViolation::new("maintainer_id", "must be positive"));
    }
}

#[derive(AsChangeset, Deserialize, ToSchema)]
#[diesel(table_name=connectors)]
pub struct ConnectorUpdate {
    pub kind: String,
    pub display_name: String,
    pub status: String,
}

impl Validate for ConnectorUpdate {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        required(&mut errors, "kind", &self.kind);
        max_len(&mut errors, "kind", &self.kind, 64);
        required(&mut errors, "display_name", &self.display_name);
        max_len(&mut errors, "display_name", &self.display_name, 128);
        required(&mut errors, "status", &self.status);
        max_len(&mut errors, "status", &self.status, 32);
        one_of(
            &mut errors,
            "status",
            &self.status,
            &["active", "paused", "error"],
        );

        errors
    }
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct ConnectorConfig {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub source: String,
    pub target: String,
    pub enabled: bool,
    pub schedule_cron: Option<String>,
    pub config: String,
    pub sample_payload: String,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
    #[serde(skip_deserializing)]
    pub updated_at: DateTime<Utc>,
    #[serde(skip_deserializing)]
    pub last_scheduled_at: Option<DateTime<Utc>>,
    #[serde(skip_deserializing)]
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_scheduled_run_id: Option<i32>,
}

#[derive(Insertable)]
#[diesel(table_name=connector_configs)]
pub struct NewConnectorConfig {
    pub source: String,
    pub target: String,
    pub enabled: bool,
    pub schedule_cron: Option<String>,
    pub config: String,
    pub sample_payload: String,
    pub last_scheduled_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_scheduled_run_id: Option<i32>,
}

#[derive(Deserialize, ToSchema)]
pub struct ConnectorConfigUpdate {
    pub target: String,
    #[serde(default = "default_connector_enabled")]
    pub enabled: bool,
    pub schedule_cron: Option<String>,
    #[serde(default = "default_connector_config")]
    pub config: String,
    #[serde(default = "default_connector_sample_payload")]
    pub sample_payload: String,
}

impl Validate for ConnectorConfigUpdate {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        required(&mut errors, "target", &self.target);
        max_len(&mut errors, "target", &self.target, 64);
        one_of(
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
        max_optional_len(&mut errors, "schedule_cron", &self.schedule_cron, 128);
        if let Some(schedule_cron) = &self.schedule_cron {
            match schedule_interval_seconds(schedule_cron) {
                Some(seconds) if seconds < MIN_CONNECTOR_SCHEDULE_INTERVAL_SECONDS => {
                    errors.push(FieldViolation::new(
                        "schedule_cron",
                        "must run no more often than once every 60 seconds",
                    ));
                }
                None => errors.push(FieldViolation::new(
                    "schedule_cron",
                    "must be @every <n>s, @every <n>m, @every <n>h, @hourly, or @daily",
                )),
                Some(_) => {}
            }
        }
        required(&mut errors, "config", &self.config);
        max_len(&mut errors, "config", &self.config, 65_535);
        required(&mut errors, "sample_payload", &self.sample_payload);
        max_len(
            &mut errors,
            "sample_payload",
            &self.sample_payload,
            1_000_000,
        );

        match serde_json::from_str::<serde_json::Value>(&self.config) {
            Ok(config) => crate::connector_config_validation::validate_connector_config_adapter(
                &mut errors,
                &self.target,
                &config,
            ),
            Err(_) => errors.push(FieldViolation::new("config", "must be valid JSON")),
        }

        match serde_json::from_str::<serde_json::Value>(&self.sample_payload) {
            Ok(value)
                if value
                    .get("items")
                    .and_then(|items| items.as_array())
                    .is_some() => {}
            Ok(_) => errors.push(FieldViolation::new(
                "sample_payload",
                "must include an items array",
            )),
            Err(_) => errors.push(FieldViolation::new("sample_payload", "must be valid JSON")),
        }

        errors
    }
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct ConnectorRun {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub source: String,
    pub target: String,
    pub status: String,
    pub success_count: i32,
    pub failure_count: i32,
    pub duration_ms: i64,
    pub error_message: Option<String>,
    #[serde(skip_deserializing)]
    pub started_at: DateTime<Utc>,
    #[serde(skip_deserializing)]
    pub finished_at: Option<DateTime<Utc>>,
    pub trigger: String,
    #[serde(skip_serializing, skip_deserializing)]
    pub payload: Option<String>,
    #[serde(skip_deserializing)]
    pub claimed_at: Option<DateTime<Utc>>,
    pub worker_id: Option<String>,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub next_attempt_at: DateTime<Utc>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub cancel_requested_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub parent_run_id: Option<i32>,
    pub snapshot_complete: Option<bool>,
    pub archived_count: i32,
}

#[derive(Insertable)]
#[diesel(table_name=connector_runs)]
pub struct NewConnectorRun {
    pub source: String,
    pub target: String,
    pub status: String,
    pub success_count: i32,
    pub failure_count: i32,
    pub duration_ms: i64,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub trigger: String,
    pub payload: Option<String>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub worker_id: Option<String>,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub next_attempt_at: DateTime<Utc>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub cancel_requested_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub parent_run_id: Option<i32>,
    pub snapshot_complete: Option<bool>,
    pub archived_count: i32,
}

pub struct ConnectorRunStateUpdate {
    pub status: String,
    pub success_count: i32,
    pub failure_count: i32,
    pub duration_ms: i64,
    pub error_message: Option<String>,
    pub finished_at: Option<DateTime<Utc>>,
    pub snapshot_complete: Option<bool>,
    pub archived_count: i32,
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct ConnectorRunItem {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub connector_run_id: i32,
    pub source: String,
    pub target: String,
    pub record_id: Option<i32>,
    pub external_id: Option<String>,
    pub status: String,
    pub snapshot: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
}

#[derive(Insertable)]
#[diesel(table_name=connector_run_items)]
pub struct NewConnectorRunItem {
    pub connector_run_id: i32,
    pub source: String,
    pub target: String,
    pub record_id: Option<i32>,
    pub external_id: Option<String>,
    pub status: String,
    pub snapshot: Option<String>,
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct ConnectorRunItemError {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub connector_run_id: i32,
    pub source: String,
    pub target: String,
    pub external_id: Option<String>,
    pub message: String,
    pub raw_item: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
}

#[derive(Insertable)]
#[diesel(table_name=connector_run_item_errors)]
pub struct NewConnectorRunItemError {
    pub connector_run_id: i32,
    pub source: String,
    pub target: String,
    pub external_id: Option<String>,
    pub message: String,
    pub raw_item: Option<String>,
}

#[derive(Queryable, Serialize, Deserialize, Clone, ToSchema)]
pub struct ConnectorWorker {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub worker_id: String,
    pub status: String,
    pub scheduler_enabled: bool,
    pub retention_enabled: bool,
    pub current_run_id: Option<i32>,
    pub last_error: Option<String>,
    #[serde(skip_deserializing)]
    pub started_at: DateTime<Utc>,
    #[serde(skip_deserializing)]
    pub last_seen_at: DateTime<Utc>,
    #[serde(skip_deserializing)]
    pub updated_at: DateTime<Utc>,
}

pub struct ConnectorWorkerHeartbeat {
    pub worker_id: String,
    pub status: String,
    pub scheduler_enabled: bool,
    pub retention_enabled: bool,
    pub current_run_id: Option<i32>,
    pub last_error: Option<String>,
    pub started_at: DateTime<Utc>,
}

#[derive(Queryable, Serialize, Deserialize, Clone, ToSchema)]
pub struct MaintenanceRun {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub task: String,
    pub status: String,
    pub worker_id: Option<String>,
    #[serde(skip_deserializing)]
    pub started_at: DateTime<Utc>,
    #[serde(skip_deserializing)]
    pub finished_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub health_checks_deleted: i32,
    pub connector_runs_deleted: i32,
    pub audit_logs_deleted: i32,
    pub error_message: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
}

#[derive(Insertable)]
#[diesel(table_name=maintenance_runs)]
pub struct NewMaintenanceRun {
    pub task: String,
    pub status: String,
    pub worker_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub health_checks_deleted: i32,
    pub connector_runs_deleted: i32,
    pub audit_logs_deleted: i32,
    pub error_message: Option<String>,
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct AuditLog {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub actor_user_id: Option<i32>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub metadata: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
}

#[derive(Insertable)]
#[diesel(table_name=audit_logs)]
pub struct NewAuditLog {
    pub actor_user_id: Option<i32>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub metadata: Option<String>,
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct Maintainer {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub display_name: String,
    pub email: String,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
}

#[derive(AsChangeset, Insertable, Deserialize, ToSchema)]
#[diesel(table_name=maintainers)]
pub struct NewMaintainer {
    pub display_name: String,
    pub email: String,
}

impl Validate for NewMaintainer {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        required(&mut errors, "display_name", &self.display_name);
        max_len(&mut errors, "display_name", &self.display_name, 255);
        required(&mut errors, "email", &self.email);
        max_len(&mut errors, "email", &self.email, 255);
        email(&mut errors, "email", &self.email);

        errors
    }
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct MaintainerMember {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub maintainer_id: i32,
    pub user_id: i32,
    pub role: String,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
}

#[derive(AsChangeset, Insertable, Deserialize, ToSchema)]
#[diesel(table_name=maintainer_members)]
pub struct NewMaintainerMember {
    pub maintainer_id: i32,
    pub user_id: i32,
    pub role: String,
}

impl Validate for NewMaintainerMember {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        positive(&mut errors, "maintainer_id", self.maintainer_id);
        positive(&mut errors, "user_id", self.user_id);
        required(&mut errors, "role", &self.role);
        max_len(&mut errors, "role", &self.role, 32);
        one_of(
            &mut errors,
            "role",
            &self.role,
            &["owner", "maintainer", "viewer"],
        );

        errors
    }
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct Package {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub maintainer_id: i32,
    pub slug: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
    pub status: String,
    pub repository_url: Option<String>,
    pub documentation_url: Option<String>,
    #[serde(skip_deserializing)]
    pub updated_at: DateTime<Utc>,
}

#[derive(AsChangeset, Insertable, Deserialize, ToSchema)]
#[diesel(table_name=packages)]
#[diesel(treat_none_as_null = true)]
pub struct NewPackage {
    pub maintainer_id: i32,
    pub slug: String,
    pub name: String,
    pub version: String,
    pub status: String,
    pub description: Option<String>,
    pub repository_url: Option<String>,
    pub documentation_url: Option<String>,
}

impl Validate for NewPackage {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        positive(&mut errors, "maintainer_id", self.maintainer_id);
        required(&mut errors, "slug", &self.slug);
        max_len(&mut errors, "slug", &self.slug, 64);
        required(&mut errors, "name", &self.name);
        max_len(&mut errors, "name", &self.name, 128);
        required(&mut errors, "version", &self.version);
        max_len(&mut errors, "version", &self.version, 64);
        required(&mut errors, "status", &self.status);
        max_len(&mut errors, "status", &self.status, 32);
        one_of(
            &mut errors,
            "status",
            &self.status,
            &["active", "deprecated", "archived"],
        );
        max_optional_len(&mut errors, "repository_url", &self.repository_url, 2048);
        optional_url(&mut errors, "repository_url", &self.repository_url);
        max_optional_len(
            &mut errors,
            "documentation_url",
            &self.documentation_url,
            2048,
        );
        optional_url(&mut errors, "documentation_url", &self.documentation_url);

        errors
    }
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct Service {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub maintainer_id: i32,
    pub slug: String,
    pub name: String,
    pub lifecycle_status: String,
    pub health_status: String,
    pub description: Option<String>,
    pub repository_url: Option<String>,
    pub dashboard_url: Option<String>,
    pub runbook_url: Option<String>,
    pub last_checked_at: Option<DateTime<Utc>>,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
    #[serde(skip_deserializing)]
    pub updated_at: DateTime<Utc>,
    pub source: String,
    pub external_id: Option<String>,
}

#[derive(Clone, Queryable, Serialize, Deserialize, ToSchema)]
pub struct ServiceHealthCheck {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub service_id: i32,
    pub connector_run_id: Option<i32>,
    pub source: String,
    pub external_id: Option<String>,
    pub health_status: String,
    pub previous_health_status: Option<String>,
    pub checked_at: DateTime<Utc>,
    pub response_time_ms: Option<i32>,
    pub message: Option<String>,
    #[serde(skip_serializing, skip_deserializing)]
    pub raw_payload: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
}

#[derive(Insertable)]
#[diesel(table_name=service_health_checks)]
pub struct NewServiceHealthCheck {
    pub service_id: i32,
    pub connector_run_id: Option<i32>,
    pub source: String,
    pub external_id: Option<String>,
    pub health_status: String,
    pub previous_health_status: Option<String>,
    pub checked_at: DateTime<Utc>,
    pub response_time_ms: Option<i32>,
    pub message: Option<String>,
    pub raw_payload: Option<String>,
}

#[derive(AsChangeset, Insertable, Deserialize, ToSchema)]
#[diesel(table_name=services)]
#[diesel(treat_none_as_null = true)]
pub struct NewService {
    #[serde(default = "default_manual_source")]
    pub source: String,
    pub external_id: Option<String>,
    pub maintainer_id: i32,
    pub slug: String,
    pub name: String,
    pub lifecycle_status: String,
    pub health_status: String,
    pub description: Option<String>,
    pub repository_url: Option<String>,
    pub dashboard_url: Option<String>,
    pub runbook_url: Option<String>,
    pub last_checked_at: Option<DateTime<Utc>>,
}

impl Validate for NewService {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        required(&mut errors, "source", &self.source);
        max_len(&mut errors, "source", &self.source, 64);
        max_optional_len(&mut errors, "external_id", &self.external_id, 128);
        positive(&mut errors, "maintainer_id", self.maintainer_id);
        required(&mut errors, "slug", &self.slug);
        max_len(&mut errors, "slug", &self.slug, 64);
        required(&mut errors, "name", &self.name);
        max_len(&mut errors, "name", &self.name, 128);
        required(&mut errors, "lifecycle_status", &self.lifecycle_status);
        max_len(&mut errors, "lifecycle_status", &self.lifecycle_status, 32);
        one_of(
            &mut errors,
            "lifecycle_status",
            &self.lifecycle_status,
            &["active", "deprecated", "archived"],
        );
        required(&mut errors, "health_status", &self.health_status);
        max_len(&mut errors, "health_status", &self.health_status, 32);
        one_of(
            &mut errors,
            "health_status",
            &self.health_status,
            &["healthy", "degraded", "down", "unknown"],
        );
        max_optional_len(&mut errors, "repository_url", &self.repository_url, 2048);
        optional_url(&mut errors, "repository_url", &self.repository_url);
        max_optional_len(&mut errors, "dashboard_url", &self.dashboard_url, 2048);
        optional_url(&mut errors, "dashboard_url", &self.dashboard_url);
        max_optional_len(&mut errors, "runbook_url", &self.runbook_url, 2048);
        optional_url(&mut errors, "runbook_url", &self.runbook_url);

        errors
    }
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct CalendarEvent {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub source: String,
    pub external_id: String,
    pub title: String,
    pub body: Option<String>,
    pub organizer: Option<String>,
    pub location: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub time_zone: Option<String>,
    pub is_all_day: bool,
    pub is_cancelled: bool,
    pub web_url: Option<String>,
    pub join_url: Option<String>,
    pub connector_id: Option<i32>,
    pub owner_user_id: Option<i32>,
    pub maintainer_id: Option<i32>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub last_seen_run_id: Option<i32>,
    pub archived_at: Option<DateTime<Utc>>,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
    #[serde(skip_deserializing)]
    pub updated_at: DateTime<Utc>,
}

#[derive(AsChangeset, Insertable, Deserialize, ToSchema)]
#[diesel(table_name=calendar_events)]
#[diesel(treat_none_as_null = true)]
pub struct NewCalendarEvent {
    pub source: String,
    pub external_id: String,
    pub title: String,
    pub body: Option<String>,
    pub organizer: Option<String>,
    pub location: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub time_zone: Option<String>,
    pub is_all_day: bool,
    pub is_cancelled: bool,
    pub web_url: Option<String>,
    pub join_url: Option<String>,
    #[serde(skip_deserializing)]
    pub connector_id: Option<i32>,
    #[serde(skip_deserializing)]
    pub owner_user_id: Option<i32>,
    #[serde(skip_deserializing)]
    pub maintainer_id: Option<i32>,
    #[serde(skip_deserializing)]
    pub source_updated_at: Option<DateTime<Utc>>,
    #[serde(skip_deserializing)]
    pub last_seen_run_id: Option<i32>,
    #[serde(skip_deserializing)]
    pub archived_at: Option<DateTime<Utc>>,
}

impl Validate for NewCalendarEvent {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        required(&mut errors, "source", &self.source);
        max_len(&mut errors, "source", &self.source, 64);
        required(&mut errors, "external_id", &self.external_id);
        max_len(&mut errors, "external_id", &self.external_id, 128);
        required(&mut errors, "title", &self.title);
        max_len(&mut errors, "title", &self.title, 256);
        max_optional_len(&mut errors, "organizer", &self.organizer, 256);
        max_optional_len(&mut errors, "location", &self.location, 256);
        max_optional_len(&mut errors, "time_zone", &self.time_zone, 128);
        max_optional_len(&mut errors, "web_url", &self.web_url, 2048);
        optional_url(&mut errors, "web_url", &self.web_url);
        max_optional_len(&mut errors, "join_url", &self.join_url, 2048);
        optional_url(&mut errors, "join_url", &self.join_url);
        if self.ends_at < self.starts_at {
            errors.push(FieldViolation::new(
                "ends_at",
                "must be greater than or equal to starts_at",
            ));
        }

        errors
    }
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct WorkCard {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub source: String,
    pub external_id: Option<String>,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assignee: Option<String>,
    pub project: Option<String>,
    pub work_item_type: Option<String>,
    pub assignee_source_id: Option<String>,
    pub assignee_user_id: Option<i32>,
    pub due_at: Option<DateTime<Utc>>,
    pub url: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
    #[serde(skip_deserializing)]
    pub updated_at: DateTime<Utc>,
    pub connector_id: Option<i32>,
    pub owner_user_id: Option<i32>,
    pub maintainer_id: Option<i32>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub last_seen_run_id: Option<i32>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(AsChangeset, Insertable, Deserialize, ToSchema)]
#[diesel(table_name=work_cards)]
#[diesel(treat_none_as_null = true)]
pub struct NewWorkCard {
    pub source: String,
    pub external_id: Option<String>,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assignee: Option<String>,
    pub project: Option<String>,
    pub work_item_type: Option<String>,
    #[serde(skip_deserializing)]
    pub assignee_source_id: Option<String>,
    #[serde(skip_deserializing)]
    pub assignee_user_id: Option<i32>,
    pub due_at: Option<DateTime<Utc>>,
    pub url: Option<String>,
    #[serde(skip_deserializing)]
    pub connector_id: Option<i32>,
    #[serde(skip_deserializing)]
    pub owner_user_id: Option<i32>,
    #[serde(skip_deserializing)]
    pub maintainer_id: Option<i32>,
    #[serde(skip_deserializing)]
    pub source_updated_at: Option<DateTime<Utc>>,
    #[serde(skip_deserializing)]
    pub last_seen_run_id: Option<i32>,
    #[serde(skip_deserializing)]
    pub archived_at: Option<DateTime<Utc>>,
}

impl Validate for NewWorkCard {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        required(&mut errors, "source", &self.source);
        max_len(&mut errors, "source", &self.source, 64);
        required(&mut errors, "title", &self.title);
        max_len(&mut errors, "title", &self.title, 255);
        required(&mut errors, "status", &self.status);
        max_len(&mut errors, "status", &self.status, 32);
        one_of(
            &mut errors,
            "status",
            &self.status,
            &["todo", "in_progress", "blocked", "done"],
        );
        required(&mut errors, "priority", &self.priority);
        max_len(&mut errors, "priority", &self.priority, 32);
        one_of(
            &mut errors,
            "priority",
            &self.priority,
            &["low", "medium", "high", "urgent"],
        );
        max_optional_len(&mut errors, "external_id", &self.external_id, 128);
        max_optional_len(&mut errors, "assignee", &self.assignee, 128);
        max_optional_len(&mut errors, "project", &self.project, 128);
        max_optional_len(&mut errors, "work_item_type", &self.work_item_type, 128);
        max_optional_len(
            &mut errors,
            "assignee_source_id",
            &self.assignee_source_id,
            512,
        );
        if self.assignee_user_id.is_some_and(|user_id| user_id <= 0) {
            errors.push(FieldViolation::new(
                "assignee_user_id",
                "must be a positive portal user id",
            ));
        }
        if self.assignee_user_id.is_some()
            && self
                .assignee_source_id
                .as_deref()
                .is_none_or(|source_id| source_id.trim().is_empty())
        {
            errors.push(FieldViolation::new(
                "assignee_source_id",
                "must identify the explicitly mapped source identity when assignee_user_id is set",
            ));
        }
        max_optional_len(&mut errors, "url", &self.url, 2048);
        optional_url(&mut errors, "url", &self.url);

        errors
    }
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct Notification {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub source: String,
    pub title: String,
    pub body: Option<String>,
    pub severity: String,
    pub is_read: bool,
    pub url: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
    #[serde(skip_deserializing)]
    pub updated_at: DateTime<Utc>,
    pub external_id: Option<String>,
    pub connector_id: Option<i32>,
    pub owner_user_id: Option<i32>,
    pub maintainer_id: Option<i32>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub last_seen_run_id: Option<i32>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, ToSchema)]
pub struct NotificationView {
    pub id: i32,
    pub source: String,
    pub title: String,
    pub body: Option<String>,
    pub severity: String,
    /// Effective read state for the current user. Source-level read state cannot be overridden.
    pub is_read: bool,
    /// Read state last imported from the source system.
    pub source_is_read: bool,
    pub url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub external_id: Option<String>,
    pub connector_id: Option<i32>,
    pub owner_user_id: Option<i32>,
    pub maintainer_id: Option<i32>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub last_seen_run_id: Option<i32>,
    pub archived_at: Option<DateTime<Utc>>,
    pub read_at: Option<DateTime<Utc>>,
    pub dismissed_at: Option<DateTime<Utc>>,
    pub snoozed_until: Option<DateTime<Utc>>,
}

impl NotificationView {
    pub fn from_record(notification: Notification, receipt: Option<NotificationReceipt>) -> Self {
        let (read_at, dismissed_at, snoozed_until) = receipt
            .map(|receipt| (receipt.read_at, receipt.dismissed_at, receipt.snoozed_until))
            .unwrap_or((None, None, None));
        let source_is_read = notification.is_read;

        Self {
            id: notification.id,
            source: notification.source,
            title: notification.title,
            body: notification.body,
            severity: notification.severity,
            is_read: source_is_read || read_at.is_some(),
            source_is_read,
            url: notification.url,
            created_at: notification.created_at,
            updated_at: notification.updated_at,
            external_id: notification.external_id,
            connector_id: notification.connector_id,
            owner_user_id: notification.owner_user_id,
            maintainer_id: notification.maintainer_id,
            source_updated_at: notification.source_updated_at,
            last_seen_run_id: notification.last_seen_run_id,
            archived_at: notification.archived_at,
            read_at,
            dismissed_at,
            snoozed_until,
        }
    }
}

#[derive(AsChangeset, Insertable, Deserialize, ToSchema)]
#[diesel(table_name=notifications)]
#[diesel(treat_none_as_null = true)]
pub struct NewNotification {
    pub source: String,
    pub external_id: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub severity: String,
    pub is_read: bool,
    pub url: Option<String>,
    #[serde(skip_deserializing)]
    pub connector_id: Option<i32>,
    #[serde(skip_deserializing)]
    pub owner_user_id: Option<i32>,
    #[serde(skip_deserializing)]
    pub maintainer_id: Option<i32>,
    #[serde(skip_deserializing)]
    pub source_updated_at: Option<DateTime<Utc>>,
    #[serde(skip_deserializing)]
    pub last_seen_run_id: Option<i32>,
    #[serde(skip_deserializing)]
    pub archived_at: Option<DateTime<Utc>>,
}

impl Validate for NewNotification {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        required(&mut errors, "source", &self.source);
        max_len(&mut errors, "source", &self.source, 64);
        max_optional_len(&mut errors, "external_id", &self.external_id, 128);
        required(&mut errors, "title", &self.title);
        max_len(&mut errors, "title", &self.title, 255);
        required(&mut errors, "severity", &self.severity);
        max_len(&mut errors, "severity", &self.severity, 32);
        one_of(
            &mut errors,
            "severity",
            &self.severity,
            &["info", "warning", "critical"],
        );
        max_optional_len(&mut errors, "url", &self.url, 2048);
        optional_url(&mut errors, "url", &self.url);

        errors
    }
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct NotificationReceipt {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub notification_id: i32,
    pub user_id: i32,
    pub read_at: Option<DateTime<Utc>>,
    pub dismissed_at: Option<DateTime<Utc>>,
    pub snoozed_until: Option<DateTime<Utc>>,
    #[serde(skip_deserializing)]
    pub created_at: DateTime<Utc>,
    #[serde(skip_deserializing)]
    pub updated_at: DateTime<Utc>,
}

#[derive(Queryable, Debug, Identifiable)]
pub struct User {
    pub id: i32,
    pub username: String,
    pub password: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Insertable)]
#[diesel(table_name=users)]
pub struct NewUser {
    pub username: String,
    pub password: String,
}

#[derive(Queryable, Associations, Identifiable, Debug)]
#[diesel(belongs_to(User))]
#[diesel(table_name=external_identities)]
pub struct ExternalIdentity {
    pub id: i32,
    pub user_id: i32,
    pub provider: String,
    pub issuer: String,
    pub subject: Option<String>,
    pub tenant_id: String,
    pub object_id: String,
    pub preferred_username: Option<String>,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name=external_identities)]
pub struct NewExternalIdentity {
    pub user_id: i32,
    pub provider: String,
    pub issuer: String,
    pub subject: Option<String>,
    pub tenant_id: String,
    pub object_id: String,
    pub preferred_username: Option<String>,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub last_login_at: Option<DateTime<Utc>>,
}

#[derive(Queryable, Identifiable, Debug)]
pub struct Role {
    pub id: i32,
    pub code: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Insertable)]
#[diesel(table_name=roles)]
pub struct NewRole {
    pub code: String,
    pub name: String,
}

#[derive(Queryable, Associations, Identifiable, Debug)]
#[diesel(belongs_to(User))]
#[diesel(belongs_to(Role))]
#[diesel(table_name=users_roles)]
pub struct UserRole {
    pub id: i32,
    pub user_id: i32,
    pub role_id: i32,
}

#[derive(Insertable)]
#[diesel(table_name=users_roles)]
pub struct NewUserRole {
    pub user_id: i32,
    pub role_id: i32,
}

#[derive(Queryable, Associations, Identifiable, Debug)]
#[diesel(belongs_to(User))]
#[diesel(table_name=sessions)]
pub struct Session {
    pub id: i32,
    pub user_id: i32,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub auth_method: String,
    pub last_seen_at: DateTime<Utc>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name=sessions)]
pub struct NewSession {
    pub user_id: i32,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub auth_method: String,
    pub last_seen_at: DateTime<Utc>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Queryable, Identifiable)]
#[diesel(table_name=login_throttle_buckets)]
#[diesel(primary_key(bucket_hash))]
pub struct LoginThrottleBucket {
    pub bucket_hash: String,
    pub failure_count: i32,
    pub window_started_at: DateTime<Utc>,
    pub locked_until: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Insertable)]
#[diesel(table_name=login_throttle_buckets)]
pub struct NewLoginThrottleBucket {
    pub bucket_hash: String,
    pub failure_count: i32,
    pub window_started_at: DateTime<Utc>,
    pub locked_until: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Queryable, Identifiable, Debug)]
#[diesel(table_name=oidc_login_transactions)]
#[diesel(primary_key(state_hash))]
pub struct OidcLoginTransaction {
    pub state_hash: String,
    pub browser_binding_hash: String,
    pub nonce: String,
    pub pkce_verifier_ciphertext: String,
    pub return_to: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name=oidc_login_transactions)]
pub struct NewOidcLoginTransaction {
    pub state_hash: String,
    pub browser_binding_hash: String,
    pub nonce: String,
    pub pkce_verifier_ciphertext: String,
    pub return_to: String,
    pub expires_at: DateTime<Utc>,
}

#[cfg(test)]
mod schedule_tests {
    use super::*;
    use chrono::TimeZone;

    fn connector_config(schedule: &str) -> ConnectorConfigUpdate {
        ConnectorConfigUpdate {
            target: "work_cards".to_owned(),
            enabled: true,
            schedule_cron: Some(schedule.to_owned()),
            config: "{}".to_owned(),
            sample_payload: r#"{"items":[]}"#.to_owned(),
        }
    }

    #[test]
    fn connector_schedule_validation_rejects_sub_minute_intervals() {
        let violations = connector_config("@every 59s").validate();
        assert!(violations.iter().any(|violation| {
            violation.field == "schedule_cron" && violation.message.contains("60 seconds")
        }));

        assert!(!connector_config("@every 60s")
            .validate()
            .iter()
            .any(|violation| violation.field == "schedule_cron"));
    }

    #[test]
    fn legacy_schedule_intervals_are_clamped_and_overflow_is_rejected() {
        assert_eq!(effective_schedule_interval_seconds("@every 1s"), Some(60));
        assert_eq!(effective_schedule_interval_seconds("@every 5m"), Some(300));
        assert_eq!(
            schedule_interval_seconds("@every 9223372036854775807h"),
            None
        );
    }

    #[test]
    fn persisted_model_timestamps_serialize_as_utc_rfc3339() {
        let audit_log = AuditLog {
            id: 7,
            actor_user_id: None,
            action: "timezone.test".to_owned(),
            resource_type: "test".to_owned(),
            resource_id: None,
            metadata: None,
            created_at: Utc.with_ymd_and_hms(2026, 7, 12, 12, 30, 0).unwrap(),
        };

        let value = serde_json::to_value(audit_log).unwrap();
        assert_eq!(value["created_at"], "2026-07-12T12:30:00Z");
    }
}
