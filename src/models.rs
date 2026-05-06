use crate::schema::*;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use rocket::serde::Serialize;
use serde::Deserialize;

use crate::validation::{
    email, max_len, max_optional_len, one_of, optional_url, positive, required, FieldViolation,
    Validate,
};

#[derive(Queryable, Serialize, Deserialize)]
pub struct Maintainer {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub display_name: String,
    pub email: String,
    #[serde(skip_deserializing)]
    pub created_at: NaiveDateTime,
}

#[derive(AsChangeset, Insertable, Deserialize)]
#[diesel(table_name=maintainers)]
pub struct NewMaintainer {
    pub display_name: String,
    pub email: String,
}

impl Validate for NewMaintainer {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        required(&mut errors, "display_name", &self.display_name);
        max_len(&mut errors, "display_name", &self.display_name, 255);
        required(&mut errors, "email", &self.email);
        max_len(&mut errors, "email", &self.email, 255);
        email(&mut errors, "email", &self.email);

        errors
    }
}

#[derive(Queryable, Serialize, Deserialize)]
pub struct Package {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub maintainer_id: i32,
    pub slug: String,
    pub name: String,
    pub version: String,
    pub status: String,
    pub description: Option<String>,
    pub repository_url: Option<String>,
    pub documentation_url: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: NaiveDateTime,
    #[serde(skip_deserializing)]
    pub updated_at: NaiveDateTime,
}

#[derive(AsChangeset, Insertable, Deserialize)]
#[diesel(table_name=packages)]
#[diesel(treat_none_as_null = true)]
pub struct NewPackage {
    pub maintainer_id: i32,
    pub slug: String,
    pub name: String,
    pub version: String,
    pub status: String,
    pub description: Option<String>,
    pub repository_url: Option<String>,
    pub documentation_url: Option<String>,
}

impl Validate for NewPackage {
    fn validate(&self) -> Vec<FieldViolation> {
        let mut errors = Vec::new();

        positive(&mut errors, "maintainer_id", self.maintainer_id);
        required(&mut errors, "slug", &self.slug);
        max_len(&mut errors, "slug", &self.slug, 64);
        required(&mut errors, "name", &self.name);
        max_len(&mut errors, "name", &self.name, 128);
        required(&mut errors, "version", &self.version);
        max_len(&mut errors, "version", &self.version, 64);
        required(&mut errors, "status", &self.status);
        max_len(&mut errors, "status", &self.status, 32);
        one_of(
            &mut errors,
            "status",
            &self.status,
            &["active", "deprecated", "archived"],
        );
        max_optional_len(&mut errors, "repository_url", &self.repository_url, 2048);
        optional_url(&mut errors, "repository_url", &self.repository_url);
        max_optional_len(
            &mut errors,
            "documentation_url",
            &self.documentation_url,
            2048,
        );
        optional_url(&mut errors, "documentation_url", &self.documentation_url);

        errors
    }
}

#[derive(Queryable, Debug, Identifiable)]
pub struct User {
    pub id: i32,
    pub username: String,
    pub password: String,
    pub created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name=users)]
pub struct NewUser {
    pub username: String,
    pub password: String,
}

#[derive(Queryable, Identifiable, Debug)]
pub struct Role {
    pub id: i32,
    pub code: String,
    pub name: String,
    pub created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name=roles)]
pub struct NewRole {
    pub code: String,
    pub name: String,
}

#[derive(Queryable, Associations, Identifiable, Debug)]
#[diesel(belongs_to(User))]
#[diesel(belongs_to(Role))]
#[diesel(table_name=users_roles)]

pub struct UserRole {
    pub id: i32,
    pub user_id: i32,
    pub role_id: i32,
}

#[derive(Insertable)]
#[diesel(table_name=users_roles)]
pub struct NewUserRole {
    pub user_id: i32,
    pub role_id: i32,
}

#[derive(Queryable, Associations, Identifiable, Debug)]
#[diesel(belongs_to(User))]
#[diesel(table_name=sessions)]
pub struct Session {
    pub id: i32,
    pub user_id: i32,
    pub token: String,
    pub expires_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name=sessions)]
pub struct NewSession {
    pub user_id: i32,
    pub token: String,
    pub expires_at: NaiveDateTime,
}
