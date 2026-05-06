use chrono::Utc;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket_db_pools::Connection;

use crate::api::ApiError;
use crate::models::{Role, Session, User};
use crate::repositories::{RoleRepository, SessionRepository, UserRepository};
use crate::rocket_routes::DbConn;

pub struct AuthenticatedUser {
    pub user: User,
    pub session: Session,
    pub roles: Vec<Role>,
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
