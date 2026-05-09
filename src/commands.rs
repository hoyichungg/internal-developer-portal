use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
use diesel::result::Error as DieselError;
use diesel_async::{AsyncConnection, AsyncPgConnection};

use crate::{
    models::NewUser,
    repositories::{RoleRepository, UserRepository, UserRoleRepository},
};

async fn load_db_connection() -> AsyncPgConnection {
    let database_url = std::env::var("DATABASE_URL").expect("Cannot load DB url environment");
    AsyncPgConnection::establish(&database_url)
        .await
        .expect("Cannot connect to Postgres")
}

pub async fn create_user(username: String, password: String, role_codes: Vec<String>) {
    let mut c = load_db_connection().await;

    let new_user = NewUser {
        username,
        password: hash_password(&password),
    };
    let user = UserRepository::create(&mut c, new_user, role_codes)
        .await
        .unwrap();
    let roles = RoleRepository::find_by_user(&mut c, &user).await.unwrap();
    println!("User created id={} username={}", user.id, user.username);
    println!("Roles assigned {}", role_codes_summary(&roles));
}

pub async fn ensure_admin_user(
    username: String,
    password: String,
    role_codes: Vec<String>,
    reset_password: bool,
) {
    let mut c = load_db_connection().await;
    let role_codes = normalized_roles(role_codes);
    let user = match UserRepository::find_by_username(&mut c, &username).await {
        Ok(user) if reset_password => {
            UserRepository::update_password(&mut c, user.id, hash_password(&password))
                .await
                .unwrap()
        }
        Ok(user) => user,
        Err(DieselError::NotFound) => {
            let new_user = NewUser {
                username: username.clone(),
                password: hash_password(&password),
            };
            UserRepository::create(&mut c, new_user, Vec::new())
                .await
                .unwrap()
        }
        Err(error) => panic!("Cannot load admin user: {error}"),
    };

    for role_code in role_codes {
        let role = RoleRepository::find_or_create_by_code(&mut c, &role_code)
            .await
            .unwrap();
        UserRoleRepository::assign_if_missing(&mut c, user.id, role.id)
            .await
            .unwrap();
    }

    let roles = RoleRepository::find_by_user(&mut c, &user).await.unwrap();
    println!(
        "Admin user ensured id={} username={}",
        user.id, user.username
    );
    println!("Roles assigned {}", role_codes_summary(&roles));
}

pub async fn list_users() {
    let mut c = load_db_connection().await;

    let users = UserRepository::find_with_roles(&mut c).await.unwrap();
    for (user, roles) in users {
        let role_codes = roles
            .iter()
            .map(|(_, role)| role.code.as_str())
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "id={} username={} roles={}",
            user.id, user.username, role_codes
        );
    }
}

pub async fn delete_user(id: i32) {
    let mut c = load_db_connection().await;

    UserRepository::delete(&mut c, id).await.unwrap();
}

fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(OsRng);
    let argon2 = argon2::Argon2::default();

    argon2
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn normalized_roles(role_codes: Vec<String>) -> Vec<String> {
    let mut roles = role_codes
        .into_iter()
        .flat_map(|role| {
            role.split(',')
                .map(str::trim)
                .filter(|role| !role.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    if roles.is_empty() {
        roles.push("admin".to_owned());
        roles.push("member".to_owned());
    }

    roles.sort();
    roles.dedup();
    roles
}

fn role_codes_summary(roles: &[crate::models::Role]) -> String {
    roles
        .iter()
        .map(|role| role.code.as_str())
        .collect::<Vec<_>>()
        .join(",")
}
