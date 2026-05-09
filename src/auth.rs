use chrono::Utc;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket_db_pools::Connection;

use crate::api::ApiError;
use crate::models::{Role, Session, User};
use crate::repositories::{
    MaintainerMemberRepository, RoleRepository, SessionRepository, UserRepository,
};
use crate::rocket_routes::DbConn;

pub struct AuthenticatedUser {
    pub user: User,
    pub session: Session,
    pub roles: Vec<Role>,
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
        let token = match bearer_token(request) {
            Some(token) => token,
            None => return Outcome::Error((Status::Unauthorized, ApiError::Unauthorized)),
        };

        let mut db = match request.guard::<Connection<DbConn>>().await {
            Outcome::Success(db) => db,
            Outcome::Error((status, _)) => return Outcome::Error((status, ApiError::Internal)),
            Outcome::Forward(status) => return Outcome::Forward(status),
        };

        let session = match SessionRepository::find_by_token(&mut db, &token).await {
            Ok(session) => session,
            Err(diesel::result::Error::NotFound) => {
                return Outcome::Error((Status::Unauthorized, ApiError::Unauthorized))
            }
            Err(error) => return Outcome::Error((Status::InternalServerError, error.into())),
        };

        if session.expires_at <= Utc::now().naive_utc() {
            let _ = SessionRepository::delete_by_token(&mut db, &token).await;
            return Outcome::Error((Status::Unauthorized, ApiError::Unauthorized));
        }

        let user = match UserRepository::find(&mut db, session.user_id).await {
            Ok(user) => user,
            Err(error) => return Outcome::Error((Status::InternalServerError, error.into())),
        };

        let roles = match RoleRepository::find_by_user(&mut db, &user).await {
            Ok(roles) => roles,
            Err(error) => return Outcome::Error((Status::InternalServerError, error.into())),
        };

        Outcome::Success(Self {
            user,
            session,
            roles,
        })
    }
}

fn bearer_token(request: &Request<'_>) -> Option<String> {
    request
        .headers()
        .get_one("Authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|token| !token.trim().is_empty())
        .map(str::to_owned)
}
