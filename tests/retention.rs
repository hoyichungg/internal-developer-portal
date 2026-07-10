use chrono::{Duration, NaiveDateTime, Utc};
use diesel::sql_types::{BigInt, Bool, Integer, Text, Timestamp};
use diesel::{sql_query, QueryableByName};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use internal_developer_portal::rocket_routes::connectors::run_guarded_retention_cleanup_for_test;

#[derive(QueryableByName)]
struct CurrentDatabase {
    #[diesel(sql_type = Text)]
    database_name: String,
}

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = Integer)]
    id: i32,
}

#[derive(QueryableByName)]
struct ExistsRow {
    #[diesel(sql_type = Bool)]
    exists: bool,
}

#[derive(QueryableByName)]
struct MaintenanceRunRow {
    #[diesel(sql_type = Text)]
    status: String,
    #[diesel(sql_type = Integer)]
    health_checks_deleted: i32,
    #[diesel(sql_type = Integer)]
    connector_runs_deleted: i32,
    #[diesel(sql_type = Integer)]
    audit_logs_deleted: i32,
    #[diesel(sql_type = BigInt)]
    duration_ms: i64,
}

#[tokio::test]
async fn retention_cleanup_deletes_only_expired_records() {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("RETENTION_TEST_DATABASE_URL")
        .expect("RETENTION_TEST_DATABASE_URL must point to a dedicated test database");
    let mut db = AsyncPgConnection::establish(&database_url)
        .await
        .expect("the dedicated retention test database should be reachable and migrated");
    assert_safe_destructive_test_database(&mut db).await;

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let source = format!("retention_test_{suffix}");
    let worker_id = format!("retention-test-worker-{suffix}");
    let stale_at = Utc::now().naive_utc() - Duration::days(120);
    let old_at = Utc::now().naive_utc() - Duration::days(45);
    let fresh_at = Utc::now().naive_utc();

    let maintainer_id = insert_maintainer(&mut db, &suffix).await;
    let service_id = insert_service(&mut db, maintainer_id, &source, &suffix, fresh_at).await;
    let old_run_id = insert_finished_run(&mut db, &source, old_at).await;
    let stale_run_id = insert_finished_run(&mut db, &source, stale_at).await;
    let fresh_run_id = insert_finished_run(&mut db, &source, fresh_at).await;
    let old_check_id = insert_health_check(&mut db, service_id, old_run_id, &source, old_at).await;
    let fresh_check_id =
        insert_health_check(&mut db, service_id, fresh_run_id, &source, fresh_at).await;
    let old_audit_id = insert_audit_log(&mut db, &suffix, "old", old_at).await;
    let fresh_audit_id = insert_audit_log(&mut db, &suffix, "fresh", fresh_at).await;

    // The guarded entry point re-checks APP_ENV and current_database() on this
    // same connection immediately before it reaches any DELETE.
    let cleanup =
        run_guarded_retention_cleanup_for_test(&mut db, Some(30), Some(90), Some(30), &worker_id)
            .await
            .expect("retention cleanup should run on the dedicated test database");

    assert!(cleanup.0 >= 1);
    assert!(cleanup.1 >= 1);
    assert!(cleanup.2 >= 1);
    assert!(!row_exists(&mut db, "service_health_checks", old_check_id).await);
    assert!(row_exists(&mut db, "service_health_checks", fresh_check_id).await);
    assert!(row_exists(&mut db, "connector_runs", old_run_id).await);
    assert!(!row_exists(&mut db, "connector_runs", stale_run_id).await);
    assert!(row_exists(&mut db, "connector_runs", fresh_run_id).await);
    assert!(!row_exists(&mut db, "audit_logs", old_audit_id).await);
    assert!(row_exists(&mut db, "audit_logs", fresh_audit_id).await);

    let maintenance = sql_query(
        "SELECT status, health_checks_deleted, connector_runs_deleted, \
                audit_logs_deleted, duration_ms \
         FROM maintenance_runs \
         WHERE worker_id = $1 \
         ORDER BY id DESC \
         LIMIT 1",
    )
    .bind::<Text, _>(&worker_id)
    .get_result::<MaintenanceRunRow>(&mut db)
    .await
    .expect("retention cleanup should write maintenance history");
    assert_eq!(maintenance.status, "success");
    assert_eq!(maintenance.health_checks_deleted, cleanup.0 as i32);
    assert_eq!(maintenance.connector_runs_deleted, cleanup.1 as i32);
    assert_eq!(maintenance.audit_logs_deleted, cleanup.2 as i32);
    assert!(maintenance.duration_ms >= 0);

    cleanup_fixture(
        &mut db,
        maintainer_id,
        service_id,
        &source,
        &suffix,
        &worker_id,
    )
    .await;
}

async fn assert_safe_destructive_test_database(db: &mut AsyncPgConnection) {
    let app_env = std::env::var("APP_ENV").unwrap_or_default();
    assert_eq!(
        app_env, "test",
        "refusing destructive retention test: APP_ENV must be exactly 'test'"
    );

    let database_name = sql_query("SELECT current_database()::text AS database_name")
        .get_result::<CurrentDatabase>(db)
        .await
        .expect("current database name should be readable")
        .database_name;
    let has_test_segment = database_name
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|segment| segment == "test");
    assert!(
        has_test_segment,
        "refusing destructive retention test: database '{database_name}' must contain a standalone 'test' segment"
    );
}

async fn insert_maintainer(db: &mut AsyncPgConnection, suffix: &str) -> i32 {
    sql_query(
        "INSERT INTO maintainers (display_name, email) \
         VALUES ($1, $2) \
         RETURNING id",
    )
    .bind::<Text, _>(format!("Retention Test {suffix}"))
    .bind::<Text, _>(format!("retention-{suffix}@example.test"))
    .get_result::<IdRow>(db)
    .await
    .expect("maintainer fixture should be created")
    .id
}

async fn insert_service(
    db: &mut AsyncPgConnection,
    maintainer_id: i32,
    source: &str,
    suffix: &str,
    checked_at: NaiveDateTime,
) -> i32 {
    sql_query(
        "INSERT INTO services (maintainer_id, slug, name, lifecycle_status, health_status, \
                               last_checked_at, source, external_id) \
         VALUES ($1, $2, 'Retention Test Service', 'active', 'healthy', $3, $4, $5) \
         RETURNING id",
    )
    .bind::<Integer, _>(maintainer_id)
    .bind::<Text, _>(format!("retention-{suffix}"))
    .bind::<Timestamp, _>(checked_at)
    .bind::<Text, _>(source)
    .bind::<Text, _>(format!("service-{suffix}"))
    .get_result::<IdRow>(db)
    .await
    .expect("service fixture should be created")
    .id
}

async fn insert_finished_run(
    db: &mut AsyncPgConnection,
    source: &str,
    finished_at: NaiveDateTime,
) -> i32 {
    sql_query(
        "INSERT INTO connector_runs (source, target, status, success_count, failure_count, \
                                     duration_ms, started_at, finished_at, trigger) \
         VALUES ($1, 'service_health', 'success', 1, 0, 1, $2, $2, 'manual') \
         RETURNING id",
    )
    .bind::<Text, _>(source)
    .bind::<Timestamp, _>(finished_at)
    .get_result::<IdRow>(db)
    .await
    .expect("connector run fixture should be created")
    .id
}

async fn insert_health_check(
    db: &mut AsyncPgConnection,
    service_id: i32,
    connector_run_id: i32,
    source: &str,
    checked_at: NaiveDateTime,
) -> i32 {
    sql_query(
        "INSERT INTO service_health_checks \
             (service_id, connector_run_id, source, health_status, checked_at) \
         VALUES ($1, $2, $3, 'healthy', $4) \
         RETURNING id",
    )
    .bind::<Integer, _>(service_id)
    .bind::<Integer, _>(connector_run_id)
    .bind::<Text, _>(source)
    .bind::<Timestamp, _>(checked_at)
    .get_result::<IdRow>(db)
    .await
    .expect("health-check fixture should be created")
    .id
}

async fn insert_audit_log(
    db: &mut AsyncPgConnection,
    suffix: &str,
    age: &str,
    created_at: NaiveDateTime,
) -> i32 {
    sql_query(
        "INSERT INTO audit_logs (action, resource_type, resource_id, created_at) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id",
    )
    .bind::<Text, _>(format!("retention_{age}"))
    .bind::<Text, _>(format!("retention_test_{suffix}"))
    .bind::<Text, _>(format!("{age}-{suffix}"))
    .bind::<Timestamp, _>(created_at)
    .get_result::<IdRow>(db)
    .await
    .expect("audit-log fixture should be created")
    .id
}

async fn row_exists(db: &mut AsyncPgConnection, table: &str, id: i32) -> bool {
    let query = match table {
        "service_health_checks" => {
            "SELECT EXISTS(SELECT 1 FROM service_health_checks WHERE id = $1) AS exists"
        }
        "connector_runs" => "SELECT EXISTS(SELECT 1 FROM connector_runs WHERE id = $1) AS exists",
        "audit_logs" => "SELECT EXISTS(SELECT 1 FROM audit_logs WHERE id = $1) AS exists",
        _ => panic!("unsupported fixture table: {table}"),
    };

    sql_query(query)
        .bind::<Integer, _>(id)
        .get_result::<ExistsRow>(db)
        .await
        .expect("fixture existence should be queryable")
        .exists
}

async fn cleanup_fixture(
    db: &mut AsyncPgConnection,
    maintainer_id: i32,
    service_id: i32,
    source: &str,
    suffix: &str,
    worker_id: &str,
) {
    // Keep the environment and actual current_database() checks adjacent to
    // teardown. No DELETE statement may be added above this guard.
    assert_safe_destructive_test_database(db).await;

    sql_query("DELETE FROM services WHERE id = $1")
        .bind::<Integer, _>(service_id)
        .execute(db)
        .await
        .expect("service fixture should be removed");
    sql_query("DELETE FROM connector_runs WHERE source = $1")
        .bind::<Text, _>(source)
        .execute(db)
        .await
        .expect("connector-run fixtures should be removed");
    sql_query("DELETE FROM audit_logs WHERE resource_type = $1")
        .bind::<Text, _>(format!("retention_test_{suffix}"))
        .execute(db)
        .await
        .expect("audit-log fixtures should be removed");
    sql_query("DELETE FROM maintenance_runs WHERE worker_id = $1")
        .bind::<Text, _>(worker_id)
        .execute(db)
        .await
        .expect("maintenance-run fixture should be removed");
    sql_query("DELETE FROM maintainers WHERE id = $1")
        .bind::<Integer, _>(maintainer_id)
        .execute(db)
        .await
        .expect("maintainer fixture should be removed");
}
