use chrono::Utc;
use rocket::http::{Method, Status};
use rocket::request::{FromRequest, Outcome, Request};
use rocket_db_pools::Connection;

use crate::api::ApiError;
use crate::config::AppConfig;
use crate::models::{Role, Session, User};
use crate::repositories::{
    MaintainerMemberRepository, RecordAccessScope, RoleRepository, SessionRepository,
    UserRepository,
};
use crate::rocket_routes::DbConn;

pub const SESSION_COOKIE_NAME: &str = "idp_session";
pub const PRODUCTION_SESSION_COOKIE_NAME: &str = "__Host-idp_session";
pub const CSRF_HEADER_NAME: &str = "X-IDP-CSRF";

pub fn session_cookie_name(config: &AppConfig) -> &'static str {
    if config.environment == "production" {
        PRODUCTION_SESSION_COOKIE_NAME
    } else {
        SESSION_COOKIE_NAME
    }
}

enum RequestToken {
    Bearer(String),
    Cookie(String),
}

/// Authenticated portal identity loaded from the session store.
///
/// Rocket evaluates request guards from left to right and keeps successful
/// guard values alive until the route handler returns. Route handlers that
/// also need `Connection<DbConn>` must therefore declare this guard first so
/// the temporary authentication checkout is returned to the pool before the
/// route checks out its own connection.
pub struct AuthenticatedUser {
    pub user: User,
    pub session: Session,
    pub roles: Vec<Role>,
    pub token: String,
}

impl AuthenticatedUser {
    pub fn has_role(&self, role_code: &str) -> bool {
        self.roles.iter().any(|role| role.code == role_code)
    }

    pub fn is_admin(&self) -> bool {
        self.has_role("admin")
    }
}

pub fn require_admin(auth: &AuthenticatedUser) -> Result<(), ApiError> {
    if auth.is_admin() {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

pub async fn require_maintainer_write_access(
    db: &mut Connection<DbConn>,
    auth: &AuthenticatedUser,
    maintainer_id: i32,
) -> Result<(), ApiError> {
    require_maintainer_member_role(db, auth, maintainer_id, &["owner", "maintainer"]).await
}

pub async fn require_maintainer_owner_access(
    db: &mut Connection<DbConn>,
    auth: &AuthenticatedUser,
    maintainer_id: i32,
) -> Result<(), ApiError> {
    require_maintainer_member_role(db, auth, maintainer_id, &["owner"]).await
}

pub async fn require_user_directory_access(
    db: &mut Connection<DbConn>,
    auth: &AuthenticatedUser,
) -> Result<(), ApiError> {
    if auth.is_admin() {
        return Ok(());
    }

    if MaintainerMemberRepository::find_by_user(db, auth.user.id)
        .await?
        .iter()
        .any(|member| member.role == "owner")
    {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

pub async fn can_view_maintainer_members(
    db: &mut Connection<DbConn>,
    auth: &AuthenticatedUser,
    maintainer_id: i32,
) -> Result<bool, ApiError> {
    if auth.is_admin() {
        return Ok(true);
    }

    match MaintainerMemberRepository::find_by_maintainer_and_user(db, maintainer_id, auth.user.id)
        .await
    {
        Ok(_) => Ok(true),
        Err(diesel::result::Error::NotFound) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub async fn record_access_scope(
    db: &mut Connection<DbConn>,
    auth: &AuthenticatedUser,
) -> Result<RecordAccessScope, ApiError> {
    let memberships = MaintainerMemberRepository::find_by_user(db, auth.user.id).await?;

    Ok(RecordAccessScope {
        user_id: auth.user.id,
        is_admin: auth.is_admin(),
        maintainer_ids: memberships
            .into_iter()
            .map(|membership| membership.maintainer_id)
            .collect(),
    })
}

async fn require_maintainer_member_role(
    db: &mut Connection<DbConn>,
    auth: &AuthenticatedUser,
    maintainer_id: i32,
    allowed_roles: &[&str],
) -> Result<(), ApiError> {
    if auth.is_admin() {
        return Ok(());
    }

    let membership =
        MaintainerMemberRepository::find_by_maintainer_and_user(db, maintainer_id, auth.user.id)
            .await;

    match membership {
        Ok(member) if allowed_roles.iter().any(|role| *role == member.role) => Ok(()),
        Ok(_) | Err(diesel::result::Error::NotFound) => Err(ApiError::Forbidden),
        Err(error) => Err(error.into()),
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AuthenticatedUser {
    type Error = ApiError;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let config = match request.rocket().state::<AppConfig>() {
            Some(config) => config,
            None => {
                rocket::error!("AppConfig is not managed by Rocket");
                return Outcome::Error((Status::InternalServerError, ApiError::Internal));
            }
        };
        let request_token = match request_token(request, session_cookie_name(config)) {
            Some(token) => token,
            None => return Outcome::Error((Status::Unauthorized, ApiError::Unauthorized)),
        };
        let (token, authenticated_with_cookie) = match request_token {
            RequestToken::Bearer(token) => (token, false),
            RequestToken::Cookie(token) => (token, true),
        };
        if authenticated_with_cookie
            && !matches!(
                request.method(),
                Method::Get | Method::Head | Method::Options
            )
            && request.headers().get_one(CSRF_HEADER_NAME) != Some("1")
        {
            return Outcome::Error((Status::Forbidden, ApiError::Forbidden));
        }

        let mut db = match request.guard::<Connection<DbConn>>().await {
            Outcome::Success(db) => db,
            Outcome::Error((status, _)) if status == Status::ServiceUnavailable => {
                return Outcome::Error((status, ApiError::ServiceUnavailable))
            }
            Outcome::Error((status, _)) => return Outcome::Error((status, ApiError::Internal)),
            Outcome::Forward(status) => return Outcome::Forward(status),
        };

        let session = match SessionRepository::find_by_token(&mut db, &token).await {
            Ok(session) => session,
            Err(diesel::result::Error::NotFound) => {
                return Outcome::Error((Status::Unauthorized, ApiError::Unauthorized))
            }
            Err(error) => {
                let error = ApiError::from(error);
                return Outcome::Error((error.status(), error));
            }
        };

        if session.expires_at <= Utc::now() {
            let _ = SessionRepository::delete_by_token(&mut db, &token).await;
            return Outcome::Error((Status::Unauthorized, ApiError::Unauthorized));
        }

        let user = match UserRepository::find(&mut db, session.user_id).await {
            Ok(user) => user,
            Err(diesel::result::Error::NotFound) => {
                let _ = SessionRepository::delete_by_token(&mut db, &token).await;
                return Outcome::Error((Status::Unauthorized, ApiError::Unauthorized));
            }
            Err(error) => {
                let error = ApiError::from(error);
                return Outcome::Error((error.status(), error));
            }
        };

        let roles = match RoleRepository::find_by_user(&mut db, &user).await {
            Ok(roles) => roles,
            Err(error) => {
                let error = ApiError::from(error);
                return Outcome::Error((error.status(), error));
            }
        };

        Outcome::Success(Self {
            user,
            session,
            roles,
            token,
        })
    }
}

fn request_token(request: &Request<'_>, cookie_name: &str) -> Option<RequestToken> {
    if request.headers().contains("Authorization") {
        return bearer_token(request).map(RequestToken::Bearer);
    }

    request
        .cookies()
        .get(cookie_name)
        .map(|cookie| cookie.value().to_owned())
        .filter(|token| !token.trim().is_empty())
        .map(RequestToken::Cookie)
}

fn bearer_token(request: &Request<'_>) -> Option<String> {
    request
        .headers()
        .get_one("Authorization")
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
        .map(|(_, token)| token)
        .filter(|token| !token.trim().is_empty())
        .map(str::to_owned)
}
