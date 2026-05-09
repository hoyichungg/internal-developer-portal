pub mod audit_logs;
pub mod authorization;
pub mod connectors;
pub mod dashboard;
pub mod health;
pub mod maintainers;
pub mod notifications;
pub mod packages;
pub mod services;
pub mod work_cards;

#[derive(rocket_db_pools::Database)]
#[database("postgres")]
pub struct DbConn(rocket_db_pools::diesel::PgPool);
