use crate::api::{created, ok, ApiError, ApiResult, CreatedApiResult};
use crate::auth::{require_maintainer_write_access, AuthenticatedUser};
use crate::models::{
    Connector, ConnectorRun, Maintainer, MaintainerMember, NewService, Package, Service,
};
use crate::repositories::{
    ConnectorRepository, ConnectorRunRepository, MaintainerMemberRepository, MaintainerRepository,
    PackageRepository, ServiceRepository,
};
use crate::rocket_routes::audit_logs::record_audit_log;
use crate::rocket_routes::DbConn;
use crate::validation::validate_request;
use diesel::result::Error as DieselError;
use rocket::response::status::NoContent;
use rocket::serde::json::Json;
use rocket::serde::Serialize;
use rocket_db_pools::Connection;
use serde_json::json;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct ServiceOverview {
    pub service: Service,
    pub owner: ServiceOwner,
    pub maintainer: Maintainer,
    pub maintainer_members: Vec<MaintainerMember>,
    pub packages: Vec<Package>,
    pub health: ServiceHealthOverview,
    pub connector: Option<Connector>,
    pub recent_connector_runs: Vec<ConnectorRun>,
    pub links: ServiceLinks,
}

#[derive(Serialize, ToSchema)]
pub struct ServiceOwner {
    pub id: i32,
    pub display_name: String,
    pub email: String,
}

#[derive(Serialize, ToSchema)]
pub struct ServiceHealthOverview {
    pub status: String,
    pub lifecycle_status: String,
    pub last_checked_at: Option<chrono::NaiveDateTime>,
}

#[derive(Serialize, ToSchema)]
pub struct ServiceLinks {
    pub repository_url: Option<String>,
    pub dashboard_url: Option<String>,
    pub runbook_url: Option<String>,
}

#[rocket::get("/services")]
pub async fn get_services(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
) -> ApiResult<Vec<Service>> {
    let services = ServiceRepository::find_multiple(&mut db, 100).await?;
    ok(services)
}

#[rocket::get("/services/<id>")]
pub async fn view_service(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
    id: i32,
) -> ApiResult<Service> {
    let service = ServiceRepository::find(&mut db, id).await?;
    ok(service)
}

#[rocket::get("/services/<id>/overview")]
pub async fn service_overview(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
    id: i32,
) -> ApiResult<ServiceOverview> {
    let service = ServiceRepository::find(&mut db, id).await?;
    let maintainer_id = service.maintainer_id;
    let source = service.source.clone();

    let maintainer = MaintainerRepository::find(&mut db, maintainer_id).await?;
    let maintainer_members =
        MaintainerMemberRepository::find_by_maintainer(&mut db, maintainer_id).await?;
    let packages =
        PackageRepository::find_recent_for_maintainer(&mut db, 20, Some(maintainer_id)).await?;
    let connector = match ConnectorRepository::find_by_source(&mut db, &source).await {
        Ok(connector) => Some(connector),
        Err(DieselError::NotFound) => None,
        Err(error) => return Err(error.into()),
    };
    let recent_connector_runs =
        ConnectorRunRepository::find_multiple(&mut db, 10, Some(&source), Some("service_health"))
            .await?;
    let owner = ServiceOwner {
        id: maintainer.id,
        display_name: maintainer.display_name.clone(),
        email: maintainer.email.clone(),
    };
    let health = ServiceHealthOverview {
        status: service.health_status.clone(),
        lifecycle_status: service.lifecycle_status.clone(),
        last_checked_at: service.last_checked_at,
    };
    let links = ServiceLinks {
        repository_url: service.repository_url.clone(),
        dashboard_url: service.dashboard_url.clone(),
        runbook_url: service.runbook_url.clone(),
    };

    ok(ServiceOverview {
        service,
        owner,
        maintainer,
        maintainer_members,
        packages,
        health,
        connector,
        recent_connector_runs,
        links,
    })
}

#[rocket::post("/services", format = "json", data = "<new_service>")]
pub async fn create_service(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    new_service: Json<NewService>,
) -> CreatedApiResult<Service> {
    let new_service = validate_request(new_service.into_inner())?;
    require_maintainer_write_access(&mut db, &auth, new_service.maintainer_id).await?;
    let service = ServiceRepository::create(&mut db, new_service).await?;
    record_audit_log(
        &mut db,
        &auth,
        "create",
        "service",
        service.id,
        json!({
            "maintainer_id": service.maintainer_id,
            "slug": &service.slug,
            "health_status": &service.health_status,
        }),
    )
    .await?;

    created(service)
}

#[rocket::put("/services/<id>", format = "json", data = "<service>")]
pub async fn update_service(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
    service: Json<NewService>,
) -> ApiResult<Service> {
    let service = validate_request(service.into_inner())?;
    let existing = ServiceRepository::find(&mut db, id).await?;
    require_maintainer_write_access(&mut db, &auth, existing.maintainer_id).await?;
    require_maintainer_write_access(&mut db, &auth, service.maintainer_id).await?;
    let service = ServiceRepository::update(&mut db, id, service).await?;
    record_audit_log(
        &mut db,
        &auth,
        "update",
        "service",
        service.id,
        json!({
            "maintainer_id": service.maintainer_id,
            "slug": &service.slug,
            "health_status": &service.health_status,
        }),
    )
    .await?;

    ok(service)
}

#[rocket::delete("/services/<id>")]
pub async fn delete_service(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
) -> Result<NoContent, ApiError> {
    let service = ServiceRepository::find(&mut db, id).await?;
    require_maintainer_write_access(&mut db, &auth, service.maintainer_id).await?;
    ServiceRepository::delete(&mut db, id).await?;
    record_audit_log(
        &mut db,
        &auth,
        "delete",
        "service",
        id,
        json!({
            "maintainer_id": service.maintainer_id,
            "slug": &service.slug,
        }),
    )
    .await?;

    Ok(NoContent)
}
