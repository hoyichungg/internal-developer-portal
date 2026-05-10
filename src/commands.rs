use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
use chrono::{Duration, Utc};
use diesel::prelude::*;
use diesel::result::Error as DieselError;
use diesel::OptionalExtension;
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use serde_json::{json, Value};

use crate::{
    models::{
        ConnectorRun, ConnectorRunStateUpdate, ConnectorUpdate, NewConnector, NewConnectorRun,
        NewConnectorRunItem, NewConnectorRunItemError, NewMaintainer, NewMaintainerMember,
        NewNotification, NewPackage, NewService, NewUser, NewWorkCard,
    },
    repositories::{
        ConnectorConfigRepository, ConnectorRepository, ConnectorRunItemErrorRepository,
        ConnectorRunItemRepository, ConnectorRunRepository, MaintainerMemberRepository,
        MaintainerRepository, NotificationRepository, PackageRepository, RoleRepository,
        ServiceRepository, UserRepository, UserRoleRepository, WorkCardRepository,
    },
    schema::{
        connector_configs, connector_run_item_errors, connector_run_items, connector_runs,
        packages, service_health_checks, users,
    },
};

const DEMO_SOURCE: &str = "demo-workday";
const DEMO_MAINTAINER_EMAIL: &str = "platform-ops@example.test";

struct DemoSeedSummary {
    maintainer_id: i32,
    service_count: usize,
    package_count: usize,
    work_card_count: usize,
    notification_count: usize,
    connector_run_count: usize,
}

async fn load_db_connection() -> AsyncPgConnection {
    let database_url = std::env::var("DATABASE_URL").expect("Cannot load DB url environment");
    AsyncPgConnection::establish(&database_url)
        .await
        .expect("Cannot connect to Postgres")
}

pub async fn create_user(username: String, password: String, role_codes: Vec<String>) {
    let mut c = load_db_connection().await;

    let new_user = NewUser {
        username,
        password: hash_password(&password),
    };
    let user = UserRepository::create(&mut c, new_user, role_codes)
        .await
        .unwrap();
    let roles = RoleRepository::find_by_user(&mut c, &user).await.unwrap();
    println!("User created id={} username={}", user.id, user.username);
    println!("Roles assigned {}", role_codes_summary(&roles));
}

pub async fn ensure_admin_user(
    username: String,
    password: String,
    role_codes: Vec<String>,
    reset_password: bool,
) {
    let mut c = load_db_connection().await;
    let role_codes = normalized_roles(role_codes);
    let user = match UserRepository::find_by_username(&mut c, &username).await {
        Ok(user) if reset_password => {
            UserRepository::update_password(&mut c, user.id, hash_password(&password))
                .await
                .unwrap()
        }
        Ok(user) => user,
        Err(DieselError::NotFound) => {
            let new_user = NewUser {
                username: username.clone(),
                password: hash_password(&password),
            };
            UserRepository::create(&mut c, new_user, Vec::new())
                .await
                .unwrap()
        }
        Err(error) => panic!("Cannot load admin user: {error}"),
    };

    for role_code in role_codes {
        let role = RoleRepository::find_or_create_by_code(&mut c, &role_code)
            .await
            .unwrap();
        UserRoleRepository::assign_if_missing(&mut c, user.id, role.id)
            .await
            .unwrap();
    }

    let roles = RoleRepository::find_by_user(&mut c, &user).await.unwrap();
    println!(
        "Admin user ensured id={} username={}",
        user.id, user.username
    );
    println!("Roles assigned {}", role_codes_summary(&roles));
}

pub async fn list_users() {
    let mut c = load_db_connection().await;

    let users = UserRepository::find_with_roles(&mut c).await.unwrap();
    for (user, roles) in users {
        let role_codes = roles
            .iter()
            .map(|(_, role)| role.code.as_str())
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "id={} username={} roles={}",
            user.id, user.username, role_codes
        );
    }
}

pub async fn delete_user(id: i32) {
    let mut c = load_db_connection().await;

    UserRepository::delete(&mut c, id).await.unwrap();
}

pub async fn seed_demo_data(admin_username: Option<String>) {
    let mut c = load_db_connection().await;
    let admin_username = admin_username
        .or_else(|| std::env::var("SEED_ADMIN_USERNAME").ok())
        .unwrap_or_else(|| "admin".to_owned());

    let summary = c
        .transaction::<DemoSeedSummary, DieselError, _>(|conn| {
            Box::pin(async move { seed_demo_data_transaction(conn, &admin_username).await })
        })
        .await
        .unwrap();

    println!(
        "Demo data seeded source={} maintainer_id={}",
        DEMO_SOURCE, summary.maintainer_id
    );
    println!(
        "Seeded {} services, {} packages, {} work cards, {} notifications, {} connector runs",
        summary.service_count,
        summary.package_count,
        summary.work_card_count,
        summary.notification_count,
        summary.connector_run_count
    );
}

async fn seed_demo_data_transaction(
    c: &mut AsyncPgConnection,
    admin_username: &str,
) -> Result<DemoSeedSummary, DieselError> {
    let now = Utc::now().naive_utc();
    let maintainer = ensure_demo_maintainer(c).await?;

    if let Some(admin) = users::table
        .filter(users::username.eq(admin_username))
        .first::<crate::models::User>(c)
        .await
        .optional()?
    {
        MaintainerMemberRepository::upsert(
            c,
            NewMaintainerMember {
                maintainer_id: maintainer.id,
                user_id: admin.id,
                role: "owner".to_owned(),
            },
        )
        .await?;
    }

    ensure_demo_connector(c, maintainer.id).await?;

    let packages = vec![
        ensure_demo_package(
            c,
            maintainer.id,
            NewPackage {
                maintainer_id: maintainer.id,
                slug: "identity-sdk".to_owned(),
                name: "Identity SDK".to_owned(),
                version: "2.4.1".to_owned(),
                status: "active".to_owned(),
                description: Some("Shared authentication client used by internal apps.".to_owned()),
                repository_url: Some("https://github.com/acme/identity-sdk".to_owned()),
                documentation_url: Some("https://docs.acme.test/identity-sdk".to_owned()),
            },
        )
        .await?,
        ensure_demo_package(
            c,
            maintainer.id,
            NewPackage {
                maintainer_id: maintainer.id,
                slug: "deploy-orchestrator".to_owned(),
                name: "Deploy Orchestrator".to_owned(),
                version: "1.12.0".to_owned(),
                status: "active".to_owned(),
                description: Some("Deployment automation library for service owners.".to_owned()),
                repository_url: Some("https://github.com/acme/deploy-orchestrator".to_owned()),
                documentation_url: Some("https://docs.acme.test/deploy-orchestrator".to_owned()),
            },
        )
        .await?,
    ];

    let health_run = prepare_demo_run(c, "service_health", "import", now).await?;
    clear_demo_run_artifacts(c, health_run.id, true).await?;
    let services = seed_demo_services(c, maintainer.id, health_run.id, now).await?;
    write_imported_run_items(c, health_run.id, "service_health", &services).await?;
    finish_demo_run(c, health_run, "success", services.len(), 0, None, 82).await?;
    ConnectorRepository::touch_run_state(c, DEMO_SOURCE, "monitoring", "success", now).await?;

    let notification_run = prepare_demo_run(c, "notifications", "import", now).await?;
    clear_demo_run_artifacts(c, notification_run.id, false).await?;
    let notifications = seed_demo_notifications(c).await?;
    write_imported_run_items(c, notification_run.id, "notifications", &notifications).await?;
    finish_demo_run(
        c,
        notification_run,
        "success",
        notifications.len(),
        0,
        None,
        34,
    )
    .await?;
    ConnectorRepository::touch_run_state(c, DEMO_SOURCE, "custom", "success", now).await?;

    let work_run = prepare_demo_run(c, "work_cards", "import", now).await?;
    clear_demo_run_artifacts(c, work_run.id, false).await?;
    let work_cards = seed_demo_work_cards(c, now).await?;
    write_imported_run_items(c, work_run.id, "work_cards", &work_cards).await?;
    ConnectorRunItemErrorRepository::create_many(
        c,
        vec![NewConnectorRunItemError {
            connector_run_id: work_run.id,
            source: DEMO_SOURCE.to_owned(),
            target: "work_cards".to_owned(),
            external_id: Some("DP-119".to_owned()),
            message: "priority is not supported".to_owned(),
            raw_item: Some(
                json!({
                    "external_id": "DP-119",
                    "title": "Import malformed backlog item",
                    "status": "todo",
                    "priority": "not-a-priority"
                })
                .to_string(),
            ),
        }],
    )
    .await?;
    ConnectorRunItemRepository::create_many(
        c,
        vec![NewConnectorRunItem {
            connector_run_id: work_run.id,
            source: DEMO_SOURCE.to_owned(),
            target: "work_cards".to_owned(),
            record_id: None,
            external_id: Some("DP-119".to_owned()),
            status: "failed".to_owned(),
            snapshot: Some(
                json!({
                    "external_id": "DP-119",
                    "title": "Import malformed backlog item",
                    "status": "todo",
                    "priority": "not-a-priority"
                })
                .to_string(),
            ),
        }],
    )
    .await?;
    finish_demo_run(
        c,
        work_run,
        "partial_success",
        work_cards.len(),
        1,
        Some("DP-119: priority is not supported".to_owned()),
        57,
    )
    .await?;
    ConnectorRepository::touch_run_state(c, DEMO_SOURCE, "custom", "partial_success", now).await?;

    Ok(DemoSeedSummary {
        maintainer_id: maintainer.id,
        service_count: services.len(),
        package_count: packages.len(),
        work_card_count: work_cards.len(),
        notification_count: notifications.len(),
        connector_run_count: 3,
    })
}

async fn ensure_demo_maintainer(
    c: &mut AsyncPgConnection,
) -> Result<crate::models::Maintainer, DieselError> {
    match crate::schema::maintainers::table
        .filter(crate::schema::maintainers::email.eq(DEMO_MAINTAINER_EMAIL))
        .first::<crate::models::Maintainer>(c)
        .await
        .optional()?
    {
        Some(maintainer) => {
            MaintainerRepository::update(
                c,
                maintainer.id,
                NewMaintainer {
                    display_name: "Platform Operations".to_owned(),
                    email: DEMO_MAINTAINER_EMAIL.to_owned(),
                },
            )
            .await
        }
        None => {
            MaintainerRepository::create(
                c,
                NewMaintainer {
                    display_name: "Platform Operations".to_owned(),
                    email: DEMO_MAINTAINER_EMAIL.to_owned(),
                },
            )
            .await
        }
    }
}

async fn ensure_demo_connector(
    c: &mut AsyncPgConnection,
    maintainer_id: i32,
) -> Result<(), DieselError> {
    match ConnectorRepository::find_by_source(c, DEMO_SOURCE).await {
        Ok(_) => {
            ConnectorRepository::update_by_source(
                c,
                DEMO_SOURCE,
                ConnectorUpdate {
                    kind: "demo".to_owned(),
                    display_name: "Demo Workday Feed".to_owned(),
                    status: "active".to_owned(),
                },
            )
            .await?;
        }
        Err(DieselError::NotFound) => {
            ConnectorRepository::create(
                c,
                NewConnector {
                    source: DEMO_SOURCE.to_owned(),
                    kind: "demo".to_owned(),
                    display_name: "Demo Workday Feed".to_owned(),
                    status: "active".to_owned(),
                },
            )
            .await?;
        }
        Err(error) => return Err(error),
    }

    ConnectorConfigRepository::upsert_by_source(
        c,
        DEMO_SOURCE,
        crate::models::ConnectorConfigUpdate {
            target: "service_health".to_owned(),
            enabled: true,
            schedule_cron: Some("@every 15m".to_owned()),
            config: "{}".to_owned(),
            sample_payload: demo_service_health_payload(maintainer_id, Utc::now().naive_utc())
                .to_string(),
        },
    )
    .await?;
    diesel::update(connector_configs::table.filter(connector_configs::source.eq(DEMO_SOURCE)))
        .set(
            connector_configs::next_run_at.eq(Some(Utc::now().naive_utc() + Duration::minutes(15))),
        )
        .execute(c)
        .await?;

    Ok(())
}

async fn ensure_demo_package(
    c: &mut AsyncPgConnection,
    maintainer_id: i32,
    package: NewPackage,
) -> Result<crate::models::Package, DieselError> {
    let existing = packages::table
        .filter(packages::maintainer_id.eq(maintainer_id))
        .filter(packages::slug.eq(&package.slug))
        .first::<crate::models::Package>(c)
        .await
        .optional()?;

    match existing {
        Some(existing) => PackageRepository::update(c, existing.id, package).await,
        None => PackageRepository::create(c, package).await,
    }
}

async fn prepare_demo_run(
    c: &mut AsyncPgConnection,
    target: &str,
    trigger: &str,
    started_at: chrono::NaiveDateTime,
) -> Result<ConnectorRun, DieselError> {
    let existing = connector_runs::table
        .filter(connector_runs::source.eq(DEMO_SOURCE))
        .filter(connector_runs::target.eq(target))
        .filter(connector_runs::trigger.eq(trigger))
        .first::<ConnectorRun>(c)
        .await
        .optional()?;

    match existing {
        Some(run) => {
            diesel::update(connector_runs::table.find(run.id))
                .set((
                    connector_runs::status.eq("running"),
                    connector_runs::success_count.eq(0),
                    connector_runs::failure_count.eq(0),
                    connector_runs::duration_ms.eq(0_i64),
                    connector_runs::error_message.eq::<Option<String>>(None),
                    connector_runs::started_at.eq(started_at),
                    connector_runs::finished_at.eq::<Option<chrono::NaiveDateTime>>(None),
                    connector_runs::payload.eq::<Option<String>>(None),
                    connector_runs::claimed_at.eq::<Option<chrono::NaiveDateTime>>(None),
                    connector_runs::worker_id.eq::<Option<String>>(None),
                ))
                .get_result(c)
                .await
        }
        None => {
            ConnectorRunRepository::create(
                c,
                NewConnectorRun {
                    source: DEMO_SOURCE.to_owned(),
                    target: target.to_owned(),
                    status: "running".to_owned(),
                    success_count: 0,
                    failure_count: 0,
                    duration_ms: 0,
                    error_message: None,
                    started_at,
                    finished_at: None,
                    trigger: trigger.to_owned(),
                    payload: None,
                    claimed_at: None,
                    worker_id: None,
                },
            )
            .await
        }
    }
}

async fn clear_demo_run_artifacts(
    c: &mut AsyncPgConnection,
    run_id: i32,
    clear_health_checks: bool,
) -> Result<(), DieselError> {
    diesel::delete(
        connector_run_item_errors::table
            .filter(connector_run_item_errors::connector_run_id.eq(run_id)),
    )
    .execute(c)
    .await?;
    diesel::delete(
        connector_run_items::table.filter(connector_run_items::connector_run_id.eq(run_id)),
    )
    .execute(c)
    .await?;

    if clear_health_checks {
        diesel::delete(
            service_health_checks::table.filter(service_health_checks::connector_run_id.eq(run_id)),
        )
        .execute(c)
        .await?;
    }

    Ok(())
}

async fn seed_demo_services(
    c: &mut AsyncPgConnection,
    maintainer_id: i32,
    run_id: i32,
    now: chrono::NaiveDateTime,
) -> Result<Vec<SeededRecord>, DieselError> {
    let items = vec![
        json!({
            "external_id": "checkout-api",
            "maintainer_id": maintainer_id,
            "slug": "checkout-api",
            "name": "Checkout API",
            "lifecycle_status": "active",
            "health_status": "degraded",
            "description": "Handles internal purchase approvals and deployment entitlements.",
            "repository_url": "https://github.com/acme/checkout-api",
            "dashboard_url": "https://grafana.acme.test/d/checkout-api",
            "runbook_url": "https://docs.acme.test/runbooks/checkout-api",
            "last_checked_at": (now - Duration::minutes(8)).format("%Y-%m-%dT%H:%M:%S").to_string()
        }),
        json!({
            "external_id": "build-runner",
            "maintainer_id": maintainer_id,
            "slug": "build-runner",
            "name": "Build Runner Fleet",
            "lifecycle_status": "active",
            "health_status": "healthy",
            "description": "Ephemeral CI runner pool for backend and frontend builds.",
            "repository_url": "https://github.com/acme/build-runner",
            "dashboard_url": "https://grafana.acme.test/d/build-runner",
            "runbook_url": "https://docs.acme.test/runbooks/build-runner",
            "last_checked_at": (now - Duration::minutes(13)).format("%Y-%m-%dT%H:%M:%S").to_string()
        }),
        json!({
            "external_id": "erp-bridge",
            "maintainer_id": maintainer_id,
            "slug": "erp-bridge",
            "name": "ERP Bridge",
            "lifecycle_status": "active",
            "health_status": "down",
            "description": "Synchronizes ERP approvals and private messages into engineering workflows.",
            "repository_url": "https://github.com/acme/erp-bridge",
            "dashboard_url": "https://grafana.acme.test/d/erp-bridge",
            "runbook_url": "https://docs.acme.test/runbooks/erp-bridge",
            "last_checked_at": (now - Duration::minutes(21)).format("%Y-%m-%dT%H:%M:%S").to_string()
        }),
    ];
    let mut records = Vec::new();

    for item in items {
        let service = ServiceRepository::upsert_from_connector_with_health_check(
            c,
            NewService {
                source: DEMO_SOURCE.to_owned(),
                external_id: item["external_id"].as_str().map(ToOwned::to_owned),
                maintainer_id,
                slug: item["slug"].as_str().unwrap().to_owned(),
                name: item["name"].as_str().unwrap().to_owned(),
                lifecycle_status: item["lifecycle_status"].as_str().unwrap().to_owned(),
                health_status: item["health_status"].as_str().unwrap().to_owned(),
                description: item["description"].as_str().map(ToOwned::to_owned),
                repository_url: item["repository_url"].as_str().map(ToOwned::to_owned),
                dashboard_url: item["dashboard_url"].as_str().map(ToOwned::to_owned),
                runbook_url: item["runbook_url"].as_str().map(ToOwned::to_owned),
                last_checked_at: chrono::NaiveDateTime::parse_from_str(
                    item["last_checked_at"].as_str().unwrap(),
                    "%Y-%m-%dT%H:%M:%S",
                )
                .ok(),
            },
            run_id,
            Some(item.to_string()),
        )
        .await?;

        records.push(SeededRecord {
            record_id: Some(service.id),
            external_id: service.external_id,
            snapshot: Some(item.to_string()),
        });
    }

    Ok(records)
}

async fn seed_demo_work_cards(
    c: &mut AsyncPgConnection,
    now: chrono::NaiveDateTime,
) -> Result<Vec<SeededRecord>, DieselError> {
    let items = vec![
        json!({
            "external_id": "DP-104",
            "title": "Investigate Checkout API latency spike",
            "status": "in_progress",
            "priority": "urgent",
            "assignee": "platform-team",
            "due_at": (now + Duration::hours(4)).format("%Y-%m-%dT%H:%M:%S").to_string(),
            "url": "https://dev.azure.test/work-items/DP-104"
        }),
        json!({
            "external_id": "DP-118",
            "title": "Review ERP Bridge deployment exception",
            "status": "blocked",
            "priority": "high",
            "assignee": "release-captain",
            "due_at": (now + Duration::days(1)).format("%Y-%m-%dT%H:%M:%S").to_string(),
            "url": "https://dev.azure.test/work-items/DP-118"
        }),
    ];
    let mut records = Vec::new();

    for item in items {
        let work_card = WorkCardRepository::upsert_from_connector(
            c,
            NewWorkCard {
                source: DEMO_SOURCE.to_owned(),
                external_id: item["external_id"].as_str().map(ToOwned::to_owned),
                title: item["title"].as_str().unwrap().to_owned(),
                status: item["status"].as_str().unwrap().to_owned(),
                priority: item["priority"].as_str().unwrap().to_owned(),
                assignee: item["assignee"].as_str().map(ToOwned::to_owned),
                due_at: chrono::NaiveDateTime::parse_from_str(
                    item["due_at"].as_str().unwrap(),
                    "%Y-%m-%dT%H:%M:%S",
                )
                .ok(),
                url: item["url"].as_str().map(ToOwned::to_owned),
            },
        )
        .await?;

        records.push(SeededRecord {
            record_id: Some(work_card.id),
            external_id: work_card.external_id,
            snapshot: Some(item.to_string()),
        });
    }

    Ok(records)
}

async fn seed_demo_notifications(
    c: &mut AsyncPgConnection,
) -> Result<Vec<SeededRecord>, DieselError> {
    let items = vec![
        json!({
            "external_id": "OUTLOOK-9001",
            "title": "Incident review starts in 30 minutes",
            "body": "Checkout API latency review is on the team calendar.",
            "severity": "critical",
            "is_read": false,
            "url": "https://outlook.office.test/mail/OUTLOOK-9001"
        }),
        json!({
            "external_id": "ERP-4217",
            "title": "ERP access request needs approval",
            "body": "A deployment exception for ERP Bridge is waiting for owner review.",
            "severity": "warning",
            "is_read": false,
            "url": "https://erp.acme.test/messages/4217"
        }),
        json!({
            "external_id": "MON-2048",
            "title": "Build Runner capacity recovered",
            "body": "Runner queue depth is back under the daily threshold.",
            "severity": "info",
            "is_read": true,
            "url": "https://monitoring.acme.test/events/2048"
        }),
    ];
    let mut records = Vec::new();

    for item in items {
        let notification = NotificationRepository::upsert_from_connector(
            c,
            NewNotification {
                source: DEMO_SOURCE.to_owned(),
                external_id: item["external_id"].as_str().map(ToOwned::to_owned),
                title: item["title"].as_str().unwrap().to_owned(),
                body: item["body"].as_str().map(ToOwned::to_owned),
                severity: item["severity"].as_str().unwrap().to_owned(),
                is_read: item["is_read"].as_bool().unwrap_or(false),
                url: item["url"].as_str().map(ToOwned::to_owned),
            },
        )
        .await?;

        records.push(SeededRecord {
            record_id: Some(notification.id),
            external_id: notification.external_id,
            snapshot: Some(item.to_string()),
        });
    }

    Ok(records)
}

async fn write_imported_run_items(
    c: &mut AsyncPgConnection,
    run_id: i32,
    target: &str,
    records: &[SeededRecord],
) -> Result<(), DieselError> {
    ConnectorRunItemRepository::create_many(
        c,
        records
            .iter()
            .map(|record| NewConnectorRunItem {
                connector_run_id: run_id,
                source: DEMO_SOURCE.to_owned(),
                target: target.to_owned(),
                record_id: record.record_id,
                external_id: record.external_id.clone(),
                status: "imported".to_owned(),
                snapshot: record.snapshot.clone(),
            })
            .collect(),
    )
    .await?;

    Ok(())
}

async fn finish_demo_run(
    c: &mut AsyncPgConnection,
    run: ConnectorRun,
    status: &str,
    success_count: usize,
    failure_count: usize,
    error_message: Option<String>,
    duration_ms: i64,
) -> Result<ConnectorRun, DieselError> {
    ConnectorRunRepository::update_state(
        c,
        run.id,
        ConnectorRunStateUpdate {
            status: status.to_owned(),
            success_count: success_count.min(i32::MAX as usize) as i32,
            failure_count: failure_count.min(i32::MAX as usize) as i32,
            duration_ms,
            error_message,
            finished_at: Some(run.started_at + Duration::milliseconds(duration_ms)),
        },
    )
    .await
}

struct SeededRecord {
    record_id: Option<i32>,
    external_id: Option<String>,
    snapshot: Option<String>,
}

fn demo_service_health_payload(maintainer_id: i32, now: chrono::NaiveDateTime) -> Value {
    json!({
        "items": [{
            "external_id": "checkout-api",
            "maintainer_id": maintainer_id,
            "slug": "checkout-api",
            "name": "Checkout API",
            "lifecycle_status": "active",
            "health_status": "degraded",
            "description": "Handles internal purchase approvals and deployment entitlements.",
            "repository_url": "https://github.com/acme/checkout-api",
            "dashboard_url": "https://grafana.acme.test/d/checkout-api",
            "runbook_url": "https://docs.acme.test/runbooks/checkout-api",
            "last_checked_at": (now - Duration::minutes(8)).format("%Y-%m-%dT%H:%M:%S").to_string()
        }, {
            "external_id": "build-runner",
            "maintainer_id": maintainer_id,
            "slug": "build-runner",
            "name": "Build Runner Fleet",
            "lifecycle_status": "active",
            "health_status": "healthy",
            "description": "Ephemeral CI runner pool for backend and frontend builds.",
            "repository_url": "https://github.com/acme/build-runner",
            "dashboard_url": "https://grafana.acme.test/d/build-runner",
            "runbook_url": "https://docs.acme.test/runbooks/build-runner",
            "last_checked_at": (now - Duration::minutes(13)).format("%Y-%m-%dT%H:%M:%S").to_string()
        }, {
            "external_id": "erp-bridge",
            "maintainer_id": maintainer_id,
            "slug": "erp-bridge",
            "name": "ERP Bridge",
            "lifecycle_status": "active",
            "health_status": "down",
            "description": "Synchronizes ERP approvals and private messages into engineering workflows.",
            "repository_url": "https://github.com/acme/erp-bridge",
            "dashboard_url": "https://grafana.acme.test/d/erp-bridge",
            "runbook_url": "https://docs.acme.test/runbooks/erp-bridge",
            "last_checked_at": (now - Duration::minutes(21)).format("%Y-%m-%dT%H:%M:%S").to_string()
        }]
    })
}

fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(OsRng);
    let argon2 = argon2::Argon2::default();

    argon2
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn normalized_roles(role_codes: Vec<String>) -> Vec<String> {
    let mut roles = role_codes
        .into_iter()
        .flat_map(|role| {
            role.split(',')
                .map(str::trim)
                .filter(|role| !role.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    if roles.is_empty() {
        roles.push("admin".to_owned());
        roles.push("member".to_owned());
    }

    roles.sort();
    roles.dedup();
    roles
}

fn role_codes_summary(roles: &[crate::models::Role]) -> String {
    roles
        .iter()
        .map(|role| role.code.as_str())
        .collect::<Vec<_>>()
        .join(",")
}
