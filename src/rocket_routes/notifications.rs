use crate::api::{created, ok, ApiError, ApiResult, CreatedApiResult};
use crate::auth::{require_admin, AuthenticatedUser};
use crate::models::{NewNotification, Notification};
use crate::repositories::NotificationRepository;
use crate::rocket_routes::audit_logs::record_audit_log;
use crate::rocket_routes::DbConn;
use crate::validation::validate_request;
use rocket::response::status::NoContent;
use rocket::serde::json::Json;
use rocket_db_pools::Connection;
use serde_json::json;

#[rocket::get("/notifications")]
pub async fn get_notifications(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
) -> ApiResult<Vec<Notification>> {
    let notifications = NotificationRepository::find_multiple(&mut db, 100).await?;
    ok(notifications)
}

#[rocket::get("/notifications/<id>")]
pub async fn view_notification(
    mut db: Connection<DbConn>,
    _auth: AuthenticatedUser,
    id: i32,
) -> ApiResult<Notification> {
    let notification = NotificationRepository::find(&mut db, id).await?;
    ok(notification)
}

#[rocket::post("/notifications", format = "json", data = "<new_notification>")]
pub async fn create_notification(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    new_notification: Json<NewNotification>,
) -> CreatedApiResult<Notification> {
    require_admin(&auth)?;
    let new_notification = validate_request(new_notification.into_inner())?;
    let notification = NotificationRepository::create(&mut db, new_notification).await?;
    record_audit_log(
        &mut db,
        &auth,
        "create",
        "notification",
        notification.id,
        json!({
            "source": &notification.source,
            "severity": &notification.severity,
            "is_read": notification.is_read,
        }),
    )
    .await?;

    created(notification)
}

#[rocket::put("/notifications/<id>", format = "json", data = "<notification>")]
pub async fn update_notification(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
    notification: Json<NewNotification>,
) -> ApiResult<Notification> {
    require_admin(&auth)?;
    let notification = validate_request(notification.into_inner())?;
    let notification = NotificationRepository::update(&mut db, id, notification).await?;
    record_audit_log(
        &mut db,
        &auth,
        "update",
        "notification",
        notification.id,
        json!({
            "source": &notification.source,
            "severity": &notification.severity,
            "is_read": notification.is_read,
        }),
    )
    .await?;

    ok(notification)
}

#[rocket::delete("/notifications/<id>")]
pub async fn delete_notification(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
    id: i32,
) -> Result<NoContent, ApiError> {
    require_admin(&auth)?;
    let notification = NotificationRepository::find(&mut db, id).await?;
    NotificationRepository::delete(&mut db, id).await?;
    record_audit_log(
        &mut db,
        &auth,
        "delete",
        "notification",
        id,
        json!({
            "source": &notification.source,
            "severity": &notification.severity,
        }),
    )
    .await?;

    Ok(NoContent)
}
