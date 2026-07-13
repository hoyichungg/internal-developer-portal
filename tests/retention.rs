use chrono::{DateTime, Duration, Utc};
use diesel::sql_types::{BigInt, Bool, Integer, Text, Timestamptz};
use diesel::{sql_query, QueryableByName};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use internal_developer_portal::rocket_routes::connectors::{
    run_guarded_retention_cleanup_for_test, GuardedRetentionCleanupResult,
    RETENTION_CLEANUP_ADVISORY_LOCK_KEY,
};

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
struct AdvisoryLockRow {
    #[diesel(sql_type = Bool)]
    acquired: bool,
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
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
    let skipped_worker_id = format!("retention-test-skipped-worker-{suffix}");

    // A distinct PostgreSQL session holds the same advisory-lock key.
    // The production xact try-lock must skip immediately and must not create a
    // maintenance history row for work it did not own.
    let mut lock_holder = AsyncPgConnection::establish(&database_url)
        .await
        .expect("the retention lock-holder connection should be reachable");
    assert_safe_destructive_test_database(&mut lock_holder).await;
    let acquired = sql_query("SELECT pg_try_advisory_lock($1) AS acquired")
        .bind::<BigInt, _>(RETENTION_CLEANUP_ADVISORY_LOCK_KEY)
        .get_result::<AdvisoryLockRow>(&mut lock_holder)
        .await
        .expect("the test lock holder should query the retention advisory lock");
    assert!(
        acquired.acquired,
        "the test lock holder should own the lock"
    );

    let skipped = run_guarded_retention_cleanup_for_test(
        &mut db,
        Some(30),
        Some(90),
        Some(30),
        &skipped_worker_id,
    )
    .await
    .expect("lock contention should be a successful skipped attempt");
    assert_eq!(skipped, GuardedRetentionCleanupResult::SkippedLockBusy);
    let skipped_history =
        sql_query("SELECT COUNT(*)::bigint AS count FROM maintenance_runs WHERE worker_id = $1")
            .bind::<Text, _>(&skipped_worker_id)
            .get_result::<CountRow>(&mut db)
            .await
            .expect("skipped maintenance history count should be queryable");
    assert_eq!(skipped_history.count, 0);

    let unlocked = sql_query("SELECT pg_advisory_unlock($1) AS acquired")
        .bind::<BigInt, _>(RETENTION_CLEANUP_ADVISORY_LOCK_KEY)
        .get_result::<AdvisoryLockRow>(&mut lock_holder)
        .await
        .expect("the test lock holder should release the retention advisory lock");
    assert!(unlocked.acquired, "the test lock should be released");

    let stale_at = Utc::now() - Duration::days(120);
    let old_at = Utc::now() - Duration::days(45);
    let fresh_at = Utc::now();

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
    let expired_oidc_state = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let fresh_oidc_state = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    insert_oidc_transaction(&mut db, &expired_oidc_state, old_at).await;
    insert_oidc_transaction(&mut db, &fresh_oidc_state, fresh_at + Duration::hours(1)).await;
    let user_id = insert_user(&mut db, &suffix).await;
    let expired_session_id = insert_session(&mut db, user_id, &suffix, "expired", old_at).await;
    let fresh_session_id = insert_session(
        &mut db,
        user_id,
        &suffix,
        "fresh",
        fresh_at + Duration::hours(1),
    )
    .await;
    let stale_throttle_bucket = fixture_hash("stale-throttle", &suffix);
    let fresh_throttle_bucket = fixture_hash("fresh-throttle", &suffix);
    insert_login_throttle_bucket(&mut db, &stale_throttle_bucket, old_at).await;
    insert_login_throttle_bucket(&mut db, &fresh_throttle_bucket, fresh_at).await;

    // The guarded entry point re-checks APP_ENV and current_database() on this
    // same connection immediately before it reaches any DELETE.
    let cleanup_attempt =
        run_guarded_retention_cleanup_for_test(&mut db, Some(30), Some(90), Some(30), &worker_id)
            .await
            .expect("retention cleanup should run on the dedicated test database");
    let GuardedRetentionCleanupResult::Completed(cleanup) = cleanup_attempt else {
        panic!("retention cleanup should own the advisory lock after it is released");
    };

    assert!(cleanup.0 >= 1);
    assert!(cleanup.1 >= 1);
    assert!(cleanup.2 >= 1);
    assert_eq!(cleanup.3, 1);
    assert!(cleanup.4 >= 1);
    assert!(cleanup.5 >= 1);
    assert!(!row_exists(&mut db, "service_health_checks", old_check_id).await);
    assert!(row_exists(&mut db, "service_health_checks", fresh_check_id).await);
    assert!(row_exists(&mut db, "connector_runs", old_run_id).await);
    assert!(!row_exists(&mut db, "connector_runs", stale_run_id).await);
    assert!(row_exists(&mut db, "connector_runs", fresh_run_id).await);
    assert!(!row_exists(&mut db, "audit_logs", old_audit_id).await);
    assert!(row_exists(&mut db, "audit_logs", fresh_audit_id).await);
    assert!(!oidc_transaction_exists(&mut db, &expired_oidc_state).await);
    assert!(oidc_transaction_exists(&mut db, &fresh_oidc_state).await);
    assert!(!row_exists(&mut db, "sessions", expired_session_id).await);
    assert!(row_exists(&mut db, "sessions", fresh_session_id).await);
    assert!(!login_throttle_bucket_exists(&mut db, &stale_throttle_bucket).await);
    assert!(login_throttle_bucket_exists(&mut db, &fresh_throttle_bucket).await);

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

    let recently_completed_worker_id = format!("retention-test-recent-worker-{suffix}");
    let recently_completed = run_guarded_retention_cleanup_for_test(
        &mut db,
        Some(30),
        Some(90),
        Some(30),
        &recently_completed_worker_id,
    )
    .await
    .expect("a second sequential attempt should be checked against shared history");
    assert_eq!(
        recently_completed,
        GuardedRetentionCleanupResult::SkippedRecentlyCompleted
    );
    let recently_completed_history =
        sql_query("SELECT COUNT(*)::bigint AS count FROM maintenance_runs WHERE worker_id = $1")
            .bind::<Text, _>(&recently_completed_worker_id)
            .get_result::<CountRow>(&mut db)
            .await
            .expect("recently-completed skip history count should be queryable");
    assert_eq!(recently_completed_history.count, 0);

    cleanup_fixture(
        &mut db,
        RetentionFixtureCleanup {
            maintainer_id,
            service_id,
            source: &source,
            suffix: &suffix,
            worker_id: &worker_id,
            fresh_oidc_state: &fresh_oidc_state,
            user_id,
            fresh_throttle_bucket: &fresh_throttle_bucket,
        },
    )
    .await;
}

struct RetentionFixtureCleanup<'a> {
    maintainer_id: i32,
    service_id: i32,
    source: &'a str,
    suffix: &'a str,
    worker_id: &'a str,
    fresh_oidc_state: &'a str,
    user_id: i32,
    fresh_throttle_bucket: &'a str,
}

fn fixture_hash(prefix: &str, suffix: &str) -> String {
    use sha2::{Digest, Sha256};

    format!(
        "{:x}",
        Sha256::digest(format!("{prefix}:{suffix}").as_bytes())
    )
}

async fn insert_user(db: &mut AsyncPgConnection, suffix: &str) -> i32 {
    sql_query(
        "INSERT INTO users (username, password) VALUES ($1, 'retention-test-unused') RETURNING id",
    )
    .bind::<Text, _>(format!("retention-session-{suffix}"))
    .get_result::<IdRow>(db)
    .await
    .expect("session owner fixture should be created")
    .id
}

async fn insert_session(
    db: &mut AsyncPgConnection,
    user_id: i32,
    suffix: &str,
    age: &str,
    expires_at: DateTime<Utc>,
) -> i32 {
    let token_hash = fixture_hash(&format!("{age}-session"), suffix);
    sql_query(
        "INSERT INTO sessions (user_id, token_hash, expires_at, auth_method, last_seen_at) \
         VALUES ($1, $2, $3, 'password', NOW()) RETURNING id",
    )
    .bind::<Integer, _>(user_id)
    .bind::<Text, _>(token_hash)
    .bind::<Timestamptz, _>(expires_at)
    .get_result::<IdRow>(db)
    .await
    .expect("session fixture should be created")
    .id
}

async fn insert_login_throttle_bucket(
    db: &mut AsyncPgConnection,
    bucket_hash: &str,
    updated_at: DateTime<Utc>,
) {
    sql_query(
        "INSERT INTO login_throttle_buckets \
             (bucket_hash, failure_count, window_started_at, locked_until, updated_at) \
         VALUES ($1, 1, $2, NULL, $2)",
    )
    .bind::<Text, _>(bucket_hash)
    .bind::<Timestamptz, _>(updated_at)
    .execute(db)
    .await
    .expect("login-throttle fixture should be created");
}

async fn login_throttle_bucket_exists(db: &mut AsyncPgConnection, bucket_hash: &str) -> bool {
    sql_query(
        "SELECT EXISTS(SELECT 1 FROM login_throttle_buckets WHERE bucket_hash = $1) AS exists",
    )
    .bind::<Text, _>(bucket_hash)
    .get_result::<ExistsRow>(db)
    .await
    .expect("login-throttle fixture existence should be queryable")
    .exists
}

async fn insert_oidc_transaction(
    db: &mut AsyncPgConnection,
    state_hash: &str,
    expires_at: DateTime<Utc>,
) {
    let browser_binding_hash = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    sql_query(
        "INSERT INTO oidc_login_transactions \
             (state_hash, browser_binding_hash, nonce, pkce_verifier_ciphertext, return_to, expires_at) \
         VALUES ($1, $2, 'retention-test-nonce', 'retention-test-ciphertext', '/#dashboard', $3)",
    )
    .bind::<Text, _>(state_hash)
    .bind::<Text, _>(browser_binding_hash)
    .bind::<Timestamptz, _>(expires_at)
    .execute(db)
    .await
    .expect("OIDC transaction fixture should be created");
}

async fn oidc_transaction_exists(db: &mut AsyncPgConnection, state_hash: &str) -> bool {
    sql_query(
        "SELECT EXISTS(SELECT 1 FROM oidc_login_transactions WHERE state_hash = $1) AS exists",
    )
    .bind::<Text, _>(state_hash)
    .get_result::<ExistsRow>(db)
    .await
    .expect("OIDC transaction existence should be queryable")
    .exists
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
    checked_at: DateTime<Utc>,
) -> i32 {
    sql_query(
        "INSERT INTO services (maintainer_id, slug, name, lifecycle_status, health_status, \
                               last_checked_at, source, external_id) \
         VALUES ($1, $2, 'Retention Test Service', 'active', 'healthy', $3, $4, $5) \
         RETURNING id",
    )
    .bind::<Integer, _>(maintainer_id)
    .bind::<Text, _>(format!("retention-{suffix}"))
    .bind::<Timestamptz, _>(checked_at)
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
    finished_at: DateTime<Utc>,
) -> i32 {
    sql_query(
        "INSERT INTO connector_runs (source, target, status, success_count, failure_count, \
                                     duration_ms, started_at, finished_at, trigger) \
         VALUES ($1, 'service_health', 'success', 1, 0, 1, $2, $2, 'manual') \
         RETURNING id",
    )
    .bind::<Text, _>(source)
    .bind::<Timestamptz, _>(finished_at)
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
    checked_at: DateTime<Utc>,
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
    .bind::<Timestamptz, _>(checked_at)
    .get_result::<IdRow>(db)
    .await
    .expect("health-check fixture should be created")
    .id
}

async fn insert_audit_log(
    db: &mut AsyncPgConnection,
    suffix: &str,
    age: &str,
    created_at: DateTime<Utc>,
) -> i32 {
    sql_query(
        "INSERT INTO audit_logs (action, resource_type, resource_id, created_at) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id",
    )
    .bind::<Text, _>(format!("retention_{age}"))
    .bind::<Text, _>(format!("retention_test_{suffix}"))
    .bind::<Text, _>(format!("{age}-{suffix}"))
    .bind::<Timestamptz, _>(created_at)
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
        "sessions" => "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = $1) AS exists",
        _ => panic!("unsupported fixture table: {table}"),
    };

    sql_query(query)
        .bind::<Integer, _>(id)
        .get_result::<ExistsRow>(db)
        .await
        .expect("fixture existence should be queryable")
        .exists
}

async fn cleanup_fixture(db: &mut AsyncPgConnection, fixture: RetentionFixtureCleanup<'_>) {
    // Keep the environment and actual current_database() checks adjacent to
    // teardown. No DELETE statement may be added above this guard.
    assert_safe_destructive_test_database(db).await;

    sql_query("DELETE FROM oidc_login_transactions WHERE state_hash = $1")
        .bind::<Text, _>(fixture.fresh_oidc_state)
        .execute(db)
        .await
        .expect("OIDC transaction fixture should be removed");

    sql_query("DELETE FROM login_throttle_buckets WHERE bucket_hash = $1")
        .bind::<Text, _>(fixture.fresh_throttle_bucket)
        .execute(db)
        .await
        .expect("login-throttle fixture should be removed");
    sql_query("DELETE FROM users WHERE id = $1")
        .bind::<Integer, _>(fixture.user_id)
        .execute(db)
        .await
        .expect("session owner fixture should be removed");

    sql_query("DELETE FROM services WHERE id = $1")
        .bind::<Integer, _>(fixture.service_id)
        .execute(db)
        .await
        .expect("service fixture should be removed");
    sql_query("DELETE FROM connector_runs WHERE source = $1")
        .bind::<Text, _>(fixture.source)
        .execute(db)
        .await
        .expect("connector-run fixtures should be removed");
    sql_query("DELETE FROM audit_logs WHERE resource_type = $1")
        .bind::<Text, _>(format!("retention_test_{}", fixture.suffix))
        .execute(db)
        .await
        .expect("audit-log fixtures should be removed");
    sql_query("DELETE FROM maintenance_runs WHERE worker_id = $1")
        .bind::<Text, _>(fixture.worker_id)
        .execute(db)
        .await
        .expect("maintenance-run fixture should be removed");
    sql_query("DELETE FROM maintainers WHERE id = $1")
        .bind::<Integer, _>(fixture.maintainer_id)
        .execute(db)
        .await
        .expect("maintainer fixture should be removed");
}
