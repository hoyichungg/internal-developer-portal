use crate::{
    api::{ApiError, ApiResponse},
    config::{AppConfig, EntraConfig},
    models::{NewExternalIdentity, NewOidcLoginTransaction, NewUser, User},
    repositories::{
        ExternalIdentityLoginProfile, ExternalIdentityRepository, OidcLoginTransactionRepository,
        UserRepository,
    },
    rocket_routes::{authorization, DbConn},
};
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::{
    password_hash::{rand_core::OsRng, rand_core::RngCore, SaltString},
    Argon2, PasswordHasher,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use reqwest::{redirect::Policy, Url};
use rocket::{
    fairing::{Fairing, Info, Kind},
    form::FromForm,
    http::{Cookie, CookieJar, Header, SameSite},
    response::{self, Redirect, Responder},
    serde::{json::Json, Serialize},
    time::Duration as CookieDuration,
    Request, Response, State,
};
use rocket_db_pools::Connection;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::{Duration as StdDuration, Instant};
use tokio::sync::Mutex;
use utoipa::ToSchema;
use uuid::Uuid;

const OIDC_COOKIE_NAME: &str = "idp_oidc";
const OIDC_PRODUCTION_COOKIE_NAME: &str = "__Host-idp_oidc";
const MAX_TOKEN_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_JWKS_RESPONSE_BYTES: usize = 1024 * 1024;
const UNKNOWN_KID_REFRESH_COOLDOWN_SECONDS: u64 = 30;
const MAX_PENDING_OIDC_TRANSACTIONS: i64 = 10_000;

#[derive(Serialize, ToSchema)]
pub struct PublicAuthConfig {
    pub password_login_enabled: bool,
    pub entra_login_enabled: bool,
}

#[derive(Default)]
struct JwksCacheState {
    document: Option<JwksDocument>,
    fetched_at: Option<Instant>,
    last_unknown_kid_refresh_at: Option<Instant>,
}

pub struct EntraOidcClient {
    http: reqwest::Client,
    jwks: Mutex<JwksCacheState>,
}

impl EntraOidcClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(StdDuration::from_secs(5))
            .timeout(StdDuration::from_secs(15))
            .redirect(Policy::none())
            .build()
            .expect("the static Entra HTTP client configuration must be valid");

        Self {
            http,
            jwks: Mutex::new(JwksCacheState::default()),
        }
    }

    async fn exchange_code(
        &self,
        config: &EntraConfig,
        code: &str,
        pkce_verifier: &str,
    ) -> Result<String, EntraFlowError> {
        let mut form = vec![
            ("client_id", config.client_id.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", config.redirect_uri.as_str()),
            ("code_verifier", pkce_verifier),
        ];
        if let Some(client_secret) = config.client_secret.as_deref() {
            form.push(("client_secret", client_secret));
        }

        let response = self
            .http
            .post(&config.token_url)
            .header("Accept", "application/json")
            .form(&form)
            .send()
            .await
            .map_err(|_| EntraFlowError::ProviderUnavailable)?;
        if !response.status().is_success() {
            rocket::warn!(
                "Entra token exchange returned HTTP status {}",
                response.status()
            );
            return Err(EntraFlowError::ProviderUnavailable);
        }

        let token: TokenResponse = read_bounded_json(response, MAX_TOKEN_RESPONSE_BYTES).await?;
        if token.id_token.len() > 32 * 1024 || token.id_token.matches('.').count() != 2 {
            return Err(EntraFlowError::AccountNotAllowed);
        }

        Ok(token.id_token)
    }

    async fn decoding_key(
        &self,
        config: &EntraConfig,
        kid: &str,
    ) -> Result<DecodingKey, EntraFlowError> {
        let mut cache = self.jwks.lock().await;
        let now = Instant::now();
        let cache_ttl = StdDuration::from_secs(config.jwks_cache_seconds as u64);
        let cache_is_fresh = cache
            .fetched_at
            .is_some_and(|fetched_at| now.duration_since(fetched_at) < cache_ttl);

        if cache_is_fresh {
            if let Some(document) = cache.document.as_ref() {
                if let Ok(key) = document.key(kid) {
                    return decoding_key_from_jwk(key);
                }
            }

            let cooldown = StdDuration::from_secs(UNKNOWN_KID_REFRESH_COOLDOWN_SECONDS);
            if cache
                .last_unknown_kid_refresh_at
                .is_some_and(|refreshed_at| now.duration_since(refreshed_at) < cooldown)
            {
                return Err(EntraFlowError::AccountNotAllowed);
            }
            cache.last_unknown_kid_refresh_at = Some(now);
        }

        let document = self.fetch_jwks(config).await?;
        cache.document = Some(document);
        cache.fetched_at = Some(Instant::now());
        let key = cache
            .document
            .as_ref()
            .ok_or(EntraFlowError::ProviderUnavailable)?
            .key(kid)
            .and_then(decoding_key_from_jwk)?;

        Ok(key)
    }

    async fn fetch_jwks(&self, config: &EntraConfig) -> Result<JwksDocument, EntraFlowError> {
        let response = self
            .http
            .get(&config.jwks_url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|_| EntraFlowError::ProviderUnavailable)?;
        if !response.status().is_success() {
            rocket::warn!(
                "Entra JWKS endpoint returned HTTP status {}",
                response.status()
            );
            return Err(EntraFlowError::ProviderUnavailable);
        }

        let document: JwksDocument = read_bounded_json(response, MAX_JWKS_RESPONSE_BYTES).await?;
        if document.keys.is_empty() || document.keys.len() > 100 {
            return Err(EntraFlowError::ProviderUnavailable);
        }

        Ok(document)
    }
}

impl Default for EntraOidcClient {
    fn default() -> Self {
        Self::new()
    }
}

#[rocket::get("/auth/config")]
pub fn auth_config(config: &State<AppConfig>) -> NoStoreAuthConfig {
    NoStoreAuthConfig(Json(ApiResponse {
        data: PublicAuthConfig {
            password_login_enabled: config.auth_password_login_enabled,
            entra_login_enabled: config.entra.is_some(),
        },
    }))
}

#[rocket::get("/auth/entra/start?<return_to>")]
pub async fn start_entra_login(
    mut db: Connection<DbConn>,
    config: &State<AppConfig>,
    cookies: &CookieJar<'_>,
    return_to: Option<&str>,
) -> Result<NoStoreRedirect, ApiError> {
    let entra = config.entra.as_ref().ok_or(ApiError::NotFound)?;
    let state = random_urlsafe(32);
    let browser_binding = random_urlsafe(32);
    let nonce = random_urlsafe(32);
    let pkce_verifier = random_urlsafe(64);
    let state_hash = sha256_hex(&state);
    let browser_binding_hash = sha256_hex(&browser_binding);
    let pkce_verifier_ciphertext =
        encrypt_transaction_secret(&pkce_verifier, &state_hash, &entra.transaction_key)
            .map_err(|_| ApiError::Internal)?;
    let return_to = normalize_return_to(return_to);
    let now = Utc::now();

    let created = OidcLoginTransactionRepository::create_bounded(
        &mut db,
        NewOidcLoginTransaction {
            state_hash,
            browser_binding_hash,
            nonce: nonce.clone(),
            pkce_verifier_ciphertext,
            return_to,
            expires_at: now + Duration::seconds(entra.transaction_ttl_seconds),
        },
        now,
        MAX_PENDING_OIDC_TRANSACTIONS,
    )
    .await?;
    if created.is_none() {
        return Err(ApiError::AuthenticationCapacityLimited {
            retry_after_seconds: 60,
        });
    }

    cookies.add(oidc_cookie(&browser_binding, config, entra));
    let authorization_url = build_authorization_url(entra, &state, &nonce, &pkce_verifier)
        .map_err(|_| ApiError::Internal)?;

    Ok(NoStoreRedirect(Redirect::to(authorization_url)))
}

#[derive(FromForm)]
pub struct EntraCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[rocket::get("/auth/entra/callback?<query..>")]
pub async fn finish_entra_login(
    db: &State<DbConn>,
    config: &State<AppConfig>,
    oidc_client: &State<EntraOidcClient>,
    cookies: &CookieJar<'_>,
    client: authorization::LoginClientContext,
    query: EntraCallbackQuery,
) -> NoStoreRedirect {
    let _ = query.error_description.as_deref();
    let default_return_to = "/#dashboard";
    let Some(entra) = config.entra.as_ref() else {
        clear_oidc_cookie(cookies, config);
        return callback_redirect(default_return_to, Err(EntraFlowError::Configuration));
    };

    let browser_binding = cookies
        .get(oidc_cookie_name(config))
        .map(|cookie| cookie.value().to_owned());
    let Some(state) = query.state.as_deref().filter(|value| value.len() <= 256) else {
        return callback_redirect(default_return_to, Err(EntraFlowError::InvalidState));
    };
    let Some(browser_binding) = browser_binding.filter(|value| value.len() <= 256) else {
        return callback_redirect(default_return_to, Err(EntraFlowError::InvalidState));
    };

    let transaction = {
        let mut db = match db.get().await {
            Ok(db) => db,
            Err(pool_error) => {
                rocket::error!(
                    "Could not check out a database connection for OIDC transaction consume: {}",
                    pool_error
                );
                return callback_redirect(
                    default_return_to,
                    Err(EntraFlowError::ProviderUnavailable),
                );
            }
        };
        match OidcLoginTransactionRepository::consume(
            &mut db,
            &sha256_hex(state),
            &sha256_hex(&browser_binding),
            Utc::now(),
        )
        .await
        {
            Ok(transaction) => transaction,
            Err(DieselError::NotFound) => {
                return callback_redirect(default_return_to, Err(EntraFlowError::InvalidState));
            }
            Err(db_error) => {
                rocket::error!("Could not consume OIDC login transaction: {}", db_error);
                return callback_redirect(
                    default_return_to,
                    Err(EntraFlowError::ProviderUnavailable),
                );
            }
        }
    };
    let return_to = transaction.return_to.clone();
    clear_oidc_cookie(cookies, config);

    if let Some(provider_error) = query.error.as_deref() {
        let flow_error = if provider_error == "access_denied" {
            EntraFlowError::AccessDenied
        } else {
            EntraFlowError::ProviderUnavailable
        };
        return callback_redirect(&return_to, Err(flow_error));
    }

    let result = async {
        let code = query
            .code
            .as_deref()
            .filter(|value| !value.is_empty() && value.len() <= 8 * 1024)
            .ok_or(EntraFlowError::InvalidState)?;
        let pkce_verifier = decrypt_transaction_secret(
            &transaction.pkce_verifier_ciphertext,
            &transaction.state_hash,
            &entra.transaction_key,
        )?;
        let id_token = oidc_client
            .exchange_code(entra, code, &pkce_verifier)
            .await?;
        let header = decode_header(&id_token).map_err(|_| EntraFlowError::AccountNotAllowed)?;
        if header.alg != Algorithm::RS256 {
            return Err(EntraFlowError::AccountNotAllowed);
        }
        let kid = header
            .kid
            .filter(|kid| !kid.is_empty() && kid.len() <= 256)
            .ok_or(EntraFlowError::AccountNotAllowed)?;
        let decoding_key = oidc_client.decoding_key(entra, &kid).await?;
        let claims =
            decode_and_validate_id_token(&id_token, &decoding_key, entra, &transaction.nonce)?;
        let mut db = db.get().await.map_err(|pool_error| {
            rocket::error!(
                "Could not check out a database connection to finish OIDC login: {}",
                pool_error
            );
            EntraFlowError::ProviderUnavailable
        })?;
        let user = resolve_portal_user(&mut db, entra, &claims).await?;
        authorization::establish_session(&mut db, user.id, "entra", client, cookies, config)
            .await
            .map_err(|_| EntraFlowError::ProviderUnavailable)?;

        Ok(())
    }
    .await;

    if let Err(flow_error) = &result {
        rocket::warn!("Entra login callback failed: {:?}", flow_error);
    }
    callback_redirect(&return_to, result)
}

async fn resolve_portal_user(
    db: &mut diesel_async::AsyncPgConnection,
    config: &EntraConfig,
    claims: &ValidatedEntraClaims,
) -> Result<User, EntraFlowError> {
    let existing = match ExternalIdentityRepository::find_by_entra_object(
        db,
        &claims.tenant_id,
        &claims.object_id,
    )
    .await
    {
        Ok(identity) => Some(identity),
        Err(DieselError::NotFound) => None,
        Err(_) => return Err(EntraFlowError::ProviderUnavailable),
    };

    let (user, identity) = if let Some(identity) = existing {
        let user = match UserRepository::find(db, identity.user_id).await {
            Ok(user) => user,
            Err(DieselError::NotFound) => return Err(EntraFlowError::AccountNotAllowed),
            Err(_) => return Err(EntraFlowError::ProviderUnavailable),
        };
        (user, identity)
    } else {
        if !config.jit_provisioning {
            return Err(EntraFlowError::AccountNotAllowed);
        }
        let username = jit_username(claims);
        let password = random_password_hash()?;
        let provisioned = ExternalIdentityRepository::find_or_create_jit_user(
            db,
            NewUser { username, password },
            NewExternalIdentity {
                user_id: 0,
                provider: "entra".to_owned(),
                issuer: config.issuer.clone(),
                subject: Some(claims.subject.clone()),
                tenant_id: claims.tenant_id.clone(),
                object_id: claims.object_id.clone(),
                preferred_username: claims.preferred_username.clone(),
                display_name: claims.display_name.clone(),
                email: claims.email.clone(),
                last_login_at: Some(Utc::now()),
            },
        )
        .await
        .map_err(identity_write_error)?;
        if provisioned.created {
            rocket::info!("Provisioned portal user {} from Entra", provisioned.user.id);
        }
        (provisioned.user, provisioned.identity)
    };

    if identity.issuer != config.issuer
        || identity
            .subject
            .as_deref()
            .is_some_and(|subject| subject != claims.subject)
    {
        return Err(EntraFlowError::AccountNotAllowed);
    }
    ExternalIdentityRepository::update_login_profile(
        db,
        identity.id,
        ExternalIdentityLoginProfile {
            issuer: &config.issuer,
            subject: &claims.subject,
            preferred_username: claims.preferred_username.as_deref(),
            display_name: claims.display_name.as_deref(),
            email: claims.email.as_deref(),
            last_login_at: Utc::now(),
        },
    )
    .await
    .map_err(identity_write_error)?;

    Ok(user)
}

fn identity_write_error(error: DieselError) -> EntraFlowError {
    match error {
        DieselError::NotFound => EntraFlowError::AccountNotAllowed,
        DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => {
            EntraFlowError::AccountNotAllowed
        }
        _ => EntraFlowError::ProviderUnavailable,
    }
}

fn decode_and_validate_id_token(
    token: &str,
    key: &DecodingKey,
    config: &EntraConfig,
    expected_nonce: &str,
) -> Result<ValidatedEntraClaims, EntraFlowError> {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    validation.set_audience(&[&config.client_id]);
    validation.set_issuer(&[&config.issuer]);
    validation.leeway = config.clock_skew_seconds as u64;
    validation.validate_nbf = true;
    let raw = decode::<EntraIdTokenClaims>(token, key, &validation)
        .map_err(|_| EntraFlowError::AccountNotAllowed)?
        .claims;

    if raw.version.as_deref() != Some("2.0")
        || raw.nonce.as_deref() != Some(expected_nonce)
        || raw.subject.is_empty()
        || raw.subject.len() > 255
        || raw.subject.chars().any(char::is_control)
    {
        return Err(EntraFlowError::AccountNotAllowed);
    }
    let issued_at = raw.issued_at.ok_or(EntraFlowError::AccountNotAllowed)?;
    if issued_at <= 0 || issued_at > Utc::now().timestamp() + config.clock_skew_seconds {
        return Err(EntraFlowError::AccountNotAllowed);
    }
    let tenant_id = canonical_uuid(&raw.tenant_id)?;
    let configured_tenant_id = canonical_uuid(&config.tenant_id)?;
    if tenant_id != configured_tenant_id {
        return Err(EntraFlowError::AccountNotAllowed);
    }
    let object_id = canonical_uuid(&raw.object_id)?;
    let authorized_party_is_wrong = raw
        .authorized_party
        .as_deref()
        .is_some_and(|authorized_party| authorized_party != config.client_id);
    if authorized_party_is_wrong || (raw.audience.is_multiple() && raw.authorized_party.is_none()) {
        return Err(EntraFlowError::AccountNotAllowed);
    }
    if let Some(required_role) = config.required_role.as_deref() {
        if !raw.roles.iter().any(|role| role == required_role) {
            return Err(EntraFlowError::AccountNotAllowed);
        }
    }

    Ok(ValidatedEntraClaims {
        subject: raw.subject,
        tenant_id,
        object_id,
        preferred_username: bounded_optional(raw.preferred_username, 320)?,
        display_name: bounded_optional(raw.display_name, 256)?,
        email: bounded_optional(raw.email, 320)?,
    })
}

fn build_authorization_url(
    config: &EntraConfig,
    state: &str,
    nonce: &str,
    pkce_verifier: &str,
) -> Result<String, EntraFlowError> {
    let mut url =
        Url::parse(&config.authorization_url).map_err(|_| EntraFlowError::Configuration)?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(pkce_verifier.as_bytes()));
    url.query_pairs_mut()
        .append_pair("client_id", &config.client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &config.redirect_uri)
        .append_pair("response_mode", "query")
        .append_pair("scope", "openid profile email")
        .append_pair("state", state)
        .append_pair("nonce", nonce)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");

    Ok(url.into())
}

async fn read_bounded_json<T: DeserializeOwned>(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<T, EntraFlowError> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(EntraFlowError::ProviderUnavailable);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| EntraFlowError::ProviderUnavailable)?
    {
        if body.len().saturating_add(chunk.len()) > max_bytes {
            return Err(EntraFlowError::ProviderUnavailable);
        }
        body.extend_from_slice(&chunk);
    }

    serde_json::from_slice(&body).map_err(|_| EntraFlowError::ProviderUnavailable)
}

fn normalize_return_to(return_to: Option<&str>) -> String {
    let candidate = return_to.unwrap_or("/#dashboard");
    let fragment = candidate.strip_prefix("/#");
    let safe = candidate.len() <= 512
        && fragment.is_some_and(|fragment| !fragment.is_empty())
        && fragment.is_some_and(|fragment| !fragment.contains(['\\', '#']))
        && !candidate.chars().any(char::is_control)
        && fragment
            .unwrap_or_default()
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "/?&=_%.+-".contains(character));

    if safe {
        candidate.to_owned()
    } else {
        "/#dashboard".to_owned()
    }
}

fn callback_redirect(return_to: &str, result: Result<(), EntraFlowError>) -> NoStoreRedirect {
    NoStoreRedirect(Redirect::to(callback_target(return_to, result)))
}

fn callback_target(return_to: &str, result: Result<(), EntraFlowError>) -> String {
    let fragment = normalize_return_to(Some(return_to))
        .strip_prefix("/#")
        .unwrap_or("dashboard")
        .to_owned();
    let marker = match result {
        Ok(()) => "auth_result=entra",
        Err(error) => match error {
            EntraFlowError::AccessDenied => "auth_error=entra_access_denied",
            EntraFlowError::AccountNotAllowed => "auth_error=entra_account_not_allowed",
            EntraFlowError::InvalidState => "auth_error=entra_invalid_state",
            EntraFlowError::ProviderUnavailable => "auth_error=entra_provider_unavailable",
            EntraFlowError::Configuration => "auth_error=entra_configuration_error",
        },
    };

    format!("/?{marker}#{fragment}")
}

fn oidc_cookie(
    browser_binding: &str,
    app_config: &AppConfig,
    entra: &EntraConfig,
) -> Cookie<'static> {
    Cookie::build((
        oidc_cookie_name(app_config).to_owned(),
        browser_binding.to_owned(),
    ))
    .path("/")
    .http_only(true)
    .secure(app_config.auth_cookie_secure)
    .same_site(SameSite::Lax)
    .max_age(CookieDuration::seconds(entra.transaction_ttl_seconds))
    .build()
}

fn oidc_cookie_name(config: &AppConfig) -> &'static str {
    if config.environment == "production" {
        OIDC_PRODUCTION_COOKIE_NAME
    } else {
        OIDC_COOKIE_NAME
    }
}

fn clear_oidc_cookie(cookies: &CookieJar<'_>, config: &AppConfig) {
    cookies.remove(
        Cookie::build((oidc_cookie_name(config), ""))
            .path("/")
            .secure(config.auth_cookie_secure)
            .same_site(SameSite::Lax)
            .build(),
    );
}

fn encrypt_transaction_secret(
    plaintext: &str,
    state_hash: &str,
    transaction_key: &str,
) -> Result<String, EntraFlowError> {
    let cipher = transaction_cipher(transaction_key)?;
    let mut nonce = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext.as_bytes(),
                aad: state_hash.as_bytes(),
            },
        )
        .map_err(|_| EntraFlowError::Configuration)?;

    Ok(format!(
        "v1.{}.{}",
        URL_SAFE_NO_PAD.encode(nonce),
        URL_SAFE_NO_PAD.encode(ciphertext)
    ))
}

fn decrypt_transaction_secret(
    encrypted: &str,
    state_hash: &str,
    transaction_key: &str,
) -> Result<String, EntraFlowError> {
    let mut parts = encrypted.split('.');
    if parts.next() != Some("v1") {
        return Err(EntraFlowError::InvalidState);
    }
    let nonce = parts
        .next()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .filter(|value| value.len() == 12)
        .ok_or(EntraFlowError::InvalidState)?;
    let ciphertext = parts
        .next()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .filter(|value| !value.is_empty())
        .ok_or(EntraFlowError::InvalidState)?;
    if parts.next().is_some() {
        return Err(EntraFlowError::InvalidState);
    }
    let plaintext = transaction_cipher(transaction_key)?
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: state_hash.as_bytes(),
            },
        )
        .map_err(|_| EntraFlowError::InvalidState)?;

    String::from_utf8(plaintext).map_err(|_| EntraFlowError::InvalidState)
}

fn transaction_cipher(transaction_key: &str) -> Result<Aes256Gcm, EntraFlowError> {
    let digest = Sha256::digest(transaction_key.as_bytes());
    Aes256Gcm::new_from_slice(&digest).map_err(|_| EntraFlowError::Configuration)
}

fn random_urlsafe(byte_count: usize) -> String {
    let mut bytes = vec![0_u8; byte_count];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn sha256_hex(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn random_password_hash() -> Result<String, EntraFlowError> {
    let random_password = random_urlsafe(64);
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(random_password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| EntraFlowError::Configuration)
}

fn jit_username(claims: &ValidatedEntraClaims) -> String {
    let base = claims
        .preferred_username
        .as_deref()
        .and_then(|username| username.split('@').next())
        .unwrap_or("entra")
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let base = base.trim_matches(['.', '_', '-']);
    let base = if base.is_empty() { "entra" } else { base };
    let suffix = claims.object_id.replace('-', "");
    let max_base_len = 64_usize.saturating_sub(suffix.len() + 1);
    let truncated = base.chars().take(max_base_len).collect::<String>();

    format!("{truncated}-{suffix}")
}

fn canonical_uuid(value: &str) -> Result<String, EntraFlowError> {
    Uuid::parse_str(value)
        .map(|uuid| uuid.hyphenated().to_string())
        .map_err(|_| EntraFlowError::AccountNotAllowed)
}

fn bounded_optional(
    value: Option<String>,
    max_length: usize,
) -> Result<Option<String>, EntraFlowError> {
    match value {
        Some(value) if value.len() > max_length || value.chars().any(char::is_control) => {
            Err(EntraFlowError::AccountNotAllowed)
        }
        Some(value) if value.trim().is_empty() => Ok(None),
        value => Ok(value),
    }
}

fn decoding_key_from_jwk(jwk: &Jwk) -> Result<DecodingKey, EntraFlowError> {
    if jwk.key_type != "RSA"
        || jwk
            .algorithm
            .as_deref()
            .is_some_and(|algorithm| algorithm != "RS256")
        || jwk
            .key_use
            .as_deref()
            .is_some_and(|key_use| key_use != "sig")
        || !(256..=16 * 1024).contains(&jwk.modulus.len())
        || jwk.exponent.is_empty()
        || jwk.exponent.len() > 128
    {
        return Err(EntraFlowError::AccountNotAllowed);
    }

    DecodingKey::from_rsa_components(&jwk.modulus, &jwk.exponent)
        .map_err(|_| EntraFlowError::AccountNotAllowed)
}

pub struct NoStoreRedirect(Redirect);

impl<'r> Responder<'r, 'static> for NoStoreRedirect {
    fn respond_to(self, request: &'r Request<'_>) -> response::Result<'static> {
        let mut response = self.0.respond_to(request)?;
        response.set_header(Header::new("Cache-Control", "no-store"));
        response.set_header(Header::new("Pragma", "no-cache"));
        response.set_header(Header::new("Referrer-Policy", "no-referrer"));
        Ok(response)
    }
}

pub struct AuthSecurityHeaders;

#[rocket::async_trait]
impl Fairing for AuthSecurityHeaders {
    fn info(&self) -> Info {
        Info {
            name: "Authentication callback response security headers",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        let path = request.uri().path().as_str();
        let entra_path = path.starts_with("/auth/entra/");
        let connector_oauth_callback = matches!(
            path,
            "/oauth/microsoft/callback" | "/connectors/oauth/microsoft/callback"
        );
        if path == "/auth/config" || entra_path || connector_oauth_callback {
            response.set_header(Header::new("Cache-Control", "no-store"));
            response.set_header(Header::new("Pragma", "no-cache"));
            response.set_header(Header::new("Expires", "0"));
        }
        if entra_path || connector_oauth_callback {
            response.set_header(Header::new("Referrer-Policy", "no-referrer"));
        }
    }
}

pub struct NoStoreAuthConfig(Json<ApiResponse<PublicAuthConfig>>);

impl<'r> Responder<'r, 'static> for NoStoreAuthConfig {
    fn respond_to(self, request: &'r Request<'_>) -> response::Result<'static> {
        let mut response = self.0.respond_to(request)?;
        response.set_header(Header::new("Cache-Control", "no-store"));
        response.set_header(Header::new("Pragma", "no-cache"));
        Ok(response)
    }
}

#[derive(Debug)]
enum EntraFlowError {
    AccessDenied,
    AccountNotAllowed,
    InvalidState,
    ProviderUnavailable,
    Configuration,
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
}

#[derive(Clone, Default, Deserialize)]
struct JwksDocument {
    keys: Vec<Jwk>,
}

impl JwksDocument {
    fn key(&self, kid: &str) -> Result<&Jwk, EntraFlowError> {
        let mut matching = self.keys.iter().filter(|key| key.key_id == kid);
        let key = matching.next().ok_or(EntraFlowError::AccountNotAllowed)?;
        if matching.next().is_some() {
            return Err(EntraFlowError::AccountNotAllowed);
        }

        Ok(key)
    }
}

#[derive(Clone, Deserialize)]
struct Jwk {
    #[serde(rename = "kty")]
    key_type: String,
    #[serde(rename = "use")]
    key_use: Option<String>,
    #[serde(rename = "alg")]
    algorithm: Option<String>,
    #[serde(rename = "kid")]
    key_id: String,
    #[serde(rename = "n")]
    modulus: String,
    #[serde(rename = "e")]
    exponent: String,
}

#[derive(Deserialize)]
struct EntraIdTokenClaims {
    #[serde(rename = "sub")]
    subject: String,
    #[serde(rename = "tid")]
    tenant_id: String,
    #[serde(rename = "oid")]
    object_id: String,
    #[serde(rename = "ver")]
    version: Option<String>,
    nonce: Option<String>,
    #[serde(rename = "aud")]
    audience: AudienceClaim,
    #[serde(rename = "azp")]
    authorized_party: Option<String>,
    #[serde(rename = "iat")]
    issued_at: Option<i64>,
    #[serde(default)]
    roles: Vec<String>,
    preferred_username: Option<String>,
    #[serde(rename = "name")]
    display_name: Option<String>,
    email: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AudienceClaim {
    One(String),
    Many(Vec<String>),
}

impl AudienceClaim {
    fn is_multiple(&self) -> bool {
        match self {
            Self::One(value) => {
                let _ = value;
                false
            }
            Self::Many(values) => values.len() > 1,
        }
    }
}

struct ValidatedEntraClaims {
    subject: String,
    tenant_id: String,
    object_id: String,
    preferred_username: Option<String>,
    display_name: Option<String>,
    email: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::{json, Value};

    const TEST_RSA_PRIVATE_DER: &str = "MIIEpAIBAAKCAQEAyRE6rHuNR0QbHO3H3Kt2pOKGVhQqGZXInOduQNxXzuKlvQTLUTv4l4sggh5/CYYi/cvI+SXVT9kPWSKXxJXBXd/4LkvcPuUakBoAkfh+eiFVMh2VrUyWyj3MFl0HTVF9KwRXLAcwkREiS3npThHRyIxuy0ZMeZfxVL5arMhw1SRELB8HoGfG/AtH89BIE9jDBHZ9dLelK9a184zAf8LwoPLxvJb3Il5nncqPcSfKDDodMFBIMc4lQzDKL5gvmiXLXB1AGLm8KBjfE8s3L5xqi+yUod+j8MtvIj812dkS4QMiRVN/by2h3ZY8LYVGrqZXZTcgn2ujn8uKjXLZVD5TdQIDAQABAoIBAHREk0I0O9DvECKdWUpAmF3mY7oY9PNQiu44Yaf+AoSuyRpRUGTMIgc3u3eivOE8ALX0BmYUO5JtuRNZDpvt4SAwqCnVUinIf6C+eH/wSurCpapSM0BAHp4aOA7igptyOMgMPYBHNA1e9A7jE0dCxKWMl3DSWNyjQTk4zeRGEAEfbNjHrq6YCtjHSZSLmWiG80hnfnYos9hOr5JnLnyS7ZmFE/5P3XVrxLc/tQ5zum0R4cbrgzHiQP5RgfxGJaEi7XcgherCCOgurJSSbYH29Gz8u5fFbS+Yg8s+OiCss3cs1rSgJ9/eHZuzGEdUZVARH6hVMjSuwvqVTFaE8AgtleECgYEA+uLMn4kNqHlJS2A5uAnCkj90ZxEtNm3E8hAxUrhssktY5XSOAPBlxyf5RuRGIImGtUVIr4HuJSa5TX48n3Vdt9MYCprO/iYl6moNRSPt5qowIIOJmIjY2mqPDfDt/zw+fcDD3lmCJrFlzcnh0uea1CohxEbQnL3cypeLt+WbU6kCgYEAzSp19m1ajieFkqgoB0YTpt/OroDx38vvI5unInJlEeOjQ+oIAQdN2wpxBvTrRorMU6P07mFUbt1j+Co6CbNiw+X8HcCaqYLR5clbJOOWNR36PuzOpQLkfK8woupBxzW9B8gZmY8rB1mbJ+/WTPrEJy6YGmIEBkWylQ2VpW8O4O0CgYEApdbvvfFBlwD9YxbrcGz7MeNCFbMz+MucqQntIKoKJ91ImPxvtc0y6e/Rhnv0oyNlaUOwJVu0yNgNG117w0g4t/+Q38mvVC5xV7/cn7x9UMFk6MkqVir3dYGEqIl/OP1grY2Tq9HtB5iyG9L8NIamQOLMyUqqMUILxdthHyFmiGkCgYEAn9+PjpjGMPHxL0gj8Q8VbzsFtou6b1deIRRA2CHmSltltR1gYVTMwXxQeUhPMmgkMqUXzs4/WijgpthY44hK1TaZEKIuoxrS70nJ4WQLf5a9k1065fDsFZD6yGjdGxvwEmlGMZgTwqV7t1I4X0Ilqhav5hcs5apYL7gnPYPeRz0CgYALHCj/Ji8XSsDoF/MhVhnGdIs2P99NNdmo3R2Pv0CuZbDKMU559LJHUvrKS8WkuWRDuKrz1W/EQKApFjDGpdqToZqriUFQzwy7mR3ayIiogzNtHcvbDHx8oFnGY0OFksX/ye0/XGpy2SFxYRwGU98HPYeBvAQQrVjdkzfy7BmXQQ==";
    const TEST_RSA_PUBLIC_DER: &str = "MIIBCgKCAQEAyRE6rHuNR0QbHO3H3Kt2pOKGVhQqGZXInOduQNxXzuKlvQTLUTv4l4sggh5/CYYi/cvI+SXVT9kPWSKXxJXBXd/4LkvcPuUakBoAkfh+eiFVMh2VrUyWyj3MFl0HTVF9KwRXLAcwkREiS3npThHRyIxuy0ZMeZfxVL5arMhw1SRELB8HoGfG/AtH89BIE9jDBHZ9dLelK9a184zAf8LwoPLxvJb3Il5nncqPcSfKDDodMFBIMc4lQzDKL5gvmiXLXB1AGLm8KBjfE8s3L5xqi+yUod+j8MtvIj812dkS4QMiRVN/by2h3ZY8LYVGrqZXZTcgn2ujn8uKjXLZVD5TdQIDAQAB";
    const TEST_RSA_MODULUS: &str = "yRE6rHuNR0QbHO3H3Kt2pOKGVhQqGZXInOduQNxXzuKlvQTLUTv4l4sggh5_CYYi_cvI-SXVT9kPWSKXxJXBXd_4LkvcPuUakBoAkfh-eiFVMh2VrUyWyj3MFl0HTVF9KwRXLAcwkREiS3npThHRyIxuy0ZMeZfxVL5arMhw1SRELB8HoGfG_AtH89BIE9jDBHZ9dLelK9a184zAf8LwoPLxvJb3Il5nncqPcSfKDDodMFBIMc4lQzDKL5gvmiXLXB1AGLm8KBjfE8s3L5xqi-yUod-j8MtvIj812dkS4QMiRVN_by2h3ZY8LYVGrqZXZTcgn2ujn8uKjXLZVD5TdQ";

    fn test_entra_config() -> EntraConfig {
        EntraConfig {
            tenant_id: "11111111-1111-4111-8111-111111111111".to_owned(),
            client_id: "22222222-2222-4222-8222-222222222222".to_owned(),
            client_secret: Some("client-secret".to_owned()),
            transaction_key: "0123456789abcdef0123456789abcdef".to_owned(),
            redirect_uri: "https://portal.example/auth/entra/callback".to_owned(),
            issuer: "https://login.microsoftonline.com/11111111-1111-4111-8111-111111111111/v2.0"
                .to_owned(),
            authorization_url: "https://login.microsoftonline.com/authorize".to_owned(),
            token_url: "https://login.microsoftonline.com/token".to_owned(),
            jwks_url: "https://login.microsoftonline.com/keys".to_owned(),
            jit_provisioning: false,
            required_role: Some("Portal.Member".to_owned()),
            transaction_ttl_seconds: 600,
            jwks_cache_seconds: 300,
            clock_skew_seconds: 120,
        }
    }

    #[rocket::get("/auth/entra/callback")]
    fn callback_failure_before_handler() -> rocket::http::Status {
        rocket::http::Status::ServiceUnavailable
    }

    #[rocket::get("/oauth/microsoft/callback")]
    fn connector_oauth_callback_page_for_headers() -> rocket::http::Status {
        rocket::http::Status::Ok
    }

    fn valid_id_token_claims(config: &EntraConfig) -> Value {
        let now = Utc::now().timestamp();
        json!({
            "iss": config.issuer,
            "aud": config.client_id,
            "exp": now + 300,
            "nbf": now - 10,
            "iat": now - 10,
            "sub": "subject-for-this-client",
            "nonce": "expected-nonce",
            "tid": config.tenant_id,
            "oid": "33333333-3333-4333-8333-333333333333",
            "ver": "2.0",
            "roles": ["Portal.Member"],
            "preferred_username": "alice@example.com",
            "name": "Alice Example",
            "email": "alice@example.com"
        })
    }

    fn sign_id_token(claims: &Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".to_owned());
        let private_key = STANDARD.decode(TEST_RSA_PRIVATE_DER).unwrap();
        encode(&header, claims, &EncodingKey::from_rsa_der(&private_key)).unwrap()
    }

    fn test_decoding_key() -> DecodingKey {
        let public_key = STANDARD.decode(TEST_RSA_PUBLIC_DER).unwrap();
        DecodingKey::from_rsa_der(&public_key)
    }

    async fn read_test_http_request(stream: &mut tokio::net::TcpStream) -> (String, String) {
        use tokio::io::AsyncReadExt;

        let mut request = Vec::new();
        let header_end = loop {
            let mut buffer = [0_u8; 4_096];
            let read = stream.read(&mut buffer).await.unwrap();
            assert!(read > 0, "mock provider request closed before headers");
            request.extend_from_slice(&buffer[..read]);
            if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                break end + 4;
            }
            assert!(
                request.len() <= 64 * 1024,
                "mock provider request too large"
            );
        };
        let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("content-length:")
                    .or_else(|| line.strip_prefix("Content-Length:"))
            })
            .map(str::trim)
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        while request.len() < header_end + content_length {
            let mut buffer = [0_u8; 4_096];
            let read = stream.read(&mut buffer).await.unwrap();
            assert!(read > 0, "mock provider request closed before body");
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

    async fn write_test_http_json(stream: &mut tokio::net::TcpStream, body: &str) {
        use tokio::io::AsyncWriteExt;

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
    }

    #[test]
    fn authorization_url_uses_pkce_and_oidc_state() {
        let config = test_entra_config();
        let url = build_authorization_url(&config, "state-value", "nonce-value", "verifier")
            .expect("authorization URL");
        let url = Url::parse(&url).unwrap();
        let query = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(query.get("response_type").unwrap(), "code");
        assert_eq!(query.get("response_mode").unwrap(), "query");
        assert_eq!(query.get("scope").unwrap(), "openid profile email");
        assert_eq!(query.get("state").unwrap(), "state-value");
        assert_eq!(query.get("nonce").unwrap(), "nonce-value");
        assert_eq!(query.get("code_challenge_method").unwrap(), "S256");
        assert_ne!(query.get("code_challenge").unwrap(), "verifier");
        assert!(!url.as_str().contains("client-secret"));
    }

    #[test]
    fn public_auth_config_is_minimal_and_not_cacheable() {
        let app_config = AppConfig {
            environment: "test".to_owned(),
            auth_token_ttl_seconds: 3_600,
            auth_max_active_sessions_per_user: 10,
            auth_cookie_secure: false,
            auth_login_max_failures: 5,
            auth_login_account_max_failures: 50,
            auth_login_window_seconds: 900,
            auth_login_lockout_seconds: 900,
            auth_password_login_enabled: true,
            entra: Some(test_entra_config()),
        };
        let rocket = rocket::build()
            .manage(app_config)
            .mount("/", rocket::routes![auth_config]);
        let client = rocket::local::blocking::Client::tracked(rocket).unwrap();
        let response = client.get("/auth/config").dispatch();

        assert_eq!(response.status(), rocket::http::Status::Ok);
        assert_eq!(
            response.headers().get_one("Cache-Control"),
            Some("no-store")
        );
        assert_eq!(response.headers().get_one("Pragma"), Some("no-cache"));
        let body = response.into_json::<serde_json::Value>().unwrap();
        assert_eq!(
            body,
            json!({
                "data": {
                    "password_login_enabled": true,
                    "entra_login_enabled": true
                }
            })
        );
    }

    #[test]
    fn callback_failure_before_handler_still_has_security_headers() {
        let rocket = rocket::build()
            .attach(AuthSecurityHeaders)
            .mount("/", rocket::routes![callback_failure_before_handler]);
        let client = rocket::local::blocking::Client::tracked(rocket).unwrap();
        let response = client
            .get("/auth/entra/callback?code=secret&state=secret")
            .dispatch();

        assert_eq!(response.status(), rocket::http::Status::ServiceUnavailable);
        assert_eq!(
            response.headers().get_one("Cache-Control"),
            Some("no-store")
        );
        assert_eq!(
            response.headers().get_one("Referrer-Policy"),
            Some("no-referrer")
        );
        assert_eq!(response.headers().get_one("Expires"), Some("0"));
    }

    #[test]
    fn connector_oauth_callback_never_leaks_query_through_referrer_or_cache() {
        let rocket = rocket::build().attach(AuthSecurityHeaders).mount(
            "/",
            rocket::routes![connector_oauth_callback_page_for_headers],
        );
        let client = rocket::local::blocking::Client::tracked(rocket).unwrap();
        let response = client
            .get("/oauth/microsoft/callback?code=secret&state=secret")
            .dispatch();

        assert_eq!(response.status(), rocket::http::Status::Ok);
        assert_eq!(
            response.headers().get_one("Cache-Control"),
            Some("no-store")
        );
        assert_eq!(
            response.headers().get_one("Referrer-Policy"),
            Some("no-referrer")
        );
        assert_eq!(response.headers().get_one("Pragma"), Some("no-cache"));
        assert_eq!(response.headers().get_one("Expires"), Some("0"));
    }

    #[test]
    fn transaction_secrets_are_encrypted_and_bound_to_state() {
        let key = "0123456789abcdef0123456789abcdef";
        let encrypted = encrypt_transaction_secret("pkce-verifier", "state-a", key).unwrap();

        assert!(!encrypted.contains("pkce-verifier"));
        assert_eq!(
            decrypt_transaction_secret(&encrypted, "state-a", key).unwrap(),
            "pkce-verifier"
        );
        assert!(decrypt_transaction_secret(&encrypted, "state-b", key).is_err());
    }

    #[test]
    fn id_token_requires_valid_signature_and_all_identity_bindings() {
        let config = test_entra_config();
        let claims = valid_id_token_claims(&config);
        let token = sign_id_token(&claims);
        let validated =
            decode_and_validate_id_token(&token, &test_decoding_key(), &config, "expected-nonce")
                .expect("valid signed token");
        assert_eq!(validated.tenant_id, "11111111-1111-4111-8111-111111111111");
        assert_eq!(validated.object_id, "33333333-3333-4333-8333-333333333333");

        for (claim, invalid_value) in [
            ("nonce", json!("wrong-nonce")),
            ("tid", json!("44444444-4444-4444-8444-444444444444")),
            ("aud", json!("55555555-5555-4555-8555-555555555555")),
            ("roles", json!(["Some.Other.Role"])),
            ("ver", json!("1.0")),
        ] {
            let mut invalid_claims = valid_id_token_claims(&config);
            invalid_claims[claim] = invalid_value;
            assert!(
                decode_and_validate_id_token(
                    &sign_id_token(&invalid_claims),
                    &test_decoding_key(),
                    &config,
                    "expected-nonce",
                )
                .is_err(),
                "claim {claim} must be rejected"
            );
        }

        let mut tampered = token.into_bytes();
        let last = tampered.last_mut().unwrap();
        *last = if *last == b'A' { b'B' } else { b'A' };
        assert!(decode_and_validate_id_token(
            &String::from_utf8(tampered).unwrap(),
            &test_decoding_key(),
            &config,
            "expected-nonce",
        )
        .is_err());
    }

    #[test]
    fn id_token_rejects_bad_times_and_requires_azp_for_multiple_audiences() {
        let config = test_entra_config();
        let now = Utc::now().timestamp();

        for (claim, invalid_value) in [
            ("exp", json!(now - 500)),
            ("nbf", json!(now + config.clock_skew_seconds + 60)),
            ("nbf", json!("99999999999")),
            ("iat", json!(now + config.clock_skew_seconds + 60)),
        ] {
            let mut claims = valid_id_token_claims(&config);
            claims[claim] = invalid_value;
            assert!(
                decode_and_validate_id_token(
                    &sign_id_token(&claims),
                    &test_decoding_key(),
                    &config,
                    "expected-nonce",
                )
                .is_err(),
                "claim {claim} must be rejected"
            );
        }

        let mut claims = valid_id_token_claims(&config);
        claims["aud"] = json!([config.client_id, "another-audience"]);
        assert!(decode_and_validate_id_token(
            &sign_id_token(&claims),
            &test_decoding_key(),
            &config,
            "expected-nonce",
        )
        .is_err());
        claims["azp"] = json!(config.client_id);
        assert!(decode_and_validate_id_token(
            &sign_id_token(&claims),
            &test_decoding_key(),
            &config,
            "expected-nonce",
        )
        .is_ok());
    }

    #[rocket::async_test]
    async fn mock_provider_covers_token_exchange_and_jwks_cache() {
        use std::sync::{Arc, Mutex as StdMutex};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut config = test_entra_config();
        config.issuer = format!("http://{address}/issuer");
        config.token_url = format!("http://{address}/token");
        config.jwks_url = format!("http://{address}/keys");
        let token = sign_id_token(&valid_id_token_claims(&config));
        let jwks = json!({
            "keys": [{
                "kty": "RSA",
                "use": "sig",
                "alg": "RS256",
                "kid": "test-key",
                "n": TEST_RSA_MODULUS,
                "e": "AQAB"
            }]
        })
        .to_string();
        let requests = Arc::new(StdMutex::new(Vec::new()));
        let server_requests = Arc::clone(&requests);
        let server = rocket::tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (path, body) = read_test_http_request(&mut stream).await;
                server_requests.lock().unwrap().push((path.clone(), body));
                let response = match path.as_str() {
                    "/token" => json!({ "token_type": "Bearer", "id_token": token }).to_string(),
                    "/keys" => jwks.clone(),
                    _ => panic!("unexpected mock provider path: {path}"),
                };
                write_test_http_json(&mut stream, &response).await;
            }
        });

        let client = EntraOidcClient::new();
        let id_token = client
            .exchange_code(&config, "authorization-code", "pkce-verifier")
            .await
            .expect("mock token exchange");
        let key = client
            .decoding_key(&config, "test-key")
            .await
            .expect("mock JWKS key");
        decode_and_validate_id_token(&id_token, &key, &config, "expected-nonce")
            .expect("mock token validation");
        client
            .decoding_key(&config, "test-key")
            .await
            .expect("cached mock JWKS key");
        server.await.unwrap();

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2, "the second key lookup must use cache");
        assert_eq!(requests[0].0, "/token");
        assert!(requests[0].1.contains("grant_type=authorization_code"));
        assert!(requests[0].1.contains("code=authorization-code"));
        assert!(requests[0].1.contains("code_verifier=pkce-verifier"));
        assert!(requests[0].1.contains("client_secret=client-secret"));
        assert_eq!(requests[1].0, "/keys");
    }

    #[test]
    fn return_to_never_becomes_an_open_redirect() {
        assert_eq!(normalize_return_to(Some("/#catalog")), "/#catalog");
        assert_eq!(
            normalize_return_to(Some("/#connectors?source=graph&runId=42")),
            "/#connectors?source=graph&runId=42"
        );
        assert_eq!(
            normalize_return_to(Some("/#connectors?source=graph+mail")),
            "/#connectors?source=graph+mail"
        );
        for unsafe_value in [
            "https://evil.example/",
            "//evil.example/",
            "/\\evil.example/",
            "/#dashboard#https://evil.example",
            "/#dashboard\r\nLocation:https://evil.example",
        ] {
            assert_eq!(normalize_return_to(Some(unsafe_value)), "/#dashboard");
        }
    }

    #[test]
    fn callback_only_uses_fixed_markers_and_safe_fragment() {
        assert_eq!(
            callback_target("/#work-cards/42", Ok(())),
            "/?auth_result=entra#work-cards/42"
        );
        assert_eq!(
            callback_target("https://evil.example/", Err(EntraFlowError::InvalidState)),
            "/?auth_error=entra_invalid_state#dashboard"
        );
    }

    #[test]
    fn jit_username_is_deterministic_and_does_not_link_by_email() {
        let claims = ValidatedEntraClaims {
            subject: "subject".to_owned(),
            tenant_id: "11111111-1111-4111-8111-111111111111".to_owned(),
            object_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".to_owned(),
            preferred_username: Some("Alice.Example@example.com".to_owned()),
            display_name: None,
            email: Some("different@example.com".to_owned()),
        };

        assert_eq!(
            jit_username(&claims),
            "alice.example-aaaaaaaaaaaa4aaa8aaaaaaaaaaaaaaa"
        );
    }
}
