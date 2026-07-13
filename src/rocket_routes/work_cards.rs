use crate::api::{created, ok, ApiError, ApiResult, CreatedApiResult};
use crate::auth::{record_access_scope, require_admin, AuthenticatedUser};
use crate::models::{NewWorkCard, WorkCard};
use crate::repositories::{MyWorkCardFilters, WorkCardDueFilter, WorkCardRepository, WorkCardSort};
use crate::rocket_routes::audit_logs::record_audit_log;
use crate::rocket_routes::DbConn;
use crate::validation::validate_request;
use rocket::form::FromForm;
use rocket::response::status::NoContent;
use rocket::serde::json::Json;
use rocket_db_pools::Connection;
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;

use chrono::{Duration, Utc};

use crate::validation::{FieldViolation, Validate};

const DEFAULT_MY_WORK_PAGE_SIZE: i64 = 25;
const MAX_MY_WORK_PAGE_SIZE: i64 = 100;
const MAX_MY_WORK_PAGE: i64 = 1_000_000;

#[derive(FromForm)]
pub struct MyWorkCardQuery {
    pub status: Option<String>,
    pub due: Option<String>,
    pub project: Option<String>,
    pub work_item_type: Option<String>,
    pub source: Option<String>,
    pub sort: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

impl Validate for MyWorkCardQuery {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();
        validate_optional_choice(
            &mut errors,
            "status",
            self.status.as_deref(),
            &["todo", "in_progress", "blocked", "done"],
        );
        validate_optional_choice(
            &mut errors,
            "due",
            self.due.as_deref(),
            &["overdue", "today", "next_7_days", "none"],
        );
        validate_optional_choice(
            &mut errors,
            "sort",
            self.sort.as_deref(),
            &["attention", "due_asc", "source_updated_desc"],
        );
        crate::validation::max_optional_len(&mut errors, "project", &self.project, 128);
        crate::validation::max_optional_len(
            &mut errors,
            "work_item_type",
            &self.work_item_type,
            128,
        );
        crate::validation::max_optional_len(&mut errors, "source", &self.source, 64);
        for (field, value) in [
            ("project", self.project.as_deref()),
            ("work_item_type", self.work_item_type.as_deref()),
            ("source", self.source.as_deref()),
        ] {
            if value.is_some_and(|value| value.trim().is_empty()) {
                errors.push(FieldViolation::new(
                    field,
                    "must not be empty when provided",
                ));
            }
        }
        if self
            .page
            .is_some_and(|page| !(1..=MAX_MY_WORK_PAGE).contains(&page))
        {
            errors.push(FieldViolation::new(
                "page",
                format!("must be from 1 to {MAX_MY_WORK_PAGE}"),
            ));
        }
        if self
            .page_size
            .is_some_and(|page_size| !(1..=MAX_MY_WORK_PAGE_SIZE).contains(&page_size))
        {
            errors.push(FieldViolation::new(
                "page_size",
                format!("must be from 1 to {MAX_MY_WORK_PAGE_SIZE}"),
            ));
        }

        errors
    }
}

#[derive(Serialize, ToSchema)]
pub struct MyWorkCardFacets {
    pub statuses: Vec<String>,
    pub projects: Vec<String>,
    pub work_item_types: Vec<String>,
    pub sources: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct MyWorkCardsResponse {
    pub items: Vec<WorkCard>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub facets: MyWorkCardFacets,
}

fn validate_optional_choice(
    errors: &mut Vec<FieldViolation>,
    field: &'static str,
    value: Option<&str>,
    allowed: &[&str],
) {
    if let Some(value) = value {
        let value = value.trim();
        if !allowed.contains(&value) {
            errors.push(FieldViolation::new(
                field,
                format!("must be one of {}", allowed.join(", ")),
            ));
        }
    }
}

fn trimmed(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_owned())
}

#[rocket::get("/work-cards")]
pub async fn get_work_cards(
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
) -> ApiResult<Vec<WorkCard>> {
    let access = record_access_scope(&mut db, &auth).await?;
    let work_cards = WorkCardRepository::find_multiple_for_access(&mut db, 100, &access).await?;
    ok(work_cards)
}

#[rocket::get("/me/work-cards?<query..>")]
pub async fn get_my_work_cards(
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
    query: MyWorkCardQuery,
) -> ApiResult<MyWorkCardsResponse> {
    let query = validate_request(query)?;
    let access = record_access_scope(&mut db, &auth).await?;
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(DEFAULT_MY_WORK_PAGE_SIZE);
    let today_start = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_utc();
    let filters = MyWorkCardFilters {
        status: trimmed(query.status),
        due: match query.due.as_deref().map(str::trim) {
            Some("overdue") => Some(WorkCardDueFilter::Overdue),
            Some("today") => Some(WorkCardDueFilter::Today),
            Some("next_7_days") => Some(WorkCardDueFilter::NextSevenDays),
            Some("none") => Some(WorkCardDueFilter::None),
            Some(_) => return Err(ApiError::BadRequest),
            None => None,
        },
        project: trimmed(query.project),
        work_item_type: trimmed(query.work_item_type),
        source: trimmed(query.source),
        sort: match query.sort.as_deref().map(str::trim) {
            Some("due_asc") => WorkCardSort::DueAsc,
            Some("source_updated_desc") => WorkCardSort::SourceUpdatedDesc,
            Some("attention") | None => WorkCardSort::Attention,
            Some(_) => return Err(ApiError::BadRequest),
        },
        today_start,
        tomorrow_start: today_start + Duration::days(1),
        next_seven_days_end: today_start + Duration::days(7),
    };

    let result =
        WorkCardRepository::find_my_work(&mut db, &access, &filters, page, page_size).await?;
    let facets = WorkCardRepository::my_work_facets(&mut db, &access).await?;

    ok(MyWorkCardsResponse {
        items: result.items,
        total: result.total,
        page,
        page_size,
        facets: MyWorkCardFacets {
            statuses: facets.statuses,
            projects: facets.projects,
            work_item_types: facets.work_item_types,
            sources: facets.sources,
        },
    })
}

#[rocket::get("/work-cards/<id>")]
pub async fn view_work_card(
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
    id: i32,
) -> ApiResult<WorkCard> {
    let work_card = WorkCardRepository::find(&mut db, id).await?;
    let access = record_access_scope(&mut db, &auth).await?;
    if !access.allows(work_card.owner_user_id, work_card.maintainer_id) {
        return Err(ApiError::NotFound);
    }
    ok(work_card)
}

#[rocket::post("/work-cards", format = "json", data = "<new_work_card>")]
pub async fn create_work_card(
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
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
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
    id: i32,
    work_card: Json<NewWorkCard>,
) -> ApiResult<WorkCard> {
    require_admin(&auth)?;
    let mut work_card = validate_request(work_card.into_inner())?;
    let existing = WorkCardRepository::find(&mut db, id).await?;
    work_card.connector_id = existing.connector_id;
    work_card.owner_user_id = existing.owner_user_id;
    work_card.maintainer_id = existing.maintainer_id;
    work_card.assignee_source_id = existing.assignee_source_id;
    work_card.assignee_user_id = existing.assignee_user_id;
    work_card.source_updated_at = existing.source_updated_at;
    work_card.last_seen_run_id = existing.last_seen_run_id;
    work_card.archived_at = existing.archived_at;
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
    auth: AuthenticatedUser,
    mut db: Connection<DbConn>,
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
