#![allow(dead_code)]

use rocket::serde::json::Json;
use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa::openapi::OpenApi;
use utoipa::{Modify, OpenApi as OpenApiDerive};

use crate::api::{ApiErrorResponse, ApiResponse};
use crate::models::{
    AuditLog, Connector, ConnectorConfigUpdate, ConnectorRun, ConnectorRunItem,
    ConnectorRunItemError, ConnectorUpdate, Maintainer, MaintainerMember, NewConnector,
    NewMaintainer, NewNotification, NewPackage, NewService, NewWorkCard, Notification, Package,
    Service, ServiceHealthCheck, WorkCard,
};
use crate::rocket_routes::authorization::{
    Credentials, LoginResponse, MeOverviewResponse, MeResponse, UserSummary,
};
use crate::rocket_routes::connectors::{
    ConnectorConfigResponse, ConnectorImportError, ConnectorOperationsResponse, ConnectorRunDetail,
    ConnectorRunExecutionResponse, ConnectorWorkerStatus, ManualConnectorRunRequest,
    NotificationImportItem, NotificationImportRequest, ServiceHealthImportItem,
    ServiceHealthImportRequest, WorkCardImportItem, WorkCardImportRequest,
};
use crate::rocket_routes::dashboard::{
    DashboardResponse, DashboardScope, DashboardSummary, ServiceHealthHistory,
    ServiceHealthHistorySummary,
};
use crate::rocket_routes::health::HealthResponse;
use crate::rocket_routes::maintainers::MaintainerMemberRequest;
use crate::rocket_routes::services::{
    ServiceHealthOverview, ServiceLinks, ServiceOverview, ServiceOwner,
};
use crate::validation::FieldViolation;

#[derive(OpenApiDerive)]
#[openapi(
    info(
        title = "Internal Developer Portal API",
        version = "0.1.0",
        description = "Backend API for the Internal Developer Portal. JSON success responses are wrapped as `{ data: ... }`; structured errors are returned as `{ error: { code, message, details? } }`."
    ),
    paths(
        openapi_json_doc,
        health_doc,
        login_doc,
        logout_doc,
        me_doc,
        list_users_doc,
        me_overview_doc,
        dashboard_doc,
        list_connectors_doc,
        connector_operations_doc,
        get_connector_doc,
        create_connector_doc,
        update_connector_doc,
        delete_connector_doc,
        get_connector_config_doc,
        upsert_connector_config_doc,
        list_connector_runs_doc,
        get_connector_run_doc,
        retry_connector_run_doc,
        run_connector_doc,
        import_work_cards_doc,
        import_notifications_doc,
        import_service_health_doc,
        list_maintainers_doc,
        get_maintainer_doc,
        create_maintainer_doc,
        update_maintainer_doc,
        delete_maintainer_doc,
        list_maintainer_members_doc,
        upsert_maintainer_member_doc,
        delete_maintainer_member_doc,
        list_services_doc,
        get_service_doc,
        get_service_overview_doc,
        create_service_doc,
        update_service_doc,
        delete_service_doc,
        list_packages_doc,
        get_package_doc,
        create_package_doc,
        update_package_doc,
        delete_package_doc,
        list_work_cards_doc,
        get_work_card_doc,
        create_work_card_doc,
        update_work_card_doc,
        delete_work_card_doc,
        list_notifications_doc,
        get_notification_doc,
        create_notification_doc,
        update_notification_doc,
        delete_notification_doc,
        list_audit_logs_doc
    ),
    components(schemas(
        ApiErrorResponse,
        ApiResponse<AuditLog>,
        ApiResponse<Connector>,
        ApiResponse<ConnectorConfigResponse>,
        ApiResponse<ConnectorOperationsResponse>,
        ApiResponse<ConnectorRun>,
        ApiResponse<ConnectorRunDetail>,
        ApiResponse<ConnectorRunExecutionResponse>,
        ApiResponse<DashboardResponse>,
        ApiResponse<HealthResponse>,
        ApiResponse<LoginResponse>,
        ApiResponse<Maintainer>,
        ApiResponse<MaintainerMember>,
        ApiResponse<MeOverviewResponse>,
        ApiResponse<MeResponse>,
        ApiResponse<Notification>,
        ApiResponse<Package>,
        ApiResponse<Service>,
        ApiResponse<ServiceOverview>,
        ApiResponse<WorkCard>,
        ApiResponse<Vec<AuditLog>>,
        ApiResponse<Vec<Connector>>,
        ApiResponse<Vec<ConnectorRun>>,
        ApiResponse<Vec<Maintainer>>,
        ApiResponse<Vec<MaintainerMember>>,
        ApiResponse<Vec<Notification>>,
        ApiResponse<Vec<Package>>,
        ApiResponse<Vec<Service>>,
        ApiResponse<Vec<UserSummary>>,
        ApiResponse<Vec<WorkCard>>,
        AuditLog,
        Connector,
        ConnectorConfigResponse,
        ConnectorConfigUpdate,
        ConnectorImportError,
        ConnectorOperationsResponse,
        ConnectorRun,
        ConnectorRunDetail,
        ConnectorRunExecutionResponse,
        ConnectorRunItem,
        ConnectorRunItemError,
        ConnectorUpdate,
        ConnectorWorkerStatus,
        Credentials,
        DashboardResponse,
        DashboardScope,
        DashboardSummary,
        FieldViolation,
        HealthResponse,
        LoginResponse,
        Maintainer,
        MaintainerMember,
        MaintainerMemberRequest,
        ManualConnectorRunRequest,
        MeOverviewResponse,
        MeResponse,
        NewConnector,
        NewMaintainer,
        NewNotification,
        NewPackage,
        NewService,
        NewWorkCard,
        Notification,
        NotificationImportItem,
        NotificationImportRequest,
        Package,
        Service,
        ServiceHealthCheck,
        ServiceHealthHistory,
        ServiceHealthHistorySummary,
        ServiceHealthImportItem,
        ServiceHealthImportRequest,
        ServiceHealthOverview,
        ServiceLinks,
        ServiceOverview,
        ServiceOwner,
        UserSummary,
        WorkCard,
        WorkCardImportItem,
        WorkCardImportRequest
    )),
    tags(
        (name = "Docs", description = "Machine-readable API documentation."),
        (name = "Auth", description = "Session and current-user endpoints."),
        (name = "Dashboard", description = "Workday overview and operational summary."),
        (name = "Catalog", description = "Maintainers, services, packages, work cards, and notifications."),
        (name = "Connectors", description = "Connector registry, configuration, run history, worker operations, and import endpoints."),
        (name = "Audit", description = "Audit log read APIs."),
        (name = "Health", description = "Service liveness endpoint.")
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

pub fn spec() -> OpenApi {
    ApiDoc::openapi()
}

#[rocket::get("/openapi.json")]
pub fn openapi_json() -> Json<OpenApi> {
    Json(spec())
}

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
        }
    }
}

#[utoipa::path(
    get,
    path = "/openapi.json",
    tag = "Docs",
    operation_id = "getOpenApiSpec",
    responses((status = 200, description = "OpenAPI 3.1 JSON document"))
)]
fn openapi_json_doc() {}

#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    operation_id = "getHealth",
    responses((status = 200, description = "API health status.", body = ApiResponse<HealthResponse>))
)]
fn health_doc() {}

#[utoipa::path(
    post,
    path = "/login",
    tag = "Auth",
    operation_id = "login",
    request_body(content = Credentials, description = "Username/password credentials.", content_type = "application/json"),
    responses(
        (status = 200, description = "Bearer token and expiration.", body = ApiResponse<LoginResponse>),
        (status = 400, description = "Invalid request body.", body = ApiErrorResponse),
        (status = 401, description = "Invalid credentials.", body = ApiErrorResponse)
    )
)]
fn login_doc() {}

#[utoipa::path(
    post,
    path = "/logout",
    tag = "Auth",
    operation_id = "logout",
    security(("bearer_auth" = [])),
    responses(
        (status = 204, description = "Session deleted."),
        (status = 401, description = "Authentication is required.", body = ApiErrorResponse)
    )
)]
fn logout_doc() {}

#[utoipa::path(
    get,
    path = "/me",
    tag = "Auth",
    operation_id = "getCurrentUser",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Current authenticated user.", body = ApiResponse<MeResponse>),
        (status = 401, description = "Authentication is required.", body = ApiErrorResponse)
    )
)]
fn me_doc() {}

#[utoipa::path(
    get,
    path = "/users",
    tag = "Auth",
    operation_id = "listUsers",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Authenticated user directory for membership assignment. Password hashes are never returned.", body = ApiResponse<Vec<UserSummary>>),
        (status = 401, description = "Authentication is required.", body = ApiErrorResponse)
    )
)]
fn list_users_doc() {}

#[utoipa::path(
    get,
    path = "/me/overview",
    tag = "Auth",
    operation_id = "getCurrentUserOverview",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "User-scoped daily operational context.", body = ApiResponse<MeOverviewResponse>),
        (status = 401, description = "Authentication is required.", body = ApiErrorResponse)
    )
)]
fn me_overview_doc() {}

#[utoipa::path(
    get,
    path = "/dashboard",
    tag = "Dashboard",
    operation_id = "getDashboard",
    security(("bearer_auth" = [])),
    params(
        ("maintainer_id" = Option<i32>, Query, description = "Optional maintainer scope."),
        ("source" = Option<String>, Query, description = "Optional connector source scope.")
    ),
    responses(
        (status = 200, description = "Dashboard cards, health timeline, work cards, notifications, and package activity.", body = ApiResponse<DashboardResponse>),
        (status = 401, description = "Authentication is required.", body = ApiErrorResponse)
    )
)]
fn dashboard_doc() {}

#[utoipa::path(
    get,
    path = "/connectors",
    tag = "Connectors",
    operation_id = "listConnectors",
    security(("bearer_auth" = [])),
    responses((status = 200, description = "Connector registry entries.", body = ApiResponse<Vec<Connector>>))
)]
fn list_connectors_doc() {}

#[utoipa::path(
    get,
    path = "/connectors/operations",
    tag = "Connectors",
    operation_id = "getConnectorOperations",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Worker heartbeat status and retention cleanup history for operator monitoring.", body = ApiResponse<ConnectorOperationsResponse>),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse)
    )
)]
fn connector_operations_doc() {}

#[utoipa::path(
    get,
    path = "/connectors/{source}",
    tag = "Connectors",
    operation_id = "getConnector",
    security(("bearer_auth" = [])),
    params(("source" = String, Path, description = "Connector source key.")),
    responses(
        (status = 200, description = "Connector registry entry.", body = ApiResponse<Connector>),
        (status = 404, description = "Connector was not found.", body = ApiErrorResponse)
    )
)]
fn get_connector_doc() {}

#[utoipa::path(
    post,
    path = "/connectors",
    tag = "Connectors",
    operation_id = "createConnector",
    security(("bearer_auth" = [])),
    request_body(content = NewConnector, description = "Connector source, adapter kind, display name, and status.", content_type = "application/json"),
    responses(
        (status = 201, description = "Connector created.", body = ApiResponse<Connector>),
        (status = 400, description = "Validation failed.", body = ApiErrorResponse),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse)
    )
)]
fn create_connector_doc() {}

#[utoipa::path(
    put,
    path = "/connectors/{source}",
    tag = "Connectors",
    operation_id = "updateConnector",
    security(("bearer_auth" = [])),
    params(("source" = String, Path, description = "Connector source key.")),
    request_body(content = ConnectorUpdate, description = "Connector mutable registry fields.", content_type = "application/json"),
    responses(
        (status = 200, description = "Connector updated.", body = ApiResponse<Connector>),
        (status = 400, description = "Validation failed.", body = ApiErrorResponse),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse),
        (status = 404, description = "Connector was not found.", body = ApiErrorResponse)
    )
)]
fn update_connector_doc() {}

#[utoipa::path(
    delete,
    path = "/connectors/{source}",
    tag = "Connectors",
    operation_id = "deleteConnector",
    security(("bearer_auth" = [])),
    params(("source" = String, Path, description = "Connector source key.")),
    responses(
        (status = 204, description = "Connector deleted."),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse),
        (status = 404, description = "Connector was not found.", body = ApiErrorResponse)
    )
)]
fn delete_connector_doc() {}

#[utoipa::path(
    get,
    path = "/connectors/{source}/config",
    tag = "Connectors",
    operation_id = "getConnectorConfig",
    security(("bearer_auth" = [])),
    params(("source" = String, Path, description = "Connector source key.")),
    responses(
        (status = 200, description = "Redacted connector configuration. Secret-like values are masked and must not be sent back as credentials.", body = ApiResponse<ConnectorConfigResponse>),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse),
        (status = 404, description = "Configuration was not found.", body = ApiErrorResponse)
    )
)]
fn get_connector_config_doc() {}

#[utoipa::path(
    put,
    path = "/connectors/{source}/config",
    tag = "Connectors",
    operation_id = "upsertConnectorConfig",
    security(("bearer_auth" = [])),
    params(("source" = String, Path, description = "Connector source key.")),
    request_body(content = ConnectorConfigUpdate, description = "Connector execution target, schedule, JSON config, and stored sample payload. Redacted secret placeholders preserve existing encrypted secrets.", content_type = "application/json"),
    responses(
        (status = 200, description = "Configuration created or updated with secrets redacted in the response.", body = ApiResponse<ConnectorConfigResponse>),
        (status = 400, description = "Validation failed, including invalid JSON or unsupported schedule/target.", body = ApiErrorResponse),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse),
        (status = 404, description = "Connector was not found.", body = ApiErrorResponse)
    )
)]
fn upsert_connector_config_doc() {}

#[utoipa::path(
    get,
    path = "/connectors/runs",
    tag = "Connectors",
    operation_id = "listConnectorRuns",
    security(("bearer_auth" = [])),
    params(
        ("source" = Option<String>, Query, description = "Optional connector source filter."),
        ("target" = Option<String>, Query, description = "Optional import target filter: service_health, work_cards, or notifications.")
    ),
    responses(
        (status = 200, description = "Recent connector runs.", body = ApiResponse<Vec<ConnectorRun>>),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse)
    )
)]
fn list_connector_runs_doc() {}

#[utoipa::path(
    get,
    path = "/connectors/runs/{id}",
    tag = "Connectors",
    operation_id = "getConnectorRun",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Connector run id.")),
    responses(
        (status = 200, description = "Connector run plus imported item snapshots, item errors, and health checks.", body = ApiResponse<ConnectorRunDetail>),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse),
        (status = 404, description = "Run was not found.", body = ApiErrorResponse)
    )
)]
fn get_connector_run_doc() {}

#[utoipa::path(
    post,
    path = "/connectors/runs/{id}/retry",
    tag = "Connectors",
    operation_id = "retryConnectorRun",
    security(("bearer_auth" = [])),
    params(("id" = i32, Path, description = "Failed or partial_success connector run id.")),
    responses(
        (status = 201, description = "Retry run queued. Only failed or partial_success runs can be retried.", body = ApiResponse<ConnectorRunExecutionResponse>),
        (status = 400, description = "Run cannot be retried.", body = ApiErrorResponse),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse),
        (status = 404, description = "Run or connector was not found.", body = ApiErrorResponse)
    )
)]
fn retry_connector_run_doc() {}

#[utoipa::path(
    post,
    path = "/connectors/{source}/runs",
    tag = "Connectors",
    operation_id = "runConnector",
    security(("bearer_auth" = [])),
    params(("source" = String, Path, description = "Connector source key.")),
    request_body(content = ManualConnectorRunRequest, description = "`mode=execute` runs immediately; `mode=queue` stores a queued run for the worker. Optional payload overrides the stored sample payload for this run.", content_type = "application/json"),
    responses(
        (status = 201, description = "Connector run executed or queued.", body = ApiResponse<ConnectorRunExecutionResponse>),
        (status = 400, description = "Validation failed or connector is paused/disabled.", body = ApiErrorResponse),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse),
        (status = 404, description = "Connector or config was not found.", body = ApiErrorResponse)
    )
)]
fn run_connector_doc() {}

#[utoipa::path(
    post,
    path = "/connectors/{source}/work-cards/import",
    tag = "Connectors",
    operation_id = "importWorkCards",
    security(("bearer_auth" = [])),
    params(("source" = String, Path, description = "Connector source key recorded on imported work cards and run history.")),
    request_body(content = WorkCardImportRequest, description = "Direct import payload for work cards. Each item is upserted by source/external_id and recorded in connector run item history.", content_type = "application/json"),
    responses(
        (status = 201, description = "Import run finished with imported/failed counts and per-item errors.", body = ApiResponse<ConnectorRunExecutionResponse>),
        (status = 400, description = "One or more items failed validation.", body = ApiErrorResponse),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse)
    )
)]
fn import_work_cards_doc() {}

#[utoipa::path(
    post,
    path = "/connectors/{source}/notifications/import",
    tag = "Connectors",
    operation_id = "importNotifications",
    security(("bearer_auth" = [])),
    params(("source" = String, Path, description = "Connector source key recorded on imported notifications and run history.")),
    request_body(content = NotificationImportRequest, description = "Direct import payload for system notifications. Items are upserted by source/external_id and visible on dashboard notifications.", content_type = "application/json"),
    responses(
        (status = 201, description = "Import run finished with imported/failed counts and per-item errors.", body = ApiResponse<ConnectorRunExecutionResponse>),
        (status = 400, description = "One or more items failed validation.", body = ApiErrorResponse),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse)
    )
)]
fn import_notifications_doc() {}

#[utoipa::path(
    post,
    path = "/connectors/{source}/service-health/import",
    tag = "Connectors",
    operation_id = "importServiceHealth",
    security(("bearer_auth" = [])),
    params(("source" = String, Path, description = "Connector source key recorded on services, health checks, and run history.")),
    request_body(content = ServiceHealthImportRequest, description = "Direct import payload for service health. Each item upserts a service, appends a health check, and records connector run item history. `maintainer_id`, `slug`, lifecycle, and health status are required.", content_type = "application/json"),
    responses(
        (status = 201, description = "Import run finished with service records, health checks, imported/failed counts, and per-item errors.", body = ApiResponse<ConnectorRunExecutionResponse>),
        (status = 400, description = "One or more items failed validation.", body = ApiErrorResponse),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse)
    )
)]
fn import_service_health_doc() {}

#[utoipa::path(get, path = "/maintainers", tag = "Catalog", operation_id = "listMaintainers", security(("bearer_auth" = [])), responses((status = 200, description = "Maintainer records.", body = ApiResponse<Vec<Maintainer>>)))]
fn list_maintainers_doc() {}

#[utoipa::path(get, path = "/maintainers/{id}", tag = "Catalog", operation_id = "getMaintainer", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Maintainer id.")), responses((status = 200, description = "Maintainer record.", body = ApiResponse<Maintainer>), (status = 404, description = "Maintainer was not found.", body = ApiErrorResponse)))]
fn get_maintainer_doc() {}

#[utoipa::path(post, path = "/maintainers", tag = "Catalog", operation_id = "createMaintainer", security(("bearer_auth" = [])), request_body(content = NewMaintainer, content_type = "application/json"), responses((status = 201, description = "Maintainer created.", body = ApiResponse<Maintainer>), (status = 400, description = "Validation failed.", body = ApiErrorResponse), (status = 403, description = "Admin role is required.", body = ApiErrorResponse)))]
fn create_maintainer_doc() {}

#[utoipa::path(put, path = "/maintainers/{id}", tag = "Catalog", operation_id = "updateMaintainer", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Maintainer id.")), request_body(content = NewMaintainer, content_type = "application/json"), responses((status = 200, description = "Maintainer updated.", body = ApiResponse<Maintainer>), (status = 400, description = "Validation failed.", body = ApiErrorResponse), (status = 403, description = "Admin role is required.", body = ApiErrorResponse), (status = 404, description = "Maintainer was not found.", body = ApiErrorResponse)))]
fn update_maintainer_doc() {}

#[utoipa::path(delete, path = "/maintainers/{id}", tag = "Catalog", operation_id = "deleteMaintainer", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Maintainer id.")), responses((status = 204, description = "Maintainer deleted."), (status = 403, description = "Admin role is required.", body = ApiErrorResponse), (status = 404, description = "Maintainer was not found.", body = ApiErrorResponse)))]
fn delete_maintainer_doc() {}

#[utoipa::path(get, path = "/maintainers/{id}/members", tag = "Catalog", operation_id = "listMaintainerMembers", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Maintainer id.")), responses((status = 200, description = "Maintainer membership rows.", body = ApiResponse<Vec<MaintainerMember>>), (status = 403, description = "Owner/admin access is required.", body = ApiErrorResponse)))]
fn list_maintainer_members_doc() {}

#[utoipa::path(post, path = "/maintainers/{id}/members", tag = "Catalog", operation_id = "upsertMaintainerMember", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Maintainer id.")), request_body(content = MaintainerMemberRequest, content_type = "application/json"), responses((status = 201, description = "Maintainer member created or updated.", body = ApiResponse<MaintainerMember>), (status = 400, description = "Validation failed.", body = ApiErrorResponse), (status = 403, description = "Owner/admin access is required.", body = ApiErrorResponse)))]
fn upsert_maintainer_member_doc() {}

#[utoipa::path(delete, path = "/maintainers/{id}/members/{user_id}", tag = "Catalog", operation_id = "deleteMaintainerMember", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Maintainer id."), ("user_id" = i32, Path, description = "User id.")), responses((status = 204, description = "Maintainer member deleted."), (status = 403, description = "Owner/admin access is required.", body = ApiErrorResponse)))]
fn delete_maintainer_member_doc() {}

#[utoipa::path(get, path = "/services", tag = "Catalog", operation_id = "listServices", security(("bearer_auth" = [])), responses((status = 200, description = "Service catalog records.", body = ApiResponse<Vec<Service>>)))]
fn list_services_doc() {}

#[utoipa::path(get, path = "/services/{id}", tag = "Catalog", operation_id = "getService", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Service id.")), responses((status = 200, description = "Service record.", body = ApiResponse<Service>), (status = 404, description = "Service was not found.", body = ApiErrorResponse)))]
fn get_service_doc() {}

#[utoipa::path(get, path = "/services/{id}/overview", tag = "Catalog", operation_id = "getServiceOverview", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Service id.")), responses((status = 200, description = "Service context with ownership, packages, links, and recent connector runs.", body = ApiResponse<ServiceOverview>), (status = 404, description = "Service was not found.", body = ApiErrorResponse)))]
fn get_service_overview_doc() {}

#[utoipa::path(post, path = "/services", tag = "Catalog", operation_id = "createService", security(("bearer_auth" = [])), request_body(content = NewService, content_type = "application/json"), responses((status = 201, description = "Service created.", body = ApiResponse<Service>), (status = 400, description = "Validation failed.", body = ApiErrorResponse), (status = 403, description = "Maintainer write access is required.", body = ApiErrorResponse)))]
fn create_service_doc() {}

#[utoipa::path(put, path = "/services/{id}", tag = "Catalog", operation_id = "updateService", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Service id.")), request_body(content = NewService, content_type = "application/json"), responses((status = 200, description = "Service updated.", body = ApiResponse<Service>), (status = 400, description = "Validation failed.", body = ApiErrorResponse), (status = 403, description = "Maintainer write access is required.", body = ApiErrorResponse), (status = 404, description = "Service was not found.", body = ApiErrorResponse)))]
fn update_service_doc() {}

#[utoipa::path(delete, path = "/services/{id}", tag = "Catalog", operation_id = "deleteService", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Service id.")), responses((status = 204, description = "Service deleted."), (status = 403, description = "Maintainer write access is required.", body = ApiErrorResponse), (status = 404, description = "Service was not found.", body = ApiErrorResponse)))]
fn delete_service_doc() {}

#[utoipa::path(get, path = "/packages", tag = "Catalog", operation_id = "listPackages", security(("bearer_auth" = [])), responses((status = 200, description = "Package catalog records.", body = ApiResponse<Vec<Package>>)))]
fn list_packages_doc() {}

#[utoipa::path(get, path = "/packages/{id}", tag = "Catalog", operation_id = "getPackage", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Package id.")), responses((status = 200, description = "Package record.", body = ApiResponse<Package>), (status = 404, description = "Package was not found.", body = ApiErrorResponse)))]
fn get_package_doc() {}

#[utoipa::path(post, path = "/packages", tag = "Catalog", operation_id = "createPackage", security(("bearer_auth" = [])), request_body(content = NewPackage, content_type = "application/json"), responses((status = 201, description = "Package created.", body = ApiResponse<Package>), (status = 400, description = "Validation failed.", body = ApiErrorResponse), (status = 403, description = "Maintainer write access is required.", body = ApiErrorResponse)))]
fn create_package_doc() {}

#[utoipa::path(put, path = "/packages/{id}", tag = "Catalog", operation_id = "updatePackage", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Package id.")), request_body(content = NewPackage, content_type = "application/json"), responses((status = 200, description = "Package updated.", body = ApiResponse<Package>), (status = 400, description = "Validation failed.", body = ApiErrorResponse), (status = 403, description = "Maintainer write access is required.", body = ApiErrorResponse), (status = 404, description = "Package was not found.", body = ApiErrorResponse)))]
fn update_package_doc() {}

#[utoipa::path(delete, path = "/packages/{id}", tag = "Catalog", operation_id = "deletePackage", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Package id.")), responses((status = 204, description = "Package deleted."), (status = 403, description = "Maintainer write access is required.", body = ApiErrorResponse), (status = 404, description = "Package was not found.", body = ApiErrorResponse)))]
fn delete_package_doc() {}

#[utoipa::path(get, path = "/work-cards", tag = "Catalog", operation_id = "listWorkCards", security(("bearer_auth" = [])), responses((status = 200, description = "Work card records.", body = ApiResponse<Vec<WorkCard>>)))]
fn list_work_cards_doc() {}

#[utoipa::path(get, path = "/work-cards/{id}", tag = "Catalog", operation_id = "getWorkCard", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Work card id.")), responses((status = 200, description = "Work card record.", body = ApiResponse<WorkCard>), (status = 404, description = "Work card was not found.", body = ApiErrorResponse)))]
fn get_work_card_doc() {}

#[utoipa::path(post, path = "/work-cards", tag = "Catalog", operation_id = "createWorkCard", security(("bearer_auth" = [])), request_body(content = NewWorkCard, content_type = "application/json"), responses((status = 201, description = "Work card created.", body = ApiResponse<WorkCard>), (status = 400, description = "Validation failed.", body = ApiErrorResponse), (status = 403, description = "Admin role is required.", body = ApiErrorResponse)))]
fn create_work_card_doc() {}

#[utoipa::path(put, path = "/work-cards/{id}", tag = "Catalog", operation_id = "updateWorkCard", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Work card id.")), request_body(content = NewWorkCard, content_type = "application/json"), responses((status = 200, description = "Work card updated.", body = ApiResponse<WorkCard>), (status = 400, description = "Validation failed.", body = ApiErrorResponse), (status = 403, description = "Admin role is required.", body = ApiErrorResponse), (status = 404, description = "Work card was not found.", body = ApiErrorResponse)))]
fn update_work_card_doc() {}

#[utoipa::path(delete, path = "/work-cards/{id}", tag = "Catalog", operation_id = "deleteWorkCard", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Work card id.")), responses((status = 204, description = "Work card deleted."), (status = 403, description = "Admin role is required.", body = ApiErrorResponse), (status = 404, description = "Work card was not found.", body = ApiErrorResponse)))]
fn delete_work_card_doc() {}

#[utoipa::path(get, path = "/notifications", tag = "Catalog", operation_id = "listNotifications", security(("bearer_auth" = [])), responses((status = 200, description = "Notification records.", body = ApiResponse<Vec<Notification>>)))]
fn list_notifications_doc() {}

#[utoipa::path(get, path = "/notifications/{id}", tag = "Catalog", operation_id = "getNotification", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Notification id.")), responses((status = 200, description = "Notification record.", body = ApiResponse<Notification>), (status = 404, description = "Notification was not found.", body = ApiErrorResponse)))]
fn get_notification_doc() {}

#[utoipa::path(post, path = "/notifications", tag = "Catalog", operation_id = "createNotification", security(("bearer_auth" = [])), request_body(content = NewNotification, content_type = "application/json"), responses((status = 201, description = "Notification created.", body = ApiResponse<Notification>), (status = 400, description = "Validation failed.", body = ApiErrorResponse), (status = 403, description = "Admin role is required.", body = ApiErrorResponse)))]
fn create_notification_doc() {}

#[utoipa::path(put, path = "/notifications/{id}", tag = "Catalog", operation_id = "updateNotification", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Notification id.")), request_body(content = NewNotification, content_type = "application/json"), responses((status = 200, description = "Notification updated.", body = ApiResponse<Notification>), (status = 400, description = "Validation failed.", body = ApiErrorResponse), (status = 403, description = "Admin role is required.", body = ApiErrorResponse), (status = 404, description = "Notification was not found.", body = ApiErrorResponse)))]
fn update_notification_doc() {}

#[utoipa::path(delete, path = "/notifications/{id}", tag = "Catalog", operation_id = "deleteNotification", security(("bearer_auth" = [])), params(("id" = i32, Path, description = "Notification id.")), responses((status = 204, description = "Notification deleted."), (status = 403, description = "Admin role is required.", body = ApiErrorResponse), (status = 404, description = "Notification was not found.", body = ApiErrorResponse)))]
fn delete_notification_doc() {}

#[utoipa::path(
    get,
    path = "/audit-logs",
    tag = "Audit",
    operation_id = "listAuditLogs",
    security(("bearer_auth" = [])),
    params(
        ("resource_type" = Option<String>, Query, description = "Optional resource type filter."),
        ("resource_id" = Option<String>, Query, description = "Optional resource id filter."),
        ("actor_user_id" = Option<i32>, Query, description = "Optional actor user id filter."),
        ("action" = Option<String>, Query, description = "Optional audit action filter."),
        ("created_from" = Option<String>, Query, description = "Optional inclusive created-at lower bound, as YYYY-MM-DD or YYYY-MM-DDTHH:MM."),
        ("created_to" = Option<String>, Query, description = "Optional inclusive created-at upper bound, as YYYY-MM-DD or YYYY-MM-DDTHH:MM.")
    ),
    responses(
        (status = 200, description = "Recent audit log entries.", body = ApiResponse<Vec<AuditLog>>),
        (status = 403, description = "Admin role is required.", body = ApiErrorResponse)
    )
)]
fn list_audit_logs_doc() {}

#[cfg(test)]
mod tests {
    use super::spec;

    #[test]
    fn openapi_spec_documents_connector_imports_and_auth_scheme() {
        let value = serde_json::to_value(spec()).expect("openapi spec serializes");
        let paths = value
            .get("paths")
            .and_then(|paths| paths.as_object())
            .expect("paths object exists");

        for path in [
            "/connectors/{source}/service-health/import",
            "/connectors/{source}/work-cards/import",
            "/connectors/{source}/notifications/import",
            "/connectors/{source}/runs",
            "/connectors/{source}/config",
            "/connectors/runs/{id}",
            "/openapi.json",
        ] {
            assert!(paths.contains_key(path), "{path} should be documented");
        }

        let service_health_import = paths
            .get("/connectors/{source}/service-health/import")
            .and_then(|path| path.get("post"))
            .expect("service health import operation exists");
        assert_eq!(
            service_health_import
                .get("operationId")
                .and_then(|operation_id| operation_id.as_str()),
            Some("importServiceHealth")
        );
        assert!(
            service_health_import.get("requestBody").is_some(),
            "service health import should document its request body"
        );

        let security_schemes = value
            .get("components")
            .and_then(|components| components.get("securitySchemes"))
            .and_then(|security_schemes| security_schemes.as_object())
            .expect("security schemes object exists");
        assert!(
            security_schemes.contains_key("bearer_auth"),
            "bearer auth scheme should be documented"
        );
    }
}
