use chrono::{Duration, Utc};
use rocket_db_pools::Connection;

use crate::api::{ok, ApiError, ApiResult};
use crate::auth::{record_access_scope, AuthenticatedUser};
use crate::models::CalendarEvent;
use crate::repositories::CalendarEventRepository;
use crate::rocket_routes::DbConn;

const CALENDAR_WINDOW_BEFORE_HOURS: i64 = 18;
const CALENDAR_WINDOW_AFTER_HOURS: i64 = 42;

#[rocket::get("/calendar-events")]
pub async fn get_calendar_events(
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
) -> ApiResult<Vec<CalendarEvent>> {
    let access = record_access_scope(&mut db, &auth).await?;
    let now = Utc::now();
    let events = CalendarEventRepository::find_upcoming_for_access(
        &mut db,
        100,
        now - Duration::hours(CALENDAR_WINDOW_BEFORE_HOURS),
        now + Duration::hours(CALENDAR_WINDOW_AFTER_HOURS),
        None,
        None,
        &access,
    )
    .await?;
    ok(events)
}

#[rocket::get("/calendar-events/<id>")]
pub async fn view_calendar_event(
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
    id: i32,
) -> ApiResult<CalendarEvent> {
    let event = CalendarEventRepository::find(&mut db, id).await?;
    let access = record_access_scope(&mut db, &auth).await?;
    if event.archived_at.is_some() || !access.allows(event.owner_user_id, event.maintainer_id) {
        return Err(ApiError::NotFound);
    }
    ok(event)
}
