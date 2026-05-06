pub mod authorization;
pub mod health;
pub mod maintainers;
pub mod packages;

#[derive(rocket_db_pools::Database)]
#[database("postgres")]
pub struct DbConn(rocket_db_pools::diesel::PgPool);
