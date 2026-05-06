use crate::api::{created, ok, ApiError, ApiResult, CreatedApiResult};
use crate::models::{NewPackage, Package};
use crate::repositories::PackageRepository;
use crate::rocket_routes::DbConn;
use crate::validation::validate_request;
use rocket::response::status::NoContent;
use rocket::serde::json::Json;
use rocket_db_pools::Connection;

#[rocket::get("/packages")]
pub async fn get_packages(mut db: Connection<DbConn>) -> ApiResult<Vec<Package>> {
    let packages = PackageRepository::find_multiple(&mut db, 100).await?;
    ok(packages)
}

#[rocket::get("/packages/<id>")]
pub async fn view_package(mut db: Connection<DbConn>, id: i32) -> ApiResult<Package> {
    let package = PackageRepository::find(&mut db, id).await?;
    ok(package)
}

#[rocket::post("/packages", format = "json", data = "<new_package>")]
pub async fn create_package(
    mut db: Connection<DbConn>,
    new_package: Json<NewPackage>,
) -> CreatedApiResult<Package> {
    let new_package = validate_request(new_package.into_inner())?;
    let package = PackageRepository::create(&mut db, new_package).await?;

    created(package)
}

#[rocket::put("/packages/<id>", format = "json", data = "<package>")]
pub async fn update_package(
    mut db: Connection<DbConn>,
    id: i32,
    package: Json<NewPackage>,
) -> ApiResult<Package> {
    let package = validate_request(package.into_inner())?;
    let package = PackageRepository::update(&mut db, id, package).await?;

    ok(package)
}

#[rocket::delete("/packages/<id>")]
pub async fn delete_package(mut db: Connection<DbConn>, id: i32) -> Result<NoContent, ApiError> {
    PackageRepository::delete(&mut db, id).await?;

    Ok(NoContent)
}
