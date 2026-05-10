use crate::schema::*;
use chrono::NaiveDateTime;
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
        .map(|seconds| seconds * multiplier)
}

#[derive(Queryable, Serialize, Deserialize, ToSchema)]
pub struct Connector {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub source: String,
    pub kind: String,
    pub display_name: String,
    pub status: String,
    pub last_run_at: Option<NaiveDateTime>,
    pub last_success_at: Option<NaiveDateTime>,
    #[serde(skip_deserializing)]
    pub created_at: NaiveDateTime,
    #[serde(skip_deserializing)]
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable, Deserialize, ToSchema)]
#[diesel(table_name=connectors)]
pub struct NewConnector {
    pub source: String,
    pub kind: String,
    pub display_name: String,
    pub status: String,
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

        errors
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
    pub created_at: NaiveDateTime,
    #[serde(skip_deserializing)]
    pub updated_at: NaiveDateTime,
    #[serde(skip_deserializing)]
    pub last_scheduled_at: Option<NaiveDateTime>,
    #[serde(skip_deserializing)]
    pub next_run_at: Option<NaiveDateTime>,
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
    pub last_scheduled_at: Option<NaiveDateTime>,
    pub next_run_at: Option<NaiveDateTime>,
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
            &["service_health", "work_cards", "notifications"],
        );
        max_optional_len(&mut errors, "schedule_cron", &self.schedule_cron, 128);
        if let Some(schedule_cron) = &self.schedule_cron {
            if schedule_interval_seconds(schedule_cron).is_none() {
                errors.push(FieldViolation::new(
                    "schedule_cron",
                    "must be @every <n>s, @every <n>m, @every <n>h, @hourly, or @daily",
                ));
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

        if serde_json::from_str::<serde_json::Value>(&self.config).is_err() {
            errors.push(FieldViolation::new("config", "must be valid JSON"));
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
    pub started_at: NaiveDateTime,
    #[serde(skip_deserializing)]
    pub finished_at: Option<NaiveDateTime>,
    pub trigger: String,
    #[serde(skip_serializing, skip_deserializing)]
    pub payload: Option<String>,
    #[serde(skip_deserializing)]
    pub claimed_at: Option<NaiveDateTime>,
    pub worker_id: Option<String>,
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
    pub started_at: NaiveDateTime,
    pub finished_at: Option<NaiveDateTime>,
    pub trigger: String,
    pub payload: Option<String>,
    pub claimed_at: Option<NaiveDateTime>,
    pub worker_id: Option<String>,
}

pub struct ConnectorRunStateUpdate {
    pub status: String,
    pub success_count: i32,
    pub failure_count: i32,
    pub duration_ms: i64,
    pub error_message: Option<String>,
    pub finished_at: Option<NaiveDateTime>,
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
    pub created_at: NaiveDateTime,
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
    pub created_at: NaiveDateTime,
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
    pub started_at: NaiveDateTime,
    #[serde(skip_deserializing)]
    pub last_seen_at: NaiveDateTime,
    #[serde(skip_deserializing)]
    pub updated_at: NaiveDateTime,
}

pub struct ConnectorWorkerHeartbeat {
    pub worker_id: String,
    pub status: String,
    pub scheduler_enabled: bool,
    pub retention_enabled: bool,
    pub current_run_id: Option<i32>,
    pub last_error: Option<String>,
    pub started_at: NaiveDateTime,
}

#[derive(Queryable, Serialize, Deserialize, Clone, ToSchema)]
pub struct MaintenanceRun {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub task: String,
    pub status: String,
    pub worker_id: Option<String>,
    #[serde(skip_deserializing)]
    pub started_at: NaiveDateTime,
    #[serde(skip_deserializing)]
    pub finished_at: NaiveDateTime,
    pub duration_ms: i64,
    pub health_checks_deleted: i32,
    pub connector_runs_deleted: i32,
    pub audit_logs_deleted: i32,
    pub error_message: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name=maintenance_runs)]
pub struct NewMaintenanceRun {
    pub task: String,
    pub status: String,
    pub worker_id: Option<String>,
    pub started_at: NaiveDateTime,
    pub finished_at: NaiveDateTime,
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
    pub created_at: NaiveDateTime,
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
    pub created_at: NaiveDateTime,
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
    pub created_at: NaiveDateTime,
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
    pub created_at: NaiveDateTime,
    pub status: String,
    pub repository_url: Option<String>,
    pub documentation_url: Option<String>,
    #[serde(skip_deserializing)]
    pub updated_at: NaiveDateTime,
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
    pub last_checked_at: Option<NaiveDateTime>,
    #[serde(skip_deserializing)]
    pub created_at: NaiveDateTime,
    #[serde(skip_deserializing)]
    pub updated_at: NaiveDateTime,
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
    pub checked_at: NaiveDateTime,
    pub response_time_ms: Option<i32>,
    pub message: Option<String>,
    #[serde(skip_serializing, skip_deserializing)]
    pub raw_payload: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: NaiveDateTime,
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
    pub checked_at: NaiveDateTime,
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
    pub last_checked_at: Option<NaiveDateTime>,
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
pub struct WorkCard {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub source: String,
    pub external_id: Option<String>,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assignee: Option<String>,
    pub due_at: Option<NaiveDateTime>,
    pub url: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: NaiveDateTime,
    #[serde(skip_deserializing)]
    pub updated_at: NaiveDateTime,
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
    pub due_at: Option<NaiveDateTime>,
    pub url: Option<String>,
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
    pub created_at: NaiveDateTime,
    #[serde(skip_deserializing)]
    pub updated_at: NaiveDateTime,
    pub external_id: Option<String>,
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

#[derive(Queryable, Debug, Identifiable)]
pub struct User {
    pub id: i32,
    pub username: String,
    pub password: String,
    pub created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name=users)]
pub struct NewUser {
    pub username: String,
    pub password: String,
}

#[derive(Queryable, Identifiable, Debug)]
pub struct Role {
    pub id: i32,
    pub code: String,
    pub name: String,
    pub created_at: NaiveDateTime,
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
    pub token: String,
    pub expires_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name=sessions)]
pub struct NewSession {
    pub user_id: i32,
    pub token: String,
    pub expires_at: NaiveDateTime,
}
