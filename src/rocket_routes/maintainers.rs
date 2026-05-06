use crate::api::{created, ok, ApiError, ApiResult, CreatedApiResult};
use crate::models::{Maintainer, NewMaintainer};
use crate::repositories::MaintainerRepository;
use crate::rocket_routes::DbConn;
use crate::validation::validate_request;
use rocket::response::status::NoContent;
use rocket::serde::json::Json;
use rocket_db_pools::Connection;

#[rocket::get("/maintainers")]
pub async fn get_maintainers(mut db: Connection<DbConn>) -> ApiResult<Vec<Maintainer>> {
    let maintainers = MaintainerRepository::find_multiple(&mut db, 100).await?;
    ok(maintainers)
}

#[rocket::get("/maintainers/<id>")]
pub async fn view_maintainer(mut db: Connection<DbConn>, id: i32) -> ApiResult<Maintainer> {
    let maintainer = MaintainerRepository::find(&mut db, id).await?;
    ok(maintainer)
}

#[rocket::post("/maintainers", format = "json", data = "<new_maintainer>")]
pub async fn create_maintainer(
    mut db: Connection<DbConn>,
    new_maintainer: Json<NewMaintainer>,
) -> CreatedApiResult<Maintainer> {
    let new_maintainer = validate_request(new_maintainer.into_inner())?;
    let maintainer = MaintainerRepository::create(&mut db, new_maintainer).await?;

    created(maintainer)
}

#[rocket::put("/maintainers/<id>", format = "json", data = "<maintainer>")]
pub async fn update_maintainer(
    mut db: Connection<DbConn>,
    id: i32,
    maintainer: Json<NewMaintainer>,
) -> ApiResult<Maintainer> {
    let maintainer = validate_request(maintainer.into_inner())?;
    let maintainer = MaintainerRepository::update(&mut db, id, maintainer).await?;

    ok(maintainer)
}

#[rocket::delete("/maintainers/<id>")]
pub async fn delete_maintainer(mut db: Connection<DbConn>, id: i32) -> Result<NoContent, ApiError> {
    MaintainerRepository::delete(&mut db, id).await?;

    Ok(NoContent)
}
