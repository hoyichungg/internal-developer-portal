use std::{error::Error, io, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use diesel::sql_types::{BigInt, Bool, Integer, Nullable, Text, Timestamptz};
use diesel::{sql_query, QueryableByName};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};

use crate::{
    config::validate_test_database_url,
    models::{NewExternalIdentity, NewOidcLoginTransaction, NewUser},
    repositories::{
        CreateSession, ExternalIdentityRepository, OidcLoginTransactionRepository,
        SessionRepository, UserRepository,
    },
};
use tokio::sync::Barrier;

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(QueryableByName)]
struct CurrentDatabase {
    #[diesel(sql_type = Text)]
    database_name: String,
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

#[derive(QueryableByName)]
struct ExistsRow {
    #[diesel(sql_type = Bool)]
    exists: bool,
}

#[derive(QueryableByName)]
struct AdvisoryLockRow {
    #[diesel(sql_type = Text)]
    #[allow(dead_code)]
    lock_result: String,
}

#[derive(QueryableByName)]
struct AdvisoryUnlockRow {
    #[diesel(sql_type = Bool)]
    unlocked: bool,
}

#[derive(Debug, QueryableByName)]
struct IdentityRow {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Integer)]
    user_id: i32,
}

#[derive(Debug, QueryableByName)]
struct UserRow {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    username: String,
}

#[derive(Debug, QueryableByName)]
struct AuditRow {
    #[diesel(sql_type = Nullable<Integer>)]
    actor_user_id: Option<i32>,
    #[diesel(sql_type = Nullable<Text>)]
    resource_id: Option<String>,
}

struct OidcObservation {
    baseline_pending: i64,
    first_state: String,
    first_browser_binding: String,
    expired_was_deleted: bool,
    cap_rejected_third: bool,
    wrong_browser_was_not_found: bool,
    first_survived_wrong_browser: bool,
    consumed_state: String,
    replay_was_not_found: bool,
}

struct ProvisionedResult {
    user_id: i32,
    username: String,
    identity_id: i32,
    identity_user_id: i32,
    created: bool,
}

struct JitObservation {
    first: ProvisionedResult,
    second: ProvisionedResult,
    identities: Vec<IdentityRow>,
    users: Vec<UserRow>,
    member_membership_count: i64,
    audits: Vec<AuditRow>,
}

struct UsernameObservation {
    legacy_username: String,
    canonical_created_username: String,
    case_duplicate_rejected_by_database: bool,
    case_duplicate_rejected_by_repository: bool,
    surrounding_whitespace_rejected_by_database: bool,
}

#[tokio::test]
async fn oidc_transactions_enforce_capacity_cleanup_browser_binding_and_single_use() {
    dotenvy::dotenv().ok();
    let database_url = integration_test_database_url();
    let mut db = AsyncPgConnection::establish(&database_url)
        .await
        .expect("the dedicated OIDC repository test database should be reachable and migrated");
    assert_safe_test_database(&mut db).await;

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let expired_state = format!("expired-{suffix}");
    let first_state = format!("first-{suffix}");
    let second_state = format!("second-{suffix}");
    let capped_state = format!("capped-{suffix}");
    let fixture_states = [
        expired_state.as_str(),
        first_state.as_str(),
        second_state.as_str(),
        capped_state.as_str(),
    ];

    acquire_oidc_capacity_lock(&mut db)
        .await
        .expect("the OIDC capacity test lock should be acquired");
    let exercise_result = exercise_oidc_repository(
        &mut db,
        &expired_state,
        &first_state,
        &second_state,
        &capped_state,
        &suffix,
    )
    .await;

    // Always remove the dynamic fixture before evaluating behavioral assertions.
    // Keeping this guard adjacent to the DELETE prevents an accidental app-DB cleanup.
    assert_safe_test_database(&mut db).await;
    let cleanup_result = delete_oidc_fixtures(&mut db, &fixture_states).await;
    let cleanup_count = count_oidc_fixtures(&mut db, &fixture_states).await;
    let unlock_result = release_oidc_capacity_lock(&mut db).await;

    cleanup_result.expect("OIDC repository fixtures should be removed");
    assert_eq!(
        cleanup_count.expect("OIDC fixture cleanup should be verifiable"),
        0,
        "OIDC repository test left dynamic rows behind"
    );
    assert!(
        unlock_result.expect("the OIDC capacity test lock should be released"),
        "the current database session did not own the OIDC test lock"
    );

    let observed = exercise_result.expect("OIDC repository behavior should be queryable");
    assert!(observed.baseline_pending >= 0);
    assert!(observed.expired_was_deleted);
    assert!(observed.cap_rejected_third);
    assert!(observed.wrong_browser_was_not_found);
    assert!(observed.first_survived_wrong_browser);
    assert_eq!(observed.consumed_state, observed.first_state);
    assert!(observed.replay_was_not_found);
    assert_eq!(observed.first_browser_binding, format!("browser-{suffix}"));
}

#[tokio::test]
async fn jit_identity_provisioning_is_exactly_once_across_two_connections() {
    dotenvy::dotenv().ok();
    let database_url = integration_test_database_url();
    let mut observer = AsyncPgConnection::establish(&database_url)
        .await
        .expect("the dedicated JIT repository test database should be reachable and migrated");
    let mut contender_a = AsyncPgConnection::establish(&database_url)
        .await
        .expect("JIT contender A should connect");
    let mut contender_b = AsyncPgConnection::establish(&database_url)
        .await
        .expect("JIT contender B should connect");
    assert_safe_test_database(&mut observer).await;

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let tenant_id = uuid::Uuid::new_v4().to_string();
    let object_id = uuid::Uuid::new_v4().to_string();
    let subject = format!("subject-{suffix}");
    let issuer = format!("https://login.example.test/{tenant_id}/v2.0");
    let username_a = format!("jit-a-{suffix}");
    let username_b = format!("jit-b-{suffix}");
    let member_role_preexisted = role_exists(&mut observer, "member")
        .await
        .expect("member role baseline should be queryable");

    let (first_result, second_result) = tokio::join!(
        ExternalIdentityRepository::find_or_create_jit_user(
            &mut contender_a,
            new_test_user(&username_a),
            new_test_identity(&tenant_id, &object_id, &issuer, &subject),
        ),
        ExternalIdentityRepository::find_or_create_jit_user(
            &mut contender_b,
            new_test_user(&username_b),
            new_test_identity(&tenant_id, &object_id, &issuer, &subject),
        ),
    );

    let exercise_result = observe_jit_results(
        &mut observer,
        first_result,
        second_result,
        &tenant_id,
        &object_id,
        &username_a,
        &username_b,
    )
    .await;

    // Cleanup runs even when either contender or any observation reports an error.
    assert_safe_test_database(&mut observer).await;
    let cleanup_result = cleanup_jit_fixture(
        &mut observer,
        &tenant_id,
        &object_id,
        &username_a,
        &username_b,
        member_role_preexisted,
    )
    .await;
    let cleanup_state = jit_fixture_counts(
        &mut observer,
        &tenant_id,
        &object_id,
        &username_a,
        &username_b,
    )
    .await;

    cleanup_result.expect("JIT repository fixtures should be removed");
    let (identity_count, user_count, audit_count) =
        cleanup_state.expect("JIT fixture cleanup should be verifiable");
    assert_eq!(identity_count, 0, "JIT identity fixture was not removed");
    assert_eq!(user_count, 0, "JIT user fixture was not removed");
    assert_eq!(audit_count, 0, "JIT audit fixture was not removed");

    let observed = exercise_result.expect("both concurrent JIT provisions should succeed");
    assert_eq!(
        usize::from(observed.first.created) + usize::from(observed.second.created),
        1,
        "exactly one contender must create the identity"
    );
    assert_eq!(observed.first.user_id, observed.second.user_id);
    assert_eq!(observed.first.username, observed.second.username);
    assert_eq!(observed.first.identity_id, observed.second.identity_id);
    assert_eq!(
        observed.first.identity_user_id,
        observed.second.identity_user_id
    );
    assert_eq!(observed.first.user_id, observed.first.identity_user_id);

    assert_eq!(observed.identities.len(), 1);
    assert_eq!(observed.identities[0].id, observed.first.identity_id);
    assert_eq!(observed.identities[0].user_id, observed.first.user_id);
    assert_eq!(observed.users.len(), 1);
    assert_eq!(observed.users[0].id, observed.first.user_id);
    assert_eq!(observed.users[0].username, observed.first.username);
    assert_eq!(observed.member_membership_count, 1);
    assert_eq!(observed.audits.len(), 1);
    assert_eq!(
        observed.audits[0].actor_user_id,
        Some(observed.first.user_id)
    );
    assert_eq!(
        observed.audits[0].resource_id.as_deref(),
        Some(observed.first.user_id.to_string().as_str())
    );
}

#[tokio::test]
async fn usernames_use_one_case_insensitive_trimmed_identity() {
    dotenvy::dotenv().ok();
    let database_url = integration_test_database_url();
    let mut db = AsyncPgConnection::establish(&database_url)
        .await
        .expect("the dedicated username repository test database should be reachable and migrated");
    assert_safe_test_database(&mut db).await;

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let legacy_username = format!("Legacy-{suffix}");
    let new_username = format!("New.User-{suffix}");
    let canonical_new_username = new_username.to_lowercase();
    let exercise_result = exercise_username_repository(
        &mut db,
        &legacy_username,
        &new_username,
        &canonical_new_username,
    )
    .await;

    assert_safe_test_database(&mut db).await;
    let cleanup_result =
        sql_query("DELETE FROM users WHERE lower(username) = $1 OR lower(username) = $2")
            .bind::<Text, _>(legacy_username.to_lowercase())
            .bind::<Text, _>(&canonical_new_username)
            .execute(&mut db)
            .await;
    let cleanup_count = sql_query(
        "SELECT COUNT(*)::bigint AS count FROM users \
         WHERE lower(username) = $1 OR lower(username) = $2",
    )
    .bind::<Text, _>(legacy_username.to_lowercase())
    .bind::<Text, _>(&canonical_new_username)
    .get_result::<CountRow>(&mut db)
    .await;

    cleanup_result.expect("username fixtures should be removed");
    assert_eq!(
        cleanup_count
            .expect("username fixture cleanup should be verifiable")
            .count,
        0,
        "username repository test left dynamic users behind"
    );

    let observed = exercise_result.expect("canonical username behavior should be queryable");
    assert_eq!(observed.legacy_username, legacy_username);
    assert_eq!(observed.canonical_created_username, canonical_new_username);
    assert!(observed.case_duplicate_rejected_by_database);
    assert!(observed.case_duplicate_rejected_by_repository);
    assert!(observed.surrounding_whitespace_rejected_by_database);
}

#[tokio::test]
async fn concurrent_session_creation_stays_within_the_per_user_capacity() {
    dotenvy::dotenv().ok();
    let database_url = integration_test_database_url();
    let mut observer = AsyncPgConnection::establish(&database_url)
        .await
        .expect("the dedicated session repository test database should be reachable and migrated");
    assert_safe_test_database(&mut observer).await;

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let username = format!("session-cap-{suffix}");
    let control_username = format!("session-control-{suffix}");
    let user_id = insert_session_test_user(&mut observer, &username)
        .await
        .expect("session capacity user should insert");
    let control_user_id = insert_session_test_user(&mut observer, &control_username)
        .await
        .expect("session control user should insert");
    let control_token = format!("session-control-token-{suffix}");
    let now = Utc::now();

    let invalid_capacity_rejected = SessionRepository::create(
        &mut observer,
        CreateSession {
            user_id,
            token: format!("invalid-capacity-{suffix}"),
            expires_at: now + Duration::hours(1),
            max_active_sessions: 0,
            auth_method: "password".to_owned(),
            ip_address: None,
            user_agent: None,
        },
    )
    .await
    .is_err();

    SessionRepository::create(
        &mut observer,
        CreateSession {
            user_id: control_user_id,
            token: control_token.clone(),
            expires_at: now + Duration::hours(1),
            max_active_sessions: 3,
            auth_method: "password".to_owned(),
            ip_address: None,
            user_agent: None,
        },
    )
    .await
    .expect("control user's session should insert");
    SessionRepository::create(
        &mut observer,
        CreateSession {
            user_id,
            token: format!("expired-session-{suffix}"),
            expires_at: now - Duration::seconds(1),
            max_active_sessions: 3,
            auth_method: "password".to_owned(),
            ip_address: None,
            user_agent: None,
        },
    )
    .await
    .expect("expired session fixture should insert");

    let attempts = 12;
    let barrier = Arc::new(Barrier::new(attempts));
    let mut tasks = Vec::with_capacity(attempts);
    for index in 0..attempts {
        let database_url = database_url.clone();
        let barrier = Arc::clone(&barrier);
        tasks.push(tokio::spawn(async move {
            let mut db = AsyncPgConnection::establish(&database_url)
                .await
                .map_err(|error| error.to_string())?;
            barrier.wait().await;
            SessionRepository::create(
                &mut db,
                CreateSession {
                    user_id,
                    token: format!("session-{index}-{}", uuid::Uuid::new_v4()),
                    expires_at: Utc::now() + Duration::hours(1),
                    max_active_sessions: 3,
                    auth_method: if index % 2 == 0 {
                        "password".to_owned()
                    } else {
                        "entra".to_owned()
                    },
                    ip_address: None,
                    user_agent: None,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
            Ok::<(), String>(())
        }));
    }

    let mut task_results = Vec::with_capacity(attempts);
    for task in tasks {
        task_results.push(match task.await {
            Ok(result) => result,
            Err(error) => Err(error.to_string()),
        });
    }

    let active_count = count_user_sessions(&mut observer, user_id, true).await;
    let expired_count = count_user_sessions(&mut observer, user_id, false).await;
    let control_survived = SessionRepository::find_by_token(&mut observer, &control_token).await;

    assert_safe_test_database(&mut observer).await;
    let cleanup_result = delete_session_test_users(&mut observer, user_id, control_user_id).await;
    let cleanup_count = count_session_test_users(&mut observer, &username, &control_username).await;

    cleanup_result.expect("session repository fixtures should be removed");
    assert_eq!(
        cleanup_count.expect("session fixture cleanup should be verifiable"),
        0,
        "session repository test left dynamic users behind"
    );
    for result in task_results {
        result.expect("every concurrent session creation should succeed");
    }
    assert!(
        invalid_capacity_rejected,
        "direct repository callers must not bypass the configured capacity contract"
    );
    assert_eq!(
        active_count.expect("active session count should be queryable"),
        3
    );
    assert_eq!(
        expired_count.expect("expired session count should be queryable"),
        0,
        "session creation should prune the same user's expired sessions"
    );
    control_survived.expect("another user's session must not be evicted");
}

async fn exercise_oidc_repository(
    db: &mut AsyncPgConnection,
    expired_state: &str,
    first_state: &str,
    second_state: &str,
    capped_state: &str,
    suffix: &str,
) -> TestResult<OidcObservation> {
    let now = Utc::now();
    let baseline_pending = count_pending_oidc_transactions(db, now).await?;
    let browser_binding = format!("browser-{suffix}");
    let wrong_browser_binding = format!("wrong-browser-{suffix}");
    let expires_at = now + Duration::minutes(10);

    insert_oidc_fixture(
        db,
        expired_state,
        &format!("expired-browser-{suffix}"),
        now - Duration::seconds(1),
    )
    .await?;

    let max_pending = baseline_pending + 2;
    let first = OidcLoginTransactionRepository::create_bounded(
        db,
        new_oidc_transaction(first_state, &browser_binding, expires_at),
        now,
        max_pending,
    )
    .await?
    .ok_or_else(|| io::Error::other("the first OIDC transaction unexpectedly hit capacity"))?;
    OidcLoginTransactionRepository::create_bounded(
        db,
        new_oidc_transaction(second_state, &browser_binding, expires_at),
        now,
        max_pending,
    )
    .await?
    .ok_or_else(|| io::Error::other("the second OIDC transaction unexpectedly hit capacity"))?;
    let capped = OidcLoginTransactionRepository::create_bounded(
        db,
        new_oidc_transaction(capped_state, &browser_binding, expires_at),
        now,
        max_pending,
    )
    .await?;

    let expired_was_deleted = !oidc_state_exists(db, expired_state).await?;
    let wrong_browser_was_not_found = expect_not_found(
        OidcLoginTransactionRepository::consume(db, first_state, &wrong_browser_binding, now).await,
    )?;
    let first_survived_wrong_browser = oidc_state_exists(db, first_state).await?;
    let consumed =
        OidcLoginTransactionRepository::consume(db, first_state, &browser_binding, now).await?;
    let replay_was_not_found = expect_not_found(
        OidcLoginTransactionRepository::consume(db, first_state, &browser_binding, now).await,
    )?;

    Ok(OidcObservation {
        baseline_pending,
        first_state: first.state_hash,
        first_browser_binding: first.browser_binding_hash,
        expired_was_deleted,
        cap_rejected_third: capped.is_none(),
        wrong_browser_was_not_found,
        first_survived_wrong_browser,
        consumed_state: consumed.state_hash,
        replay_was_not_found,
    })
}

async fn exercise_username_repository(
    db: &mut AsyncPgConnection,
    legacy_username: &str,
    new_username: &str,
    canonical_new_username: &str,
) -> TestResult<UsernameObservation> {
    sql_query(
        "INSERT INTO users (username, password) VALUES ($1, 'unused-legacy-password') \
         RETURNING id, username::text AS username",
    )
    .bind::<Text, _>(legacy_username)
    .get_result::<UserRow>(db)
    .await?;

    let legacy_lookup =
        UserRepository::find_by_username(db, &format!("  {}  ", legacy_username.to_uppercase()))
            .await?;
    let canonical_created = UserRepository::create(
        db,
        NewUser {
            username: format!("  {new_username}  "),
            password: "unused-new-password".to_owned(),
        },
        Vec::new(),
    )
    .await?;

    let database_duplicate = sql_query(
        "INSERT INTO users (username, password) VALUES ($1, 'unused-duplicate-password') \
         RETURNING id, username::text AS username",
    )
    .bind::<Text, _>(legacy_username.to_lowercase())
    .get_result::<UserRow>(db)
    .await;
    let repository_duplicate = UserRepository::create(
        db,
        NewUser {
            username: legacy_username.to_uppercase(),
            password: "unused-duplicate-password".to_owned(),
        },
        Vec::new(),
    )
    .await;
    let whitespace_insert = sql_query(
        "INSERT INTO users (username, password) VALUES ($1, 'unused-whitespace-password') \
         RETURNING id, username::text AS username",
    )
    .bind::<Text, _>(format!(
        "\u{00a0}whitespace-{canonical_new_username}\u{00a0}"
    ))
    .get_result::<UserRow>(db)
    .await;

    Ok(UsernameObservation {
        legacy_username: legacy_lookup.username,
        canonical_created_username: canonical_created.username,
        case_duplicate_rejected_by_database: is_unique_violation(database_duplicate),
        case_duplicate_rejected_by_repository: is_unique_violation(repository_duplicate),
        surrounding_whitespace_rejected_by_database: is_check_violation(whitespace_insert),
    })
}

fn is_unique_violation<T>(result: diesel::QueryResult<T>) -> bool {
    matches!(
        result,
        Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            _
        ))
    )
}

fn is_check_violation<T>(result: diesel::QueryResult<T>) -> bool {
    matches!(
        result,
        Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::CheckViolation,
            _
        ))
    )
}

fn expect_not_found<T>(result: diesel::QueryResult<T>) -> TestResult<bool> {
    match result {
        Err(diesel::result::Error::NotFound) => Ok(true),
        Err(error) => Err(error.into()),
        Ok(_) => Ok(false),
    }
}

fn new_oidc_transaction(
    state_hash: &str,
    browser_binding_hash: &str,
    expires_at: DateTime<Utc>,
) -> NewOidcLoginTransaction {
    NewOidcLoginTransaction {
        state_hash: state_hash.to_owned(),
        browser_binding_hash: browser_binding_hash.to_owned(),
        nonce: format!("nonce-{state_hash}"),
        pkce_verifier_ciphertext: format!("ciphertext-{state_hash}"),
        return_to: "/#dashboard".to_owned(),
        expires_at,
    }
}

fn new_test_user(username: &str) -> NewUser {
    NewUser {
        username: username.to_owned(),
        password: "unused-jit-test-password".to_owned(),
    }
}

fn new_test_identity(
    tenant_id: &str,
    object_id: &str,
    issuer: &str,
    subject: &str,
) -> NewExternalIdentity {
    NewExternalIdentity {
        user_id: 0,
        provider: "entra".to_owned(),
        issuer: issuer.to_owned(),
        subject: Some(subject.to_owned()),
        tenant_id: tenant_id.to_owned(),
        object_id: object_id.to_owned(),
        preferred_username: Some(format!("{object_id}@example.test")),
        display_name: Some("Concurrent JIT Test".to_owned()),
        email: Some(format!("{object_id}@example.test")),
        last_login_at: None,
    }
}

async fn observe_jit_results(
    db: &mut AsyncPgConnection,
    first: diesel::QueryResult<crate::repositories::JitProvisionedIdentity>,
    second: diesel::QueryResult<crate::repositories::JitProvisionedIdentity>,
    tenant_id: &str,
    object_id: &str,
    username_a: &str,
    username_b: &str,
) -> TestResult<JitObservation> {
    let first = first?;
    let second = second?;
    let first = ProvisionedResult {
        user_id: first.user.id,
        username: first.user.username,
        identity_id: first.identity.id,
        identity_user_id: first.identity.user_id,
        created: first.created,
    };
    let second = ProvisionedResult {
        user_id: second.user.id,
        username: second.user.username,
        identity_id: second.identity.id,
        identity_user_id: second.identity.user_id,
        created: second.created,
    };

    let identities = sql_query(
        "SELECT id, user_id FROM external_identities \
         WHERE provider = 'entra' AND tenant_id = $1 AND object_id = $2",
    )
    .bind::<Text, _>(tenant_id)
    .bind::<Text, _>(object_id)
    .load::<IdentityRow>(db)
    .await?;
    let users = sql_query(
        "SELECT id, username::text AS username FROM users \
         WHERE username = $1 OR username = $2",
    )
    .bind::<Text, _>(username_a)
    .bind::<Text, _>(username_b)
    .load::<UserRow>(db)
    .await?;
    let member_membership_count = sql_query(
        "SELECT COUNT(*)::bigint AS count FROM users_roles \
         INNER JOIN roles ON roles.id = users_roles.role_id \
         WHERE users_roles.user_id = $1 AND roles.code = 'member'",
    )
    .bind::<Integer, _>(first.user_id)
    .get_result::<CountRow>(db)
    .await?
    .count;
    let audits = sql_query(
        "SELECT actor_user_id, resource_id::text AS resource_id FROM audit_logs \
         WHERE action = 'auth.entra_jit_provisioned' \
           AND metadata::jsonb ->> 'tenant_id' = $1 \
           AND metadata::jsonb ->> 'object_id' = $2",
    )
    .bind::<Text, _>(tenant_id)
    .bind::<Text, _>(object_id)
    .load::<AuditRow>(db)
    .await?;

    Ok(JitObservation {
        first,
        second,
        identities,
        users,
        member_membership_count,
        audits,
    })
}

async fn acquire_oidc_capacity_lock(db: &mut AsyncPgConnection) -> diesel::QueryResult<()> {
    sql_query(
        "SELECT pg_advisory_lock(\
             hashtextextended('portal:oidc-login-transaction-capacity', 0)\
         )::text AS lock_result",
    )
    .get_result::<AdvisoryLockRow>(db)
    .await
    .map(|_| ())
}

async fn release_oidc_capacity_lock(db: &mut AsyncPgConnection) -> diesel::QueryResult<bool> {
    sql_query(
        "SELECT pg_advisory_unlock(\
             hashtextextended('portal:oidc-login-transaction-capacity', 0)\
         ) AS unlocked",
    )
    .get_result::<AdvisoryUnlockRow>(db)
    .await
    .map(|row| row.unlocked)
}

async fn count_pending_oidc_transactions(
    db: &mut AsyncPgConnection,
    now: DateTime<Utc>,
) -> diesel::QueryResult<i64> {
    sql_query("SELECT COUNT(*)::bigint AS count FROM oidc_login_transactions WHERE expires_at > $1")
        .bind::<Timestamptz, _>(now)
        .get_result::<CountRow>(db)
        .await
        .map(|row| row.count)
}

async fn insert_oidc_fixture(
    db: &mut AsyncPgConnection,
    state_hash: &str,
    browser_binding_hash: &str,
    expires_at: DateTime<Utc>,
) -> diesel::QueryResult<usize> {
    sql_query(
        "INSERT INTO oidc_login_transactions \
             (state_hash, browser_binding_hash, nonce, pkce_verifier_ciphertext, return_to, expires_at) \
         VALUES ($1, $2, 'repository-test-nonce', 'repository-test-ciphertext', \
                 '/#dashboard', $3)",
    )
    .bind::<Text, _>(state_hash)
    .bind::<Text, _>(browser_binding_hash)
    .bind::<Timestamptz, _>(expires_at)
    .execute(db)
    .await
}

async fn oidc_state_exists(
    db: &mut AsyncPgConnection,
    state_hash: &str,
) -> diesel::QueryResult<bool> {
    sql_query(
        "SELECT EXISTS(\
             SELECT 1 FROM oidc_login_transactions WHERE state_hash = $1\
         ) AS exists",
    )
    .bind::<Text, _>(state_hash)
    .get_result::<ExistsRow>(db)
    .await
    .map(|row| row.exists)
}

async fn delete_oidc_fixtures(
    db: &mut AsyncPgConnection,
    states: &[&str; 4],
) -> diesel::QueryResult<usize> {
    sql_query(
        "DELETE FROM oidc_login_transactions \
         WHERE state_hash = $1 OR state_hash = $2 OR state_hash = $3 OR state_hash = $4",
    )
    .bind::<Text, _>(states[0])
    .bind::<Text, _>(states[1])
    .bind::<Text, _>(states[2])
    .bind::<Text, _>(states[3])
    .execute(db)
    .await
}

async fn count_oidc_fixtures(
    db: &mut AsyncPgConnection,
    states: &[&str; 4],
) -> diesel::QueryResult<i64> {
    sql_query(
        "SELECT COUNT(*)::bigint AS count FROM oidc_login_transactions \
         WHERE state_hash = $1 OR state_hash = $2 OR state_hash = $3 OR state_hash = $4",
    )
    .bind::<Text, _>(states[0])
    .bind::<Text, _>(states[1])
    .bind::<Text, _>(states[2])
    .bind::<Text, _>(states[3])
    .get_result::<CountRow>(db)
    .await
    .map(|row| row.count)
}

async fn role_exists(db: &mut AsyncPgConnection, code: &str) -> diesel::QueryResult<bool> {
    sql_query("SELECT EXISTS(SELECT 1 FROM roles WHERE code = $1) AS exists")
        .bind::<Text, _>(code)
        .get_result::<ExistsRow>(db)
        .await
        .map(|row| row.exists)
}

async fn cleanup_jit_fixture(
    db: &mut AsyncPgConnection,
    tenant_id: &str,
    object_id: &str,
    username_a: &str,
    username_b: &str,
    member_role_preexisted: bool,
) -> diesel::QueryResult<()> {
    db.transaction::<_, diesel::result::Error, _>(|conn| {
        Box::pin(async move {
            sql_query(
                "DELETE FROM audit_logs \
                 WHERE action = 'auth.entra_jit_provisioned' \
                   AND metadata::jsonb ->> 'tenant_id' = $1 \
                   AND metadata::jsonb ->> 'object_id' = $2",
            )
            .bind::<Text, _>(tenant_id)
            .bind::<Text, _>(object_id)
            .execute(conn)
            .await?;
            sql_query(
                "DELETE FROM external_identities \
                 WHERE provider = 'entra' AND tenant_id = $1 AND object_id = $2",
            )
            .bind::<Text, _>(tenant_id)
            .bind::<Text, _>(object_id)
            .execute(conn)
            .await?;
            sql_query(
                "DELETE FROM users_roles WHERE user_id IN (\
                     SELECT id FROM users WHERE username = $1 OR username = $2\
                 )",
            )
            .bind::<Text, _>(username_a)
            .bind::<Text, _>(username_b)
            .execute(conn)
            .await?;
            sql_query("DELETE FROM users WHERE username = $1 OR username = $2")
                .bind::<Text, _>(username_a)
                .bind::<Text, _>(username_b)
                .execute(conn)
                .await?;

            if !member_role_preexisted {
                sql_query(
                    "DELETE FROM roles \
                     WHERE code = 'member' \
                       AND NOT EXISTS (\
                           SELECT 1 FROM users_roles WHERE users_roles.role_id = roles.id\
                       )",
                )
                .execute(conn)
                .await?;
            }
            Ok(())
        })
    })
    .await
}

async fn jit_fixture_counts(
    db: &mut AsyncPgConnection,
    tenant_id: &str,
    object_id: &str,
    username_a: &str,
    username_b: &str,
) -> diesel::QueryResult<(i64, i64, i64)> {
    let identity_count = sql_query(
        "SELECT COUNT(*)::bigint AS count FROM external_identities \
         WHERE provider = 'entra' AND tenant_id = $1 AND object_id = $2",
    )
    .bind::<Text, _>(tenant_id)
    .bind::<Text, _>(object_id)
    .get_result::<CountRow>(db)
    .await?
    .count;
    let user_count = sql_query(
        "SELECT COUNT(*)::bigint AS count FROM users WHERE username = $1 OR username = $2",
    )
    .bind::<Text, _>(username_a)
    .bind::<Text, _>(username_b)
    .get_result::<CountRow>(db)
    .await?
    .count;
    let audit_count = sql_query(
        "SELECT COUNT(*)::bigint AS count FROM audit_logs \
         WHERE action = 'auth.entra_jit_provisioned' \
           AND metadata::jsonb ->> 'tenant_id' = $1 \
           AND metadata::jsonb ->> 'object_id' = $2",
    )
    .bind::<Text, _>(tenant_id)
    .bind::<Text, _>(object_id)
    .get_result::<CountRow>(db)
    .await?
    .count;
    Ok((identity_count, user_count, audit_count))
}

async fn insert_session_test_user(
    db: &mut AsyncPgConnection,
    username: &str,
) -> diesel::QueryResult<i32> {
    sql_query(
        "INSERT INTO users (username, password) VALUES ($1, 'unused-session-test-password') \
         RETURNING id, username::text AS username",
    )
    .bind::<Text, _>(username)
    .get_result::<UserRow>(db)
    .await
    .map(|row| row.id)
}

async fn count_user_sessions(
    db: &mut AsyncPgConnection,
    user_id: i32,
    active: bool,
) -> diesel::QueryResult<i64> {
    let query = if active {
        "SELECT COUNT(*)::bigint AS count FROM sessions \
         WHERE user_id = $1 AND expires_at > NOW()"
    } else {
        "SELECT COUNT(*)::bigint AS count FROM sessions \
         WHERE user_id = $1 AND expires_at <= NOW()"
    };
    sql_query(query)
        .bind::<Integer, _>(user_id)
        .get_result::<CountRow>(db)
        .await
        .map(|row| row.count)
}

async fn delete_session_test_users(
    db: &mut AsyncPgConnection,
    user_id: i32,
    control_user_id: i32,
) -> diesel::QueryResult<usize> {
    sql_query("DELETE FROM users WHERE id = $1 OR id = $2")
        .bind::<Integer, _>(user_id)
        .bind::<Integer, _>(control_user_id)
        .execute(db)
        .await
}

async fn count_session_test_users(
    db: &mut AsyncPgConnection,
    username: &str,
    control_username: &str,
) -> diesel::QueryResult<i64> {
    sql_query("SELECT COUNT(*)::bigint AS count FROM users WHERE username = $1 OR username = $2")
        .bind::<Text, _>(username)
        .bind::<Text, _>(control_username)
        .get_result::<CountRow>(db)
        .await
        .map(|row| row.count)
}

async fn assert_safe_test_database(db: &mut AsyncPgConnection) {
    let app_env = std::env::var("APP_ENV").unwrap_or_default();
    assert_eq!(
        app_env, "test",
        "refusing destructive repository test: APP_ENV must be exactly 'test'"
    );

    let expected_target = validate_test_database_url(
        "test",
        &integration_test_database_url(),
        "PORTAL_TEST_DATABASE_URL",
    )
    .expect("PORTAL_TEST_DATABASE_URL must be a valid test database URL")
    .expect("test environment must produce a database safety target");
    let database_name = sql_query("SELECT current_database()::text AS database_name")
        .get_result::<CurrentDatabase>(db)
        .await
        .expect("current database name should be readable")
        .database_name;
    assert_eq!(
        database_name,
        expected_target.database_name(),
        "refusing destructive repository test: actual database does not match PORTAL_TEST_DATABASE_URL"
    );
    let has_test_segment = database_name
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|segment| segment == "test");
    assert!(
        has_test_segment,
        "refusing destructive repository test: database '{database_name}' must contain a standalone 'test' segment"
    );
}

fn integration_test_database_url() -> String {
    std::env::var("PORTAL_TEST_DATABASE_URL")
        .expect("PORTAL_TEST_DATABASE_URL must point to the integration test database")
}
