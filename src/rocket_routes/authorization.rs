use crate::{
    repositories::UserRepository,
    rocket_routes::{server_error, DbConn},
};
use argon2::{PasswordHash, PasswordVerifier};
use rocket::response::status::Custom;
use rocket::serde::json::{json, Json, Value};
use rocket_db_pools::Connection;

#[derive(serde::Deserialize)]
pub struct Credentials {
    username: String,
    password: String,
}

#[rocket::post("/login", format = "json", data = "<credentials>")]
pub async fn login(
    mut db: Connection<DbConn>,
    credentials: Json<Credentials>,
) -> Result<Value, Custom<Value>> {
    UserRepository::find_by_username(&mut db, &credentials.username)
        .await
        .map(|user| {
            let argon2 = argon2::Argon2::default();
            let db_hash = PasswordHash::new(&user.password).unwrap();
            let result = argon2.verify_password(credentials.password.as_bytes(), &db_hash);
            if result.is_ok() {
                return json!("Success");
            }
            json!("Unauthorized")
        })
        .map_err(|e| server_error(e.into()))
}
