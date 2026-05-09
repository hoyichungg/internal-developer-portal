use crate::api::{created, ok, ApiError, ApiResult, CreatedApiResult};
use crate::auth::{require_admin, AuthenticatedUser};
use crate::models::{NewWorkCard, WorkCard};
use crate::repositories::WorkCardRepository;
use crate::rocket_routes::audit_logs::record_audit_log;
use crate::rocket_routes::DbConn;
use crate::validation::validate_request;
use rocket::response::status::NoContent;
use rocket::serde::json::Json;
use rocket_db_pools::Connection;
use serde_json::json;

#[rocket::get("/work-cards")]
pub async fn get_work_cards(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
) -> ApiResult<Vec<WorkCard>> {
    let work_cards = WorkCardRepository::find_multiple(&mut db, 100).await?;
    ok(work_cards)
}

#[rocket::get("/work-cards/<id>")]
pub async fn view_work_card(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
    id: i32,
) -> ApiResult<WorkCard> {
    let work_card = WorkCardRepository::find(&mut db, id).await?;
    ok(work_card)
}

#[rocket::post("/work-cards", format = "json", data = "<new_work_card>")]
pub async fn create_work_card(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    new_work_card: Json<NewWorkCard>,
) -> CreatedApiResult<WorkCard> {
    require_admin(&auth)?;
    let new_work_card = validate_request(new_work_card.into_inner())?;
    let work_card = WorkCardRepository::create(&mut db, new_work_card).await?;
    record_audit_log(
        &mut db,
        &auth,
        "create",
        "work_card",
        work_card.id,
        json!({
            "source": &work_card.source,
            "status": &work_card.status,
            "priority": &work_card.priority,
        }),
    )
    .await?;

    created(work_card)
}

#[rocket::put("/work-cards/<id>", format = "json", data = "<work_card>")]
pub async fn update_work_card(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
    work_card: Json<NewWorkCard>,
) -> ApiResult<WorkCard> {
    require_admin(&auth)?;
    let work_card = validate_request(work_card.into_inner())?;
    let work_card = WorkCardRepository::update(&mut db, id, work_card).await?;
    record_audit_log(
        &mut db,
        &auth,
        "update",
        "work_card",
        work_card.id,
        json!({
            "source": &work_card.source,
            "status": &work_card.status,
            "priority": &work_card.priority,
        }),
    )
    .await?;

    ok(work_card)
}

#[rocket::delete("/work-cards/<id>")]
pub async fn delete_work_card(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
) -> Result<NoContent, ApiError> {
    require_admin(&auth)?;
    let work_card = WorkCardRepository::find(&mut db, id).await?;
    WorkCardRepository::delete(&mut db, id).await?;
    record_audit_log(
        &mut db,
        &auth,
        "delete",
        "work_card",
        id,
        json!({
            "source": &work_card.source,
            "status": &work_card.status,
        }),
    )
    .await?;

    Ok(NoContent)
}
