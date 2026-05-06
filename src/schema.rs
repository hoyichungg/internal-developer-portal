// @generated automatically by Diesel CLI.

diesel::table! {
    maintainers (id) {
        id -> Int4,
        display_name -> Varchar,
        email -> Varchar,
        created_at -> Timestamp,
    }
}

diesel::table! {
    packages (id) {
        id -> Int4,
        maintainer_id -> Int4,
        #[max_length = 64]
        slug -> Varchar,
        #[max_length = 128]
        name -> Varchar,
        #[max_length = 64]
        version -> Varchar,
        #[max_length = 32]
        status -> Varchar,
        description -> Nullable<Text>,
        #[max_length = 2048]
        repository_url -> Nullable<Varchar>,
        #[max_length = 2048]
        documentation_url -> Nullable<Varchar>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    roles (id) {
        id -> Int4,
        #[max_length = 64]
        code -> Varchar,
        #[max_length = 128]
        name -> Varchar,
        created_at -> Timestamp,
    }
}

diesel::table! {
    sessions (id) {
        id -> Int4,
        user_id -> Int4,
        #[max_length = 128]
        token -> Varchar,
        expires_at -> Timestamp,
        created_at -> Timestamp,
    }
}

diesel::table! {
    users (id) {
        id -> Int4,
        #[max_length = 64]
        username -> Varchar,
        #[max_length = 128]
        password -> Varchar,
        created_at -> Timestamp,
    }
}

diesel::table! {
    users_roles (id) {
        id -> Int4,
        user_id -> Int4,
        role_id -> Int4,
    }
}

diesel::joinable!(packages -> maintainers (maintainer_id));
diesel::joinable!(sessions -> users (user_id));
diesel::joinable!(users_roles -> roles (role_id));
diesel::joinable!(users_roles -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    maintainers,
    packages,
    roles,
    sessions,
    users,
    users_roles,
);
