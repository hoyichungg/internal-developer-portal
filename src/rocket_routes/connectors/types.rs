use chrono::NaiveDateTime;
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
    pub started_at: NaiveDateTime,
    pub last_seen_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
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
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub last_scheduled_at: Option<NaiveDateTime>,
    pub next_run_at: Option<NaiveDateTime>,
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
    pub expires_at: NaiveDateTime,
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
                &["service_health", "work_cards", "notifications"],
            );
        }

        errors
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct WorkCardImportRequest {
    /// Work card records to import. Items are upserted by `(source, external_id)`.
    pub items: Vec<WorkCardImportItem>,
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
    /// Optional due date/time in local NaiveDateTime JSON format.
    pub due_at: Option<NaiveDateTime>,
    /// Optional absolute URL back to the source system.
    pub url: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct NotificationImportRequest {
    /// Notification records to import. Items are upserted by `(source, external_id)`.
    pub items: Vec<NotificationImportItem>,
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
    /// Optional source check time. RFC3339 strings and `%Y-%m-%d %H:%M:%S` are accepted.
    pub last_checked_at: Option<String>,
}

pub(crate) struct ConnectorExecution {
    pub(crate) data: Vec<Value>,
    pub(crate) items: Vec<ConnectorRunItemDraft>,
    pub(crate) errors: Vec<ConnectorImportError>,
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
        now: NaiveDateTime,
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
