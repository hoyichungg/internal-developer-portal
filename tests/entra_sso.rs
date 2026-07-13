use std::{
    process::Command,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use chrono::Utc;
use diesel::{sql_query, QueryableByName};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use internal_developer_portal::{
    config::{AppConfig, EntraConfig},
    rocket_routes::DbConn,
};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Url;
use rocket::{figment::Figment, http::Status, local::asynchronous::Client};
use rocket_db_pools::Database;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;
use tokio::time::timeout;
use uuid::Uuid;

pub mod common;

const TENANT_ID: &str = "11111111-1111-4111-8111-111111111111";
const CLIENT_ID: &str = "22222222-2222-4222-8222-222222222222";
const TRANSACTION_KEY: &str = "S7yN2vQ9kL4mX8pR1tW6cF3hJ5dB0zAa";
const CLIENT_SECRET: &str = "mock-provider-client-secret";
const TEST_RSA_PRIVATE_DER: &str = "MIIEpAIBAAKCAQEAyRE6rHuNR0QbHO3H3Kt2pOKGVhQqGZXInOduQNxXzuKlvQTLUTv4l4sggh5/CYYi/cvI+SXVT9kPWSKXxJXBXd/4LkvcPuUakBoAkfh+eiFVMh2VrUyWyj3MFl0HTVF9KwRXLAcwkREiS3npThHRyIxuy0ZMeZfxVL5arMhw1SRELB8HoGfG/AtH89BIE9jDBHZ9dLelK9a184zAf8LwoPLxvJb3Il5nncqPcSfKDDodMFBIMc4lQzDKL5gvmiXLXB1AGLm8KBjfE8s3L5xqi+yUod+j8MtvIj812dkS4QMiRVN/by2h3ZY8LYVGrqZXZTcgn2ujn8uKjXLZVD5TdQIDAQABAoIBAHREk0I0O9DvECKdWUpAmF3mY7oY9PNQiu44Yaf+AoSuyRpRUGTMIgc3u3eivOE8ALX0BmYUO5JtuRNZDpvt4SAwqCnVUinIf6C+eH/wSurCpapSM0BAHp4aOA7igptyOMgMPYBHNA1e9A7jE0dCxKWMl3DSWNyjQTk4zeRGEAEfbNjHrq6YCtjHSZSLmWiG80hnfnYos9hOr5JnLnyS7ZmFE/5P3XVrxLc/tQ5zum0R4cbrgzHiQP5RgfxGJaEi7XcgherCCOgurJSSbYH29Gz8u5fFbS+Yg8s+OiCss3cs1rSgJ9/eHZuzGEdUZVARH6hVMjSuwvqVTFaE8AgtleECgYEA+uLMn4kNqHlJS2A5uAnCkj90ZxEtNm3E8hAxUrhssktY5XSOAPBlxyf5RuRGIImGtUVIr4HuJSa5TX48n3Vdt9MYCprO/iYl6moNRSPt5qowIIOJmIjY2mqPDfDt/zw+fcDD3lmCJrFlzcnh0uea1CohxEbQnL3cypeLt+WbU6kCgYEAzSp19m1ajieFkqgoB0YTpt/OroDx38vvI5unInJlEeOjQ+oIAQdN2wpxBvTrRorMU6P07mFUbt1j+Co6CbNiw+X8HcCaqYLR5clbJOOWNR36PuzOpQLkfK8woupBxzW9B8gZmY8rB1mbJ+/WTPrEJy6YGmIEBkWylQ2VpW8O4O0CgYEApdbvvfFBlwD9YxbrcGz7MeNCFbMz+MucqQntIKoKJ91ImPxvtc0y6e/Rhnv0oyNlaUOwJVu0yNgNG117w0g4t/+Q38mvVC5xV7/cn7x9UMFk6MkqVir3dYGEqIl/OP1grY2Tq9HtB5iyG9L8NIamQOLMyUqqMUILxdthHyFmiGkCgYEAn9+PjpjGMPHxL0gj8Q8VbzsFtou6b1deIRRA2CHmSltltR1gYVTMwXxQeUhPMmgkMqUXzs4/WijgpthY44hK1TaZEKIuoxrS70nJ4WQLf5a9k1065fDsFZD6yGjdGxvwEmlGMZgTwqV7t1I4X0Ilqhav5hcs5apYL7gnPYPeRz0CgYALHCj/Ji8XSsDoF/MhVhnGdIs2P99NNdmo3R2Pv0CuZbDKMU559LJHUvrKS8WkuWRDuKrz1W/EQKApFjDGpdqToZqriUFQzwy7mR3ayIiogzNtHcvbDHx8oFnGY0OFksX/ye0/XGpy2SFxYRwGU98HPYeBvAQQrVjdkzfy7BmXQQ==";
const TEST_RSA_MODULUS: &str = "yRE6rHuNR0QbHO3H3Kt2pOKGVhQqGZXInOduQNxXzuKlvQTLUTv4l4sggh5_CYYi_cvI-SXVT9kPWSKXxJXBXd_4LkvcPuUakBoAkfh-eiFVMh2VrUyWyj3MFl0HTVF9KwRXLAcwkREiS3npThHRyIxuy0ZMeZfxVL5arMhw1SRELB8HoGfG_AtH89BIE9jDBHZ9dLelK9a184zAf8LwoPLxvJb3Il5nncqPcSfKDDodMFBIMc4lQzDKL5gvmiXLXB1AGLm8KBjfE8s3L5xqi-yUod-j8MtvIj812dkS4QMiRVN_by2h3ZY8LYVGrqZXZTcgn2ujn8uKjXLZVD5TdQ";
static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(QueryableByName)]
struct UserIdRow {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    id: i32,
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    count: i64,
}

struct TestUserCleanup {
    user_id: i32,
    database_url: String,
    armed: bool,
}

impl TestUserCleanup {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TestUserCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = delete_test_user(&self.database_url, self.user_id);
        }
    }
}

#[rocket::async_test]
async fn entra_sso_is_browser_bound_single_use_and_creates_an_entra_session() {
    common::assert_safe_test_database_async().await;
    let database_url = common::database_url().to_owned();
    let object_id = Uuid::new_v4().to_string();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider_address = listener.local_addr().unwrap();
    let issuer = format!("http://{provider_address}/issuer");
    let authorization_url = format!("http://{provider_address}/authorize");
    let token_url = format!("http://{provider_address}/token");
    let jwks_url = format!("http://{provider_address}/keys");
    let redirect_uri = format!("{}/auth/entra/callback", common::app_host());

    let username = unique_name("entra_prelinked");
    create_test_user(
        &database_url,
        &username,
        "unrelated-local-password",
        "member",
    );
    let mut db = AsyncPgConnection::establish(&database_url).await.unwrap();
    let user_id = sql_query("SELECT id FROM users WHERE username = $1")
        .bind::<diesel::sql_types::Text, _>(&username)
        .get_result::<UserIdRow>(&mut db)
        .await
        .unwrap()
        .id;
    let mut cleanup = TestUserCleanup {
        user_id,
        database_url: database_url.clone(),
        armed: true,
    };

    let link = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args([
            "users",
            "link-entra",
            "--username",
            &username,
            "--object-id",
            &object_id,
        ])
        .env("DATABASE_URL", &database_url)
        .env("AUTH_ENTRA_ENABLED", "true")
        .env("AUTH_ENTRA_TENANT_ID", TENANT_ID)
        .env("AUTH_ENTRA_CLIENT_ID", CLIENT_ID)
        .env("AUTH_ENTRA_CLIENT_SECRET", CLIENT_SECRET)
        .env("AUTH_ENTRA_REDIRECT_URI", &redirect_uri)
        .env("AUTH_OIDC_TRANSACTION_KEY", TRANSACTION_KEY)
        .env("AUTH_ENTRA_ISSUER", &issuer)
        .env("AUTH_ENTRA_AUTHORIZATION_URL", &authorization_url)
        .env("AUTH_ENTRA_TOKEN_URL", &token_url)
        .env("AUTH_ENTRA_JWKS_URL", &jwks_url)
        .output()
        .unwrap();
    assert!(
        link.status.success(),
        "{}",
        String::from_utf8_lossy(&link.stderr)
    );

    let config = AppConfig {
        environment: "test".to_owned(),
        auth_token_ttl_seconds: 3_600,
        auth_max_active_sessions_per_user: 10,
        auth_cookie_secure: false,
        auth_login_max_failures: 5,
        auth_login_account_max_failures: 50,
        auth_login_window_seconds: 900,
        auth_login_lockout_seconds: 900,
        auth_password_login_enabled: true,
        entra: Some(EntraConfig {
            tenant_id: TENANT_ID.to_owned(),
            client_id: CLIENT_ID.to_owned(),
            client_secret: Some(CLIENT_SECRET.to_owned()),
            transaction_key: TRANSACTION_KEY.to_owned(),
            redirect_uri,
            issuer: issuer.clone(),
            authorization_url,
            token_url,
            jwks_url,
            jit_provisioning: false,
            required_role: Some("Portal.Member".to_owned()),
            transaction_ttl_seconds: 600,
            jwks_cache_seconds: 300,
            clock_skew_seconds: 120,
        }),
    };
    let rocket = internal_developer_portal::server_app::build(config);
    let figment: Figment = rocket
        .figment()
        .clone()
        .merge(("databases.postgres.url", database_url.clone()))
        .merge(("databases.postgres.max_connections", 1))
        .merge(("databases.postgres.connect_timeout", 5));
    let client = Arc::new(Client::tracked(rocket.configure(figment)).await.unwrap());
    let start = client
        .get("/auth/entra/start?return_to=%2F%23catalog")
        .dispatch()
        .await;
    assert_eq!(start.status(), Status::SeeOther);
    assert_eq!(start.headers().get_one("Cache-Control"), Some("no-store"));
    assert_eq!(
        start.headers().get_one("Referrer-Policy"),
        Some("no-referrer")
    );
    let start_location = start.headers().get_one("Location").unwrap();
    let authorization = Url::parse(start_location).unwrap();
    let authorization_params = authorization
        .query_pairs()
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();
    let state = authorization_params.get("state").unwrap().clone();
    let nonce = authorization_params.get("nonce").unwrap().clone();
    let expected_challenge = authorization_params.get("code_challenge").unwrap().clone();
    assert_eq!(
        authorization_params.get("code_challenge_method").unwrap(),
        "S256"
    );
    assert_eq!(authorization_params.get("response_type").unwrap(), "code");

    let attacker = Client::untracked(internal_developer_portal::server_app::build(AppConfig {
        environment: "test".to_owned(),
        auth_token_ttl_seconds: 3_600,
        auth_max_active_sessions_per_user: 10,
        auth_cookie_secure: false,
        auth_login_max_failures: 5,
        auth_login_account_max_failures: 50,
        auth_login_window_seconds: 900,
        auth_login_lockout_seconds: 900,
        auth_password_login_enabled: false,
        entra: client.rocket().state::<AppConfig>().unwrap().entra.clone(),
    }))
    .await
    .unwrap();
    let disabled_password_login = attacker
        .post("/login")
        .json(&json!({
            "username": username,
            "password": "unrelated-local-password"
        }))
        .dispatch()
        .await;
    assert_eq!(disabled_password_login.status(), Status::Forbidden);
    assert_eq!(
        disabled_password_login.into_json::<Value>().await.unwrap()["error"]["code"],
        "forbidden"
    );
    let bogus_same_browser = client
        .get("/auth/entra/callback?code=ignored&state=bogus-state")
        .dispatch()
        .await;
    assert_eq!(bogus_same_browser.status(), Status::SeeOther);
    assert_eq!(
        bogus_same_browser.headers().get_one("Location"),
        Some("/?auth_error=entra_invalid_state#dashboard")
    );
    let wrong_browser = attacker
        .get(format!("/auth/entra/callback?code=ignored&state={state}"))
        .dispatch()
        .await;
    assert_eq!(wrong_browser.status(), Status::SeeOther);
    assert_eq!(
        wrong_browser.headers().get_one("Location"),
        Some("/?auth_error=entra_invalid_state#dashboard")
    );

    let expected_nonce = Arc::new(Mutex::new(nonce));
    let expected_pkce = Arc::new(Mutex::new(expected_challenge));
    let server_nonce = Arc::clone(&expected_nonce);
    let server_pkce = Arc::clone(&expected_pkce);
    let server_issuer = issuer.clone();
    let server_object_id = object_id.clone();
    let (token_reached_tx, token_reached_rx) = oneshot::channel();
    let (release_token_tx, release_token_rx) = oneshot::channel();
    let (jwks_reached_tx, jwks_reached_rx) = oneshot::channel();
    let (release_jwks_tx, release_jwks_rx) = oneshot::channel();
    let provider = rocket::tokio::spawn(async move {
        let mut token_reached_tx = Some(token_reached_tx);
        let mut release_token_rx = Some(release_token_rx);
        let mut jwks_reached_tx = Some(jwks_reached_tx);
        let mut release_jwks_rx = Some(release_jwks_rx);
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (path, body) = read_http_request(&mut stream).await;
            let response = match path.as_str() {
                "/token" => {
                    let form = Url::parse(&format!("http://mock.invalid/?{body}"))
                        .unwrap()
                        .query_pairs()
                        .into_owned()
                        .collect::<std::collections::HashMap<_, _>>();
                    assert_eq!(form.get("code").unwrap(), "valid-code");
                    assert_eq!(form.get("client_secret").unwrap(), CLIENT_SECRET);
                    let verifier = form.get("code_verifier").unwrap();
                    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
                    assert_eq!(challenge, *server_pkce.lock().unwrap());
                    token_reached_tx
                        .take()
                        .expect("token endpoint must only be called once")
                        .send(())
                        .expect("callback test must wait for token request");
                    release_token_rx
                        .take()
                        .expect("token release must only be awaited once")
                        .await
                        .expect("callback test must release token response");
                    let now = Utc::now().timestamp();
                    let claims = json!({
                        "iss": server_issuer,
                        "aud": CLIENT_ID,
                        "exp": now + 300,
                        "nbf": now - 10,
                        "iat": now - 10,
                        "sub": "subject-for-this-client",
                        "nonce": *server_nonce.lock().unwrap(),
                        "tid": TENANT_ID,
                        "oid": server_object_id,
                        "ver": "2.0",
                        "roles": ["Portal.Member"],
                        "preferred_username": "alice.renamed@example.test",
                        "name": "Alice Renamed",
                        "email": "alice.renamed@example.test"
                    });
                    json!({ "token_type": "Bearer", "id_token": sign_token(&claims) }).to_string()
                }
                "/keys" => {
                    jwks_reached_tx
                        .take()
                        .expect("JWKS endpoint must only be called once")
                        .send(())
                        .expect("callback test must wait for JWKS request");
                    release_jwks_rx
                        .take()
                        .expect("JWKS release must only be awaited once")
                        .await
                        .expect("callback test must release JWKS response");
                    json!({
                        "keys": [{
                            "kty": "RSA",
                            "use": "sig",
                            "alg": "RS256",
                            "kid": "test-key",
                            "n": TEST_RSA_MODULUS,
                            "e": "AQAB"
                        }]
                    })
                    .to_string()
                }
                _ => panic!("unexpected provider path: {path}"),
            };
            write_http_json(&mut stream, &response).await;
        }
    });

    let callback_client = Arc::clone(&client);
    let callback_state = state.clone();
    let callback = tokio::spawn(async move {
        let response = callback_client
            .get(format!(
                "/auth/entra/callback?code=valid-code&state={callback_state}"
            ))
            .dispatch()
            .await;
        let status = response.status();
        let location = response.headers().get_one("Location").map(str::to_owned);
        let set_cookies = response
            .headers()
            .get("Set-Cookie")
            .map(str::to_owned)
            .collect::<Vec<_>>();
        (status, location, set_cookies)
    });

    timeout(Duration::from_secs(2), token_reached_rx)
        .await
        .expect("callback must reach mock token endpoint")
        .expect("mock token endpoint signal");
    assert_ready_while_provider_is_slow(&client, "token exchange").await;
    release_token_tx
        .send(())
        .expect("mock token response must still be waiting");

    timeout(Duration::from_secs(2), jwks_reached_rx)
        .await
        .expect("callback must reach mock JWKS endpoint")
        .expect("mock JWKS endpoint signal");
    assert_ready_while_provider_is_slow(&client, "JWKS fetch").await;
    release_jwks_tx
        .send(())
        .expect("mock JWKS response must still be waiting");

    let (callback_status, callback_location, callback_cookies) =
        timeout(Duration::from_secs(5), callback)
            .await
            .expect("callback must finish after provider responses")
            .expect("callback task must not panic");
    assert_eq!(callback_status, Status::SeeOther);
    assert_eq!(
        callback_location.as_deref(),
        Some("/?auth_result=entra#catalog")
    );
    assert!(callback_cookies
        .iter()
        .any(|value| value.starts_with("idp_session=") && value.contains("HttpOnly")));
    provider.await.unwrap();

    let me = client.get("/me").dispatch().await;
    assert_eq!(me.status(), Status::Ok);
    let me = me.into_json::<Value>().await.unwrap();
    assert_eq!(me["data"]["id"], user_id);
    assert_eq!(me["data"]["username"], username);
    assert_eq!(me["data"]["auth_method"], "entra");
    assert!(me["data"]["roles"]
        .as_array()
        .unwrap()
        .contains(&json!("member")));

    let replay = client
        .get(format!(
            "/auth/entra/callback?code=replayed-code&state={state}"
        ))
        .dispatch()
        .await;
    assert_eq!(replay.status(), Status::SeeOther);
    assert_eq!(
        replay.headers().get_one("Location"),
        Some("/?auth_error=entra_invalid_state#dashboard")
    );

    let denied_start = client
        .get("/auth/entra/start?return_to=%2F%23audit")
        .dispatch()
        .await;
    let denied_authorization =
        Url::parse(denied_start.headers().get_one("Location").unwrap()).unwrap();
    let denied_state = denied_authorization
        .query_pairs()
        .find_map(|(name, value)| (name == "state").then(|| value.into_owned()))
        .unwrap();
    let denied = client
        .get(format!(
            "/auth/entra/callback?error=access_denied&state={denied_state}"
        ))
        .dispatch()
        .await;
    assert_eq!(denied.status(), Status::SeeOther);
    assert_eq!(
        denied.headers().get_one("Location"),
        Some("/?auth_error=entra_access_denied#audit")
    );

    let unavailable_start = client
        .get("/auth/entra/start?return_to=%2F%23services")
        .dispatch()
        .await;
    let unavailable_authorization =
        Url::parse(unavailable_start.headers().get_one("Location").unwrap()).unwrap();
    let unavailable_state = unavailable_authorization
        .query_pairs()
        .find_map(|(name, value)| (name == "state").then(|| value.into_owned()))
        .unwrap();
    let pool = DbConn::fetch(client.rocket()).expect("database pool must be initialized");
    let held_connection = pool
        .get()
        .await
        .expect("the sole OIDC test pool connection must be available");
    let unavailable = timeout(
        Duration::from_secs(7),
        client
            .get(format!(
                "/auth/entra/callback?code=ignored&state={unavailable_state}"
            ))
            .dispatch(),
    )
    .await
    .expect("callback DB checkout must obey the configured five-second pool timeout");
    assert_eq!(unavailable.status(), Status::SeeOther);
    assert_eq!(
        unavailable.headers().get_one("Location"),
        Some("/?auth_error=entra_provider_unavailable#dashboard")
    );
    assert_eq!(
        unavailable.headers().get_one("Cache-Control"),
        Some("no-store")
    );
    assert_eq!(
        unavailable.headers().get_one("Referrer-Policy"),
        Some("no-referrer")
    );
    drop(held_connection);

    let unavailable_cleanup = client
        .get(format!(
            "/auth/entra/callback?error=access_denied&state={unavailable_state}"
        ))
        .dispatch()
        .await;
    assert_eq!(
        unavailable_cleanup.headers().get_one("Location"),
        Some("/?auth_error=entra_access_denied#services")
    );

    let audit_resource_id = format!("entra:{TENANT_ID}:{object_id}");
    let deleted_audits = sql_query(
        "DELETE FROM audit_logs WHERE resource_type = 'external_identity' AND resource_id = $1",
    )
    .bind::<diesel::sql_types::Text, _>(&audit_resource_id)
    .execute(&mut db)
    .await
    .unwrap();
    assert_eq!(deleted_audits, 1);
    let deleted_user = delete_test_user(&database_url, user_id);
    assert!(
        deleted_user.status.success(),
        "{}",
        String::from_utf8_lossy(&deleted_user.stderr)
    );
    cleanup.disarm();
    let remaining = sql_query(
        "SELECT COUNT(*)::bigint AS count FROM external_identities \
         WHERE tenant_id = $1 AND object_id = $2",
    )
    .bind::<diesel::sql_types::Text, _>(TENANT_ID)
    .bind::<diesel::sql_types::Text, _>(&object_id)
    .get_result::<CountRow>(&mut db)
    .await
    .unwrap();
    assert_eq!(remaining.count, 0);
}

async fn assert_ready_while_provider_is_slow(client: &Client, provider_phase: &str) {
    let readiness = timeout(Duration::from_millis(750), client.get("/readyz").dispatch())
        .await
        .unwrap_or_else(|_| panic!("{provider_phase} must not retain the sole DB pool connection"));
    assert_eq!(readiness.status(), Status::Ok, "{provider_phase}");
}

fn unique_name(prefix: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{}_{}_{}", std::process::id(), timestamp, counter)
}

fn create_test_user(database_url: &str, username: &str, password: &str, roles: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(["users", "create", username, password, roles])
        .env("DATABASE_URL", database_url)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn delete_test_user(database_url: &str, user_id: i32) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(["users", "delete", &user_id.to_string()])
        .env("DATABASE_URL", database_url)
        .output()
        .expect("test user cleanup command")
}

fn sign_token(claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-key".to_owned());
    let private_key = STANDARD.decode(TEST_RSA_PRIVATE_DER).unwrap();
    encode(&header, claims, &EncodingKey::from_rsa_der(&private_key)).unwrap()
}

async fn read_http_request(stream: &mut tokio::net::TcpStream) -> (String, String) {
    let mut request = Vec::new();
    let header_end = loop {
        let mut buffer = [0_u8; 4_096];
        let read = stream.read(&mut buffer).await.unwrap();
        assert!(read > 0);
        request.extend_from_slice(&buffer[..read]);
        if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(str::trim)
                .map(str::to_owned)
        })
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while request.len() < header_end + content_length {
        let mut buffer = [0_u8; 4_096];
        let read = stream.read(&mut buffer).await.unwrap();
        assert!(read > 0);
        request.extend_from_slice(&buffer[..read]);
    }
    let path = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap()
        .to_owned();
    let body =
        String::from_utf8(request[header_end..header_end + content_length].to_vec()).unwrap();

    (path, body)
}

async fn write_http_json(stream: &mut tokio::net::TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await.unwrap();
    stream.shutdown().await.unwrap();
}
