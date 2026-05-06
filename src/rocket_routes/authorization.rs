use crate::{
    api::{ok, ApiError, ApiResult},
    auth::AuthenticatedUser,
    config::AppConfig,
    repositories::{SessionRepository, UserRepository},
    rocket_routes::DbConn,
    validation::{required, FieldViolation, Validate},
};
use argon2::{PasswordHash, PasswordVerifier};
use chrono::{Duration, NaiveDateTime, Utc};
use diesel::result::Error as DieselError;
use rocket::response::status::NoContent;
use rocket::serde::json::Json;
use rocket::serde::Serialize;
use rocket::State;
use rocket_db_pools::Connection;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct Credentials {
    username: String,
    password: String,
}

impl Validate for Credentials {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        required(&mut errors, "username", &self.username);
        required(&mut errors, "password", &self.password);

        errors
    }
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub token_type: &'static str,
    pub expires_at: NaiveDateTime,
}

#[derive(Serialize)]
pub struct MeResponse {
    pub id: i32,
    pub username: String,
    pub roles: Vec<String>,
}

#[rocket::post("/login", format = "json", data = "<credentials>")]
pub async fn login(
    mut db: Connection<DbConn>,
    config: &State<AppConfig>,
    credentials: Json<Credentials>,
) -> ApiResult<LoginResponse> {
    let credentials = crate::validation::validate_request(credentials.into_inner())?;

    let user = match UserRepository::find_by_username(&mut db, &credentials.username).await {
        Ok(user) => user,
        Err(DieselError::NotFound) => return Err(ApiError::Unauthorized),
        Err(e) => return Err(e.into()),
    };

    let argon2 = argon2::Argon2::default();
    let db_hash = PasswordHash::new(&user.password).map_err(|e| {
        rocket::error!("Invalid password hash for user {}: {}", user.username, e);
        ApiError::Internal
    })?;

    if argon2
        .verify_password(credentials.password.as_bytes(), &db_hash)
        .is_ok()
    {
        let token = generate_token();
        let expires_at = Utc::now().naive_utc() + Duration::seconds(config.auth_token_ttl_seconds);

        SessionRepository::create(&mut db, user.id, token.clone(), expires_at).await?;

        ok(LoginResponse {
            token,
            token_type: "Bearer",
            expires_at,
        })
    } else {
        Err(ApiError::Unauthorized)
    }
}

#[rocket::get("/me")]
pub async fn me(auth: AuthenticatedUser) -> ApiResult<MeResponse> {
    ok(MeResponse {
        id: auth.user.id,
        username: auth.user.username,
        roles: auth.roles.into_iter().map(|role| role.code).collect(),
    })
}

#[rocket::post("/logout")]
pub async fn logout(
    mut db: Connection<DbConn>,
    auth: AuthenticatedUser,
) -> Result<NoContent, ApiError> {
    SessionRepository::delete_by_token(&mut db, &auth.session.token).await?;

    Ok(NoContent)
}

fn generate_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
