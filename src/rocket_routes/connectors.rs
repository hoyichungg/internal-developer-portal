mod oauth;
mod routes;
pub(crate) mod runtime;
pub(crate) mod shared;
pub(crate) mod types;
pub(crate) mod worker;

pub use oauth::{finish_microsoft_oauth, microsoft_oauth_callback_page, start_microsoft_oauth};
pub use routes::{
    cancel_connector_run, create_connector, delete_connector, get_connector_config,
    get_connector_operations, get_connector_run, get_connector_runs, get_connectors,
    import_calendar_events, import_notifications, import_service_health, import_work_cards,
    retry_connector_run, run_connector, update_connector, update_connector_scope,
    upsert_connector_config, view_connector,
};
pub use types::{
    CalendarEventImportItem, CalendarEventImportRequest, ConnectorConfigResponse,
    ConnectorImportError, ConnectorOperationsResponse, ConnectorRunDetail,
    ConnectorRunExecutionResponse, ConnectorWorkerStatus, ManualConnectorRunRequest,
    MicrosoftOAuthAuthorizeRequest, MicrosoftOAuthAuthorizeResponse, MicrosoftOAuthCallbackRequest,
    MicrosoftOAuthCallbackResponse, NotificationImportItem, NotificationImportRequest,
    ServiceHealthImportItem, ServiceHealthImportRequest, WorkCardImportItem, WorkCardImportRequest,
};
pub(crate) use worker::connector_worker_stale_after_seconds;
#[doc(hidden)]
pub use worker::{
    claim_connector_run_for_test, recover_connector_runs_for_test,
    request_connector_run_cancel_for_test, run_guarded_retention_cleanup_for_test,
};
pub use worker::{run_connector_worker_forever, spawn_connector_background_worker};
