use chrono::NaiveDateTime;
use diesel::sql_types::{Integer, Nullable, Text, Timestamp};
use diesel::{sql_query, QueryableByName};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use internal_developer_portal::rocket_routes::connectors::{
    claim_connector_run_for_test, recover_connector_runs_for_test,
    request_connector_run_cancel_for_test,
};

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = Integer)]
    id: i32,
}

#[derive(QueryableByName, Debug)]
struct RunStateRow {
    #[diesel(sql_type = Text)]
    status: String,
    #[diesel(sql_type = Integer)]
    attempt_count: i32,
    #[diesel(sql_type = Integer)]
    max_attempts: i32,
    #[diesel(sql_type = Nullable<Text>)]
    worker_id: Option<String>,
    #[diesel(sql_type = Timestamp)]
    next_attempt_at: NaiveDateTime,
    #[diesel(sql_type = Nullable<Timestamp>)]
    lease_expires_at: Option<NaiveDateTime>,
    #[diesel(sql_type = Nullable<Timestamp>)]
    cancelled_at: Option<NaiveDateTime>,
}

#[tokio::test]
async fn claim_recovery_attempt_limit_and_cancellation_are_atomic() {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("RETENTION_TEST_DATABASE_URL")
        .expect("RETENTION_TEST_DATABASE_URL must point to a dedicated test database");
    let mut db = AsyncPgConnection::establish(&database_url)
        .await
        .expect("the dedicated worker lease test database should be reachable and migrated");
    let mut worker_a = AsyncPgConnection::establish(&database_url)
        .await
        .expect("worker A test connection should open");
    let mut worker_b = AsyncPgConnection::establish(&database_url)
        .await
        .expect("worker B test connection should open");
    let source = format!("worker_lease_test_{}", uuid::Uuid::new_v4().simple());

    let run_id = insert_queued_run(&mut db, &source, 2).await;
    let (claim_a, claim_b) = tokio::join!(
        claim_connector_run_for_test(&mut worker_a, "lease-test-worker-a", 30),
        claim_connector_run_for_test(&mut worker_b, "lease-test-worker-b", 30),
    );
    let claim_a = claim_a.expect("worker A claim should execute");
    let claim_b = claim_b.expect("worker B claim should execute");
    assert_eq!(
        claim_a
            .iter()
            .chain(claim_b.iter())
            .copied()
            .collect::<Vec<_>>(),
        vec![run_id]
    );

    let claimed = load_run(&mut db, run_id).await;
    assert_eq!(claimed.status, "running");
    assert_eq!(claimed.attempt_count, 1);
    assert_eq!(claimed.max_attempts, 2);
    assert!(claimed.worker_id.is_some());
    assert!(claimed.lease_expires_at.is_some());

    expire_lease(&mut db, run_id).await;
    assert_eq!(
        recover_connector_runs_for_test(&mut db, 1, 2)
            .await
            .expect("first recovery should succeed"),
        (1, 0, 0)
    );
    let requeued = load_run(&mut db, run_id).await;
    assert_eq!(requeued.status, "queued");
    assert_eq!(requeued.attempt_count, 1);
    assert!(requeued.worker_id.is_none());
    assert!(requeued.lease_expires_at.is_none());
    assert!(requeued.next_attempt_at >= chrono::Utc::now().naive_utc());

    make_retry_due(&mut db, run_id).await;
    assert_eq!(
        claim_connector_run_for_test(&mut worker_a, "lease-test-worker-a", 30)
            .await
            .expect("second claim should execute"),
        Some(run_id)
    );
    assert_eq!(load_run(&mut db, run_id).await.attempt_count, 2);

    expire_lease(&mut db, run_id).await;
    assert_eq!(
        recover_connector_runs_for_test(&mut db, 1, 2)
            .await
            .expect("terminal recovery should succeed"),
        (0, 1, 0)
    );
    assert_eq!(load_run(&mut db, run_id).await.status, "failed");

    let queued_cancel_id = insert_queued_run(&mut db, &source, 3).await;
    assert_eq!(
        request_connector_run_cancel_for_test(&mut db, queued_cancel_id)
            .await
            .expect("queued cancellation should execute"),
        Some("cancelled".to_owned())
    );
    assert_eq!(
        load_run(&mut db, queued_cancel_id).await.status,
        "cancelled"
    );

    let cancelled_run_id = insert_queued_run(&mut db, &source, 3).await;
    assert_eq!(
        claim_connector_run_for_test(&mut worker_b, "lease-test-worker-b", 30)
            .await
            .expect("cancel test claim should execute"),
        Some(cancelled_run_id)
    );
    assert_eq!(
        request_connector_run_cancel_for_test(&mut db, cancelled_run_id)
            .await
            .expect("running cancellation should execute"),
        Some("running".to_owned())
    );
    expire_lease(&mut db, cancelled_run_id).await;
    assert_eq!(
        recover_connector_runs_for_test(&mut db, 1, 2)
            .await
            .expect("cancel recovery should succeed"),
        (0, 0, 1)
    );
    let cancelled = load_run(&mut db, cancelled_run_id).await;
    assert_eq!(cancelled.status, "cancelled");
    assert!(cancelled.cancelled_at.is_some());

    sql_query("DELETE FROM connector_runs WHERE source = $1")
        .bind::<Text, _>(&source)
        .execute(&mut db)
        .await
        .expect("worker lease fixtures should be cleaned up");
}

async fn insert_queued_run(db: &mut AsyncPgConnection, source: &str, max_attempts: i32) -> i32 {
    sql_query(
        "INSERT INTO connector_runs \
         (source, target, status, started_at, finished_at, trigger, max_attempts, next_attempt_at) \
         VALUES ($1, 'notifications', 'queued', NOW(), NULL, 'manual', $2, \
                 NOW() - INTERVAL '1 minute') \
         RETURNING id",
    )
    .bind::<Text, _>(source)
    .bind::<Integer, _>(max_attempts)
    .get_result::<IdRow>(db)
    .await
    .expect("queued run fixture should insert")
    .id
}

async fn load_run(db: &mut AsyncPgConnection, id: i32) -> RunStateRow {
    sql_query(
        "SELECT status::text, attempt_count, max_attempts, worker_id::text, next_attempt_at, \
         lease_expires_at, cancelled_at FROM connector_runs WHERE id = $1",
    )
    .bind::<Integer, _>(id)
    .get_result(db)
    .await
    .expect("run state should exist")
}

async fn expire_lease(db: &mut AsyncPgConnection, id: i32) {
    sql_query(
        "UPDATE connector_runs SET lease_expires_at = NOW() - INTERVAL '1 second' WHERE id = $1",
    )
    .bind::<Integer, _>(id)
    .execute(db)
    .await
    .expect("run lease should be expired");
}

async fn make_retry_due(db: &mut AsyncPgConnection, id: i32) {
    sql_query(
        "UPDATE connector_runs SET next_attempt_at = NOW() - INTERVAL '1 minute' WHERE id = $1",
    )
    .bind::<Integer, _>(id)
    .execute(db)
    .await
    .expect("requeued run should become due");
}
