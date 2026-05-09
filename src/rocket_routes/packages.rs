use crate::api::{created, ok, ApiError, ApiResult, CreatedApiResult};
use crate::auth::{require_maintainer_write_access, AuthenticatedUser};
use crate::models::{NewPackage, Package};
use crate::repositories::PackageRepository;
use crate::rocket_routes::audit_logs::record_audit_log;
use crate::rocket_routes::DbConn;
use crate::validation::validate_request;
use rocket::response::status::NoContent;
use rocket::serde::json::Json;
use rocket_db_pools::Connection;
use serde_json::json;

#[rocket::get("/packages")]
pub async fn get_packages(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
) -> ApiResult<Vec<Package>> {
    let packages = PackageRepository::find_multiple(&mut db, 100).await?;
    ok(packages)
}

#[rocket::get("/packages/<id>")]
pub async fn view_package(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
    id: i32,
) -> ApiResult<Package> {
    let package = PackageRepository::find(&mut db, id).await?;
    ok(package)
}

#[rocket::post("/packages", format = "json", data = "<new_package>")]
pub async fn create_package(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    new_package: Json<NewPackage>,
) -> CreatedApiResult<Package> {
    let new_package = validate_request(new_package.into_inner())?;
    require_maintainer_write_access(&mut db, &auth, new_package.maintainer_id).await?;
    let package = PackageRepository::create(&mut db, new_package).await?;
    record_audit_log(
        &mut db,
        &auth,
        "create",
        "package",
        package.id,
        json!({
            "maintainer_id": package.maintainer_id,
            "slug": &package.slug,
            "status": &package.status,
        }),
    )
    .await?;

    created(package)
}

#[rocket::put("/packages/<id>", format = "json", data = "<package>")]
pub async fn update_package(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
    package: Json<NewPackage>,
) -> ApiResult<Package> {
    let package = validate_request(package.into_inner())?;
    let existing = PackageRepository::find(&mut db, id).await?;
    require_maintainer_write_access(&mut db, &auth, existing.maintainer_id).await?;
    require_maintainer_write_access(&mut db, &auth, package.maintainer_id).await?;
    let package = PackageRepository::update(&mut db, id, package).await?;
    record_audit_log(
        &mut db,
        &auth,
        "update",
        "package",
        package.id,
        json!({
            "maintainer_id": package.maintainer_id,
            "slug": &package.slug,
            "status": &package.status,
        }),
    )
    .await?;

    ok(package)
}

#[rocket::delete("/packages/<id>")]
pub async fn delete_package(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
) -> Result<NoContent, ApiError> {
    let package = PackageRepository::find(&mut db, id).await?;
    require_maintainer_write_access(&mut db, &auth, package.maintainer_id).await?;
    PackageRepository::delete(&mut db, id).await?;
    record_audit_log(
        &mut db,
        &auth,
        "delete",
        "package",
        id,
        json!({
            "maintainer_id": package.maintainer_id,
            "slug": &package.slug,
        }),
    )
    .await?;

    Ok(NoContent)
}
