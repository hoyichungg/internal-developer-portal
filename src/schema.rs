// @generated automatically by Diesel CLI.

diesel::table! {
    audit_logs (id) {
        id -> Int4,
        actor_user_id -> Nullable<Int4>,
        #[max_length = 64]
        action -> Varchar,
        #[max_length = 64]
        resource_type -> Varchar,
        #[max_length = 128]
        resource_id -> Nullable<Varchar>,
        metadata -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    connector_configs (id) {
        id -> Int4,
        #[max_length = 64]
        source -> Varchar,
        #[max_length = 64]
        target -> Varchar,
        enabled -> Bool,
        #[max_length = 128]
        schedule_cron -> Nullable<Varchar>,
        config -> Text,
        sample_payload -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        last_scheduled_at -> Nullable<Timestamp>,
        next_run_at -> Nullable<Timestamp>,
        last_scheduled_run_id -> Nullable<Int4>,
    }
}

diesel::table! {
    connector_run_item_errors (id) {
        id -> Int4,
        connector_run_id -> Int4,
        #[max_length = 64]
        source -> Varchar,
        #[max_length = 64]
        target -> Varchar,
        #[max_length = 128]
        external_id -> Nullable<Varchar>,
        message -> Text,
        raw_item -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    connector_run_items (id) {
        id -> Int4,
        connector_run_id -> Int4,
        #[max_length = 64]
        source -> Varchar,
        #[max_length = 64]
        target -> Varchar,
        record_id -> Nullable<Int4>,
        #[max_length = 128]
        external_id -> Nullable<Varchar>,
        #[max_length = 32]
        status -> Varchar,
        snapshot -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    connector_runs (id) {
        id -> Int4,
        #[max_length = 64]
        source -> Varchar,
        #[max_length = 64]
        target -> Varchar,
        #[max_length = 32]
        status -> Varchar,
        success_count -> Int4,
        failure_count -> Int4,
        duration_ms -> Int8,
        error_message -> Nullable<Text>,
        started_at -> Timestamp,
        finished_at -> Nullable<Timestamp>,
        #[max_length = 32]
        trigger -> Varchar,
        payload -> Nullable<Text>,
        claimed_at -> Nullable<Timestamp>,
        #[max_length = 128]
        worker_id -> Nullable<Varchar>,
    }
}

diesel::table! {
    connector_workers (id) {
        id -> Int4,
        #[max_length = 128]
        worker_id -> Varchar,
        #[max_length = 32]
        status -> Varchar,
        scheduler_enabled -> Bool,
        retention_enabled -> Bool,
        current_run_id -> Nullable<Int4>,
        last_error -> Nullable<Text>,
        started_at -> Timestamp,
        last_seen_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    connectors (id) {
        id -> Int4,
        #[max_length = 64]
        source -> Varchar,
        #[max_length = 64]
        kind -> Varchar,
        #[max_length = 128]
        display_name -> Varchar,
        #[max_length = 32]
        status -> Varchar,
        last_run_at -> Nullable<Timestamp>,
        last_success_at -> Nullable<Timestamp>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    maintainer_members (id) {
        id -> Int4,
        maintainer_id -> Int4,
        user_id -> Int4,
        #[max_length = 32]
        role -> Varchar,
        created_at -> Timestamp,
    }
}

diesel::table! {
    maintainers (id) {
        id -> Int4,
        display_name -> Varchar,
        email -> Varchar,
        created_at -> Timestamp,
    }
}

diesel::table! {
    maintenance_runs (id) {
        id -> Int4,
        #[max_length = 64]
        task -> Varchar,
        #[max_length = 32]
        status -> Varchar,
        #[max_length = 128]
        worker_id -> Nullable<Varchar>,
        started_at -> Timestamp,
        finished_at -> Timestamp,
        duration_ms -> Int8,
        health_checks_deleted -> Int4,
        connector_runs_deleted -> Int4,
        audit_logs_deleted -> Int4,
        error_message -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    notifications (id) {
        id -> Int4,
        #[max_length = 64]
        source -> Varchar,
        #[max_length = 255]
        title -> Varchar,
        body -> Nullable<Text>,
        #[max_length = 32]
        severity -> Varchar,
        is_read -> Bool,
        #[max_length = 2048]
        url -> Nullable<Varchar>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        #[max_length = 128]
        external_id -> Nullable<Varchar>,
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
        description -> Nullable<Text>,
        created_at -> Timestamp,
        #[max_length = 32]
        status -> Varchar,
        #[max_length = 2048]
        repository_url -> Nullable<Varchar>,
        #[max_length = 2048]
        documentation_url -> Nullable<Varchar>,
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
    service_health_checks (id) {
        id -> Int4,
        service_id -> Int4,
        connector_run_id -> Nullable<Int4>,
        #[max_length = 64]
        source -> Varchar,
        #[max_length = 128]
        external_id -> Nullable<Varchar>,
        #[max_length = 32]
        health_status -> Varchar,
        #[max_length = 32]
        previous_health_status -> Nullable<Varchar>,
        checked_at -> Timestamp,
        response_time_ms -> Nullable<Int4>,
        message -> Nullable<Text>,
        raw_payload -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    services (id) {
        id -> Int4,
        maintainer_id -> Int4,
        #[max_length = 64]
        slug -> Varchar,
        #[max_length = 128]
        name -> Varchar,
        #[max_length = 32]
        lifecycle_status -> Varchar,
        #[max_length = 32]
        health_status -> Varchar,
        description -> Nullable<Text>,
        #[max_length = 2048]
        repository_url -> Nullable<Varchar>,
        #[max_length = 2048]
        dashboard_url -> Nullable<Varchar>,
        #[max_length = 2048]
        runbook_url -> Nullable<Varchar>,
        last_checked_at -> Nullable<Timestamp>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        #[max_length = 64]
        source -> Varchar,
        #[max_length = 128]
        external_id -> Nullable<Varchar>,
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

diesel::table! {
    work_cards (id) {
        id -> Int4,
        #[max_length = 64]
        source -> Varchar,
        #[max_length = 128]
        external_id -> Nullable<Varchar>,
        #[max_length = 255]
        title -> Varchar,
        #[max_length = 32]
        status -> Varchar,
        #[max_length = 32]
        priority -> Varchar,
        #[max_length = 128]
        assignee -> Nullable<Varchar>,
        due_at -> Nullable<Timestamp>,
        #[max_length = 2048]
        url -> Nullable<Varchar>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::joinable!(audit_logs -> users (actor_user_id));
diesel::joinable!(connector_configs -> connector_runs (last_scheduled_run_id));
diesel::joinable!(connector_run_item_errors -> connector_runs (connector_run_id));
diesel::joinable!(connector_run_items -> connector_runs (connector_run_id));
diesel::joinable!(connector_workers -> connector_runs (current_run_id));
diesel::joinable!(maintainer_members -> maintainers (maintainer_id));
diesel::joinable!(maintainer_members -> users (user_id));
diesel::joinable!(packages -> maintainers (maintainer_id));
diesel::joinable!(service_health_checks -> connector_runs (connector_run_id));
diesel::joinable!(service_health_checks -> services (service_id));
diesel::joinable!(services -> maintainers (maintainer_id));
diesel::joinable!(sessions -> users (user_id));
diesel::joinable!(users_roles -> roles (role_id));
diesel::joinable!(users_roles -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    audit_logs,
    connector_configs,
    connector_run_item_errors,
    connector_run_items,
    connector_runs,
    connector_workers,
    connectors,
    maintainer_members,
    maintainers,
    maintenance_runs,
    notifications,
    packages,
    roles,
    service_health_checks,
    services,
    sessions,
    users,
    users_roles,
    work_cards,
);
