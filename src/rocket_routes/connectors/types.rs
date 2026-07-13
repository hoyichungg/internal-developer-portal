use chrono::{DateTime, Utc};
use rocket::serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::crypto::redact_connector_config;
use crate::models::{
    ConnectorConfig, ConnectorRun, ConnectorRunItem, ConnectorRunItemError, ConnectorWorker,
    MaintenanceRun, ServiceHealthCheck,
};
use crate::validation::{FieldViolation, Validate};

fn default_run_mode() -> String {
    "execute".to_owned()
}

#[derive(Serialize, ToSchema)]
pub struct ConnectorRunExecutionResponse {
    pub source: String,
    pub target: String,
    pub imported: usize,
    pub failed: usize,
    pub run: ConnectorRun,
    pub data: Vec<Value>,
    pub items: Vec<ConnectorRunItem>,
    pub errors: Vec<ConnectorImportError>,
    pub item_errors: Vec<ConnectorRunItemError>,
}

#[derive(Serialize, ToSchema)]
pub struct ConnectorRunDetail {
    pub run: ConnectorRun,
    pub items: Vec<ConnectorRunItem>,
    pub item_errors: Vec<ConnectorRunItemError>,
    pub health_checks: Vec<ServiceHealthCheck>,
}

#[derive(Serialize, ToSchema)]
pub struct ConnectorOperationsResponse {
    pub stale_after_seconds: i64,
    pub workers: Vec<ConnectorWorkerStatus>,
    pub maintenance_runs: Vec<MaintenanceRun>,
}

#[derive(Serialize, ToSchema)]
pub struct ConnectorWorkerStatus {
    pub id: i32,
    pub worker_id: String,
    pub status: String,
    pub scheduler_enabled: bool,
    pub retention_enabled: bool,
    pub current_run_id: Option<i32>,
    pub last_error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub seconds_since_last_seen: i64,
    pub is_stale: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ConnectorConfigResponse {
    pub id: i32,
    pub source: String,
    pub target: String,
    pub enabled: bool,
    pub schedule_cron: Option<String>,
    pub config: String,
    pub sample_payload: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_scheduled_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_scheduled_run_id: Option<i32>,
}

#[derive(Deserialize, ToSchema)]
pub struct MicrosoftOAuthAuthorizeRequest {
    pub redirect_uri: String,
    pub prompt: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct MicrosoftOAuthAuthorizeResponse {
    pub authorization_url: String,
    pub state: String,
    pub redirect_uri: String,
    pub scope: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Deserialize, ToSchema)]
pub struct MicrosoftOAuthCallbackRequest {
    pub code: Option<String>,
    pub state: String,
    pub redirect_uri: String,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct MicrosoftOAuthCallbackResponse {
    pub source: String,
    pub config: ConnectorConfigResponse,
}

#[derive(Serialize, ToSchema)]
pub struct ConnectorImportError {
    pub external_id: Option<String>,
    pub message: String,
    pub raw_item: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct ManualConnectorRunRequest {
    #[serde(default = "default_run_mode")]
    pub mode: String,
    pub target: Option<String>,
    pub payload: Option<Value>,
}

impl Validate for MicrosoftOAuthAuthorizeRequest {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        crate::validation::required(&mut errors, "redirect_uri", &self.redirect_uri);
        crate::validation::max_len(&mut errors, "redirect_uri", &self.redirect_uri, 512);
        if !(self.redirect_uri.starts_with("http://") || self.redirect_uri.starts_with("https://"))
        {
            errors.push(FieldViolation::new(
                "redirect_uri",
                "must be an absolute HTTP URL",
            ));
        }
        crate::validation::max_optional_len(&mut errors, "prompt", &self.prompt, 64);

        errors
    }
}

impl Validate for MicrosoftOAuthCallbackRequest {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        crate::validation::required(&mut errors, "state", &self.state);
        crate::validation::max_len(&mut errors, "state", &self.state, 1024);
        crate::validation::required(&mut errors, "redirect_uri", &self.redirect_uri);
        crate::validation::max_len(&mut errors, "redirect_uri", &self.redirect_uri, 512);
        if !(self.redirect_uri.starts_with("http://") || self.redirect_uri.starts_with("https://"))
        {
            errors.push(FieldViolation::new(
                "redirect_uri",
                "must be an absolute HTTP URL",
            ));
        }
        if let Some(code) = &self.code {
            crate::validation::max_len(&mut errors, "code", code, 8192);
        }
        crate::validation::max_optional_len(&mut errors, "error", &self.error, 256);
        crate::validation::max_optional_len(
            &mut errors,
            "error_description",
            &self.error_description,
            2048,
        );

        errors
    }
}

impl Validate for ManualConnectorRunRequest {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        crate::validation::required(&mut errors, "mode", &self.mode);
        crate::validation::max_len(&mut errors, "mode", &self.mode, 32);
        crate::validation::one_of(&mut errors, "mode", &self.mode, &["execute", "queue"]);

        if let Some(target) = &self.target {
            crate::validation::max_len(&mut errors, "target", target, 64);
            crate::validation::one_of(
                &mut errors,
                "target",
                target,
                &[
                    "service_health",
                    "work_cards",
                    "notifications",
                    "calendar_events",
                ],
            );
        }

        errors
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CalendarEventImportRequest {
    /// Calendar events to import. Items are upserted by `(source, external_id)`.
    pub items: Vec<CalendarEventImportItem>,
    /// `true` only when `items` is the complete configured calendar window.
    #[serde(default)]
    pub snapshot_complete: Option<bool>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CalendarEventImportItem {
    pub external_id: String,
    pub title: String,
    pub body: Option<String>,
    pub organizer: Option<String>,
    pub location: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub time_zone: Option<String>,
    pub is_all_day: bool,
    pub is_cancelled: bool,
    pub web_url: Option<String>,
    pub join_url: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct WorkCardImportRequest {
    /// Work card records to import. Items are upserted by `(source, external_id)`.
    pub items: Vec<WorkCardImportItem>,
    /// `true` only when `items` is the complete source snapshot. Missing records are archived
    /// after every item imports successfully. Omitted/false payloads never archive records.
    #[serde(default)]
    pub snapshot_complete: Option<bool>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct WorkCardImportItem {
    /// Stable id from the external work tracking system.
    pub external_id: String,
    /// Work item title displayed on the dashboard.
    pub title: String,
    /// Supported values: `todo`, `in_progress`, `blocked`, `done`.
    pub status: String,
    /// Supported values: `low`, `medium`, `high`, `urgent`.
    pub priority: String,
    /// Optional assignee display name or email.
    pub assignee: Option<String>,
    /// Optional source project name.
    pub project: Option<String>,
    /// Optional source work item type, for example `Bug` or `User Story`.
    pub work_item_type: Option<String>,
    /// Stable assignee identity descriptor from the source. Display names and email addresses
    /// are never identity keys.
    pub assignee_source_id: Option<String>,
    /// Portal user id resolved through an explicit connector mapping.
    pub assignee_user_id: Option<i32>,
    /// Optional due instant in RFC3339 format with an explicit UTC offset.
    pub due_at: Option<DateTime<Utc>>,
    /// Optional last-change instant reported by the source.
    pub source_updated_at: Option<DateTime<Utc>>,
    /// Optional absolute URL back to the source system.
    pub url: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct NotificationImportRequest {
    /// Notification records to import. Items are upserted by `(source, external_id)`.
    pub items: Vec<NotificationImportItem>,
    /// `true` only when `items` is the complete source snapshot. Missing records are archived
    /// after every item imports successfully. Omitted/false payloads never archive records.
    #[serde(default)]
    pub snapshot_complete: Option<bool>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct NotificationImportItem {
    /// Stable id from the external notification source.
    pub external_id: String,
    /// Notification title displayed in the portal.
    pub title: String,
    /// Optional notification body.
    pub body: Option<String>,
    /// Supported values: `info`, `warning`, `critical`.
    pub severity: String,
    /// Initial read state.
    pub is_read: bool,
    /// Optional absolute URL back to the source system.
    pub url: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct ServiceHealthImportRequest {
    /// Service health records to import. Each item upserts a service and appends a health check.
    pub items: Vec<ServiceHealthImportItem>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct ServiceHealthImportItem {
    /// Stable id from the external monitoring source.
    pub external_id: String,
    /// Existing maintainer/team id that owns the imported service.
    pub maintainer_id: i32,
    /// Stable portal service slug.
    pub slug: String,
    /// Human-friendly service name.
    pub name: String,
    /// Supported values: `active`, `deprecated`, `archived`.
    pub lifecycle_status: String,
    /// Supported values: `healthy`, `degraded`, `down`, `unknown`.
    pub health_status: String,
    /// Optional service description.
    pub description: Option<String>,
    /// Optional absolute repository URL.
    pub repository_url: Option<String>,
    /// Optional absolute dashboard URL.
    pub dashboard_url: Option<String>,
    /// Optional absolute runbook URL.
    pub runbook_url: Option<String>,
    /// Optional source check instant in RFC3339 format with an explicit UTC offset.
    pub last_checked_at: Option<DateTime<Utc>>,
}

pub(crate) struct ConnectorExecution {
    pub(crate) data: Vec<Value>,
    pub(crate) items: Vec<ConnectorRunItemDraft>,
    pub(crate) errors: Vec<ConnectorImportError>,
    pub(crate) snapshot_complete: Option<bool>,
    pub(crate) archived_count: usize,
}

pub(crate) struct ConnectorRunItemDraft {
    pub(crate) external_id: Option<String>,
    pub(crate) record_id: Option<i32>,
    pub(crate) status: &'static str,
    pub(crate) snapshot: Option<String>,
}

impl From<ConnectorConfig> for ConnectorConfigResponse {
    fn from(config: ConnectorConfig) -> Self {
        Self {
            id: config.id,
            source: config.source,
            target: config.target,
            enabled: config.enabled,
            schedule_cron: config.schedule_cron,
            config: redact_connector_config(&config.config),
            sample_payload: config.sample_payload,
            created_at: config.created_at,
            updated_at: config.updated_at,
            last_scheduled_at: config.last_scheduled_at,
            next_run_at: config.next_run_at,
            last_scheduled_run_id: config.last_scheduled_run_id,
        }
    }
}

impl ConnectorWorkerStatus {
    pub(crate) fn from_worker(
        worker: ConnectorWorker,
        now: DateTime<Utc>,
        stale_after_seconds: i64,
    ) -> Self {
        let seconds_since_last_seen = (now - worker.last_seen_at).num_seconds().max(0);

        Self {
            id: worker.id,
            worker_id: worker.worker_id,
            status: worker.status,
            scheduler_enabled: worker.scheduler_enabled,
            retention_enabled: worker.retention_enabled,
            current_run_id: worker.current_run_id,
            last_error: worker.last_error,
            started_at: worker.started_at,
            last_seen_at: worker.last_seen_at,
            updated_at: worker.updated_at,
            seconds_since_last_seen,
            is_stale: seconds_since_last_seen > stale_after_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CalendarEventImportItem, ServiceHealthImportItem, WorkCardImportItem};
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    fn event_with_times(starts_at: &str, ends_at: &str) -> serde_json::Value {
        json!({
            "external_id": "event-1",
            "title": "Timezone review",
            "body": null,
            "organizer": null,
            "location": null,
            "starts_at": starts_at,
            "ends_at": ends_at,
            "time_zone": "Taipei Standard Time",
            "is_all_day": false,
            "is_cancelled": false,
            "web_url": null,
            "join_url": null
        })
    }

    #[test]
    fn connector_rfc3339_offsets_are_normalized_to_utc() {
        let item: CalendarEventImportItem = serde_json::from_value(event_with_times(
            "2026-07-12T20:30:00+08:00",
            "2026-07-12T13:00:00Z",
        ))
        .unwrap();

        assert_eq!(
            item.starts_at.unwrap(),
            Utc.with_ymd_and_hms(2026, 7, 12, 12, 30, 0).unwrap()
        );
        let serialized = serde_json::to_value(item).unwrap();
        assert_eq!(serialized["ends_at"], "2026-07-12T13:00:00Z");
    }

    #[test]
    fn public_connector_inputs_reject_timezone_less_datetimes() {
        let calendar = serde_json::from_value::<CalendarEventImportItem>(event_with_times(
            "2026-07-12T12:30:00",
            "2026-07-12 13:00:00",
        ));
        let work_card = serde_json::from_value::<WorkCardImportItem>(json!({
            "external_id": "work-1",
            "title": "Timezone migration",
            "status": "todo",
            "priority": "high",
            "assignee": null,
            "due_at": "2026-07-12T14:00:00",
            "url": null
        }));
        let service = serde_json::from_value::<ServiceHealthImportItem>(json!({
            "external_id": "service-1",
            "maintainer_id": 1,
            "slug": "service-1",
            "name": "Service One",
            "lifecycle_status": "active",
            "health_status": "healthy",
            "description": null,
            "repository_url": null,
            "dashboard_url": null,
            "runbook_url": null,
            "last_checked_at": "2026-07-12 15:00:00"
        }));

        assert!(calendar.is_err());
        assert!(work_card.is_err());
        assert!(service.is_err());
    }
}
