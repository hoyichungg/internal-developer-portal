use crate::api::{created, ok, ApiError, ApiResult, CreatedApiResult};
use crate::auth::{require_admin, require_maintainer_owner_access, AuthenticatedUser};
use crate::models::{Maintainer, MaintainerMember, NewMaintainer, NewMaintainerMember};
use crate::repositories::{MaintainerMemberRepository, MaintainerRepository, UserRepository};
use crate::rocket_routes::audit_logs::record_audit_log;
use crate::rocket_routes::DbConn;
use crate::validation::validate_request;
use rocket::response::status::NoContent;
use rocket::serde::json::Json;
use rocket::serde::Deserialize;
use rocket_db_pools::Connection;
use serde_json::json;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct MaintainerMemberRequest {
    pub user_id: i32,
    pub role: String,
}

#[rocket::get("/maintainers")]
pub async fn get_maintainers(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
) -> ApiResult<Vec<Maintainer>> {
    let maintainers = MaintainerRepository::find_multiple(&mut db, 100).await?;
    ok(maintainers)
}

#[rocket::get("/maintainers/<id>")]
pub async fn view_maintainer(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
    id: i32,
) -> ApiResult<Maintainer> {
    let maintainer = MaintainerRepository::find(&mut db, id).await?;
    ok(maintainer)
}

#[rocket::post("/maintainers", format = "json", data = "<new_maintainer>")]
pub async fn create_maintainer(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    new_maintainer: Json<NewMaintainer>,
) -> CreatedApiResult<Maintainer> {
    require_admin(&auth)?;
    let new_maintainer = validate_request(new_maintainer.into_inner())?;
    let maintainer = MaintainerRepository::create(&mut db, new_maintainer).await?;
    record_audit_log(
        &mut db,
        &auth,
        "create",
        "maintainer",
        maintainer.id,
        json!({ "email": &maintainer.email }),
    )
    .await?;

    created(maintainer)
}

#[rocket::put("/maintainers/<id>", format = "json", data = "<maintainer>")]
pub async fn update_maintainer(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
    maintainer: Json<NewMaintainer>,
) -> ApiResult<Maintainer> {
    require_admin(&auth)?;
    let maintainer = validate_request(maintainer.into_inner())?;
    let maintainer = MaintainerRepository::update(&mut db, id, maintainer).await?;
    record_audit_log(
        &mut db,
        &auth,
        "update",
        "maintainer",
        maintainer.id,
        json!({ "email": &maintainer.email }),
    )
    .await?;

    ok(maintainer)
}

#[rocket::delete("/maintainers/<id>")]
pub async fn delete_maintainer(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
) -> Result<NoContent, ApiError> {
    require_admin(&auth)?;
    MaintainerRepository::delete(&mut db, id).await?;
    record_audit_log(&mut db, &auth, "delete", "maintainer", id, json!({})).await?;

    Ok(NoContent)
}

#[rocket::get("/maintainers/<id>/members")]
pub async fn get_maintainer_members(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
) -> ApiResult<Vec<MaintainerMember>> {
    require_maintainer_owner_access(&mut db, &auth, id).await?;
    let members = MaintainerMemberRepository::find_by_maintainer(&mut db, id).await?;

    ok(members)
}

#[rocket::post("/maintainers/<id>/members", format = "json", data = "<member>")]
pub async fn upsert_maintainer_member(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
    member: Json<MaintainerMemberRequest>,
) -> CreatedApiResult<MaintainerMember> {
    require_maintainer_owner_access(&mut db, &auth, id).await?;
    let member = member.into_inner();
    MaintainerRepository::find(&mut db, id).await?;
    UserRepository::find(&mut db, member.user_id).await?;
    let new_member = validate_request(NewMaintainerMember {
        maintainer_id: id,
        user_id: member.user_id,
        role: member.role,
    })?;
    let member = MaintainerMemberRepository::upsert(&mut db, new_member).await?;
    record_audit_log(
        &mut db,
        &auth,
        "upsert_member",
        "maintainer",
        id,
        json!({
            "user_id": member.user_id,
            "role": &member.role,
        }),
    )
    .await?;

    created(member)
}

#[rocket::delete("/maintainers/<id>/members/<user_id>")]
pub async fn delete_maintainer_member(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
    user_id: i32,
) -> Result<NoContent, ApiError> {
    require_maintainer_owner_access(&mut db, &auth, id).await?;
    MaintainerMemberRepository::delete_by_maintainer_and_user(&mut db, id, user_id).await?;
    record_audit_log(
        &mut db,
        &auth,
        "delete_member",
        "maintainer",
        id,
        json!({ "user_id": user_id }),
    )
    .await?;

    Ok(NoContent)
}
