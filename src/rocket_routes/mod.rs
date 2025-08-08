use rocket::http::Status;
use rocket::response::status::Custom;
use rocket::serde::json::{json, Value};
use std::error::Error;

pub mod authorization;
pub mod crates;
pub mod rustaceans;

#[derive(rocket_db_pools::Database)]
#[database("postgres")]
pub struct DbConn(rocket_db_pools::diesel::PgPool);

pub fn server_error(e: Box<dyn Error>) -> Custom<Value> {
    rocket::error!("{}", e);
    Custom(Status::InternalServerError, json!("Error"))
}
