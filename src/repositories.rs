use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};

use crate::{models::*, schema::*};

pub struct MaintainerRepository;

impl MaintainerRepository {
    pub async fn find(c: &mut AsyncPgConnection, id: i32) -> QueryResult<Maintainer> {
        maintainers::table.find(id).get_result(c).await
    }

    pub async fn find_multiple(
        c: &mut AsyncPgConnection,
        limit: i64,
    ) -> QueryResult<Vec<Maintainer>> {
        maintainers::table.limit(limit).get_results(c).await
    }

    pub async fn create(
        c: &mut AsyncPgConnection,
        new_maintainer: NewMaintainer,
    ) -> QueryResult<Maintainer> {
        diesel::insert_into(maintainers::table)
            .values(new_maintainer)
            .get_result(c)
            .await
    }

    pub async fn update(
        c: &mut AsyncPgConnection,
        id: i32,
        maintainer: NewMaintainer,
    ) -> QueryResult<Maintainer> {
        diesel::update(maintainers::table.find(id))
            .set(maintainer)
            .get_result(c)
            .await
    }

    pub async fn delete(c: &mut AsyncPgConnection, id: i32) -> QueryResult<usize> {
        diesel::delete(maintainers::table.find(id)).execute(c).await
    }
}

pub struct PackageRepository;

impl PackageRepository {
    pub async fn find(c: &mut AsyncPgConnection, id: i32) -> QueryResult<Package> {
        packages::table.find(id).get_result(c).await
    }

    pub async fn find_multiple(c: &mut AsyncPgConnection, limit: i64) -> QueryResult<Vec<Package>> {
        packages::table.limit(limit).get_results(c).await
    }

    pub async fn create(
        c: &mut AsyncPgConnection,
        new_package: NewPackage,
    ) -> QueryResult<Package> {
        diesel::insert_into(packages::table)
            .values(new_package)
            .get_result(c)
            .await
    }

    pub async fn update(
        c: &mut AsyncPgConnection,
        id: i32,
        package: NewPackage,
    ) -> QueryResult<Package> {
        diesel::update(packages::table.find(id))
            .set((package, packages::updated_at.eq(diesel::dsl::now)))
            .get_result(c)
            .await
    }

    pub async fn delete(c: &mut AsyncPgConnection, id: i32) -> QueryResult<usize> {
        diesel::delete(packages::table.find(id)).execute(c).await
    }
}

pub struct UserRepository;

impl UserRepository {
    pub async fn find(c: &mut AsyncPgConnection, id: i32) -> QueryResult<User> {
        users::table.find(id).get_result(c).await
    }

    pub async fn find_by_username(c: &mut AsyncPgConnection, username: &str) -> QueryResult<User> {
        users::table
            .filter(users::username.eq(username))
            .get_result(c)
            .await
    }
    pub async fn find_with_roles(
        c: &mut AsyncPgConnection,
    ) -> QueryResult<Vec<(User, Vec<(UserRole, Role)>)>> {
        let users = users::table.load::<User>(c).await?;
        let result = users_roles::table
            .inner_join(roles::table)
            .load::<(UserRole, Role)>(c)
            .await?
            .grouped_by(&users);

        Ok(users.into_iter().zip(result).collect())
    }

    pub async fn create(
        c: &mut AsyncPgConnection,
        new_user: NewUser,
        role_codes: Vec<String>,
    ) -> QueryResult<User> {
        c.transaction::<_, diesel::result::Error, _>(|conn| {
            Box::pin(async move {
                let user = diesel::insert_into(users::table)
                    .values(new_user)
                    .get_result::<User>(conn)
                    .await?;

                for role_code in role_codes {
                    let role = match RoleRepository::find_by_code(conn, &role_code).await {
                        Ok(role) => role,
                        Err(diesel::result::Error::NotFound) => {
                            let new_role = NewRole {
                                code: role_code.to_owned(),
                                name: role_code.to_owned(),
                            };
                            match RoleRepository::create(conn, new_role).await {
                                Ok(role) => role,
                                Err(diesel::result::Error::DatabaseError(
                                    diesel::result::DatabaseErrorKind::UniqueViolation,
                                    _,
                                )) => RoleRepository::find_by_code(conn, &role_code).await?,
                                Err(error) => return Err(error),
                            }
                        }
                        Err(e) => return Err(e),
                    };

                    let new_user_role = NewUserRole {
                        user_id: user.id,
                        role_id: role.id,
                    };

                    diesel::insert_into(users_roles::table)
                        .values(new_user_role)
                        .get_result::<UserRole>(conn)
                        .await?;
                }

                Ok(user)
            })
        })
        .await
    }

    pub async fn delete(c: &mut AsyncPgConnection, id: i32) -> QueryResult<usize> {
        diesel::delete(users_roles::table.filter(users_roles::user_id.eq(id)))
            .execute(c)
            .await?;

        diesel::delete(users::table.find(id)).execute(c).await
    }
}

pub struct SessionRepository;

impl SessionRepository {
    pub async fn create(
        c: &mut AsyncPgConnection,
        user_id: i32,
        token: String,
        expires_at: NaiveDateTime,
    ) -> QueryResult<Session> {
        diesel::insert_into(sessions::table)
            .values(NewSession {
                user_id,
                token,
                expires_at,
            })
            .get_result(c)
            .await
    }

    pub async fn find_by_token(c: &mut AsyncPgConnection, token: &str) -> QueryResult<Session> {
        sessions::table
            .filter(sessions::token.eq(token))
            .first::<Session>(c)
            .await
    }

    pub async fn delete_by_token(c: &mut AsyncPgConnection, token: &str) -> QueryResult<usize> {
        diesel::delete(sessions::table.filter(sessions::token.eq(token)))
            .execute(c)
            .await
    }
}

pub struct RoleRepository;

impl RoleRepository {
    pub async fn find_by_ids(c: &mut AsyncPgConnection, ids: Vec<i32>) -> QueryResult<Vec<Role>> {
        roles::table.filter(roles::id.eq_any(ids)).load(c).await
    }

    pub async fn find_by_code(c: &mut AsyncPgConnection, code: &str) -> QueryResult<Role> {
        roles::table.filter(roles::code.eq(code)).first(c).await
    }

    pub async fn find_by_user(c: &mut AsyncPgConnection, user: &User) -> QueryResult<Vec<Role>> {
        let user_roles = UserRole::belonging_to(&user)
            .get_results::<UserRole>(c)
            .await?;
        let role_ids: Vec<i32> = user_roles.iter().map(|ur: &UserRole| ur.role_id).collect();

        Self::find_by_ids(c, role_ids).await
    }

    pub async fn create(c: &mut AsyncPgConnection, new_role: NewRole) -> QueryResult<Role> {
        diesel::insert_into(roles::table)
            .values(new_role)
            .get_result(c)
            .await
    }
}
