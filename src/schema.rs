// @generated automatically by Diesel CLI.

diesel::table! {
    login_throttle_buckets (bucket_hash) {
        #[max_length = 64]
        bucket_hash -> Varchar,
        failure_count -> Int4,
        window_started_at -> Timestamptz,
        locked_until -> Nullable<Timestamptz>,
        updated_at -> Timestamptz,
    }
}

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
        created_at -> Timestamptz,
    }
}

diesel::table! {
    calendar_events (id) {
        id -> Int4,
        #[max_length = 64]
        source -> Varchar,
        #[max_length = 128]
        external_id -> Varchar,
        #[max_length = 256]
        title -> Varchar,
        body -> Nullable<Text>,
        #[max_length = 256]
        organizer -> Nullable<Varchar>,
        #[max_length = 256]
        location -> Nullable<Varchar>,
        starts_at -> Timestamptz,
        ends_at -> Timestamptz,
        #[max_length = 128]
        time_zone -> Nullable<Varchar>,
        is_all_day -> Bool,
        is_cancelled -> Bool,
        #[max_length = 2048]
        web_url -> Nullable<Varchar>,
        #[max_length = 2048]
        join_url -> Nullable<Varchar>,
        connector_id -> Nullable<Int4>,
        owner_user_id -> Nullable<Int4>,
        maintainer_id -> Nullable<Int4>,
        source_updated_at -> Nullable<Timestamptz>,
        last_seen_run_id -> Nullable<Int4>,
        archived_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
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
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        last_scheduled_at -> Nullable<Timestamptz>,
        next_run_at -> Nullable<Timestamptz>,
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
        created_at -> Timestamptz,
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
        created_at -> Timestamptz,
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
        started_at -> Timestamptz,
        finished_at -> Nullable<Timestamptz>,
        #[max_length = 32]
        trigger -> Varchar,
        payload -> Nullable<Text>,
        claimed_at -> Nullable<Timestamptz>,
        #[max_length = 128]
        worker_id -> Nullable<Varchar>,
        attempt_count -> Int4,
        max_attempts -> Int4,
        next_attempt_at -> Timestamptz,
        lease_expires_at -> Nullable<Timestamptz>,
        heartbeat_at -> Nullable<Timestamptz>,
        cancel_requested_at -> Nullable<Timestamptz>,
        cancelled_at -> Nullable<Timestamptz>,
        parent_run_id -> Nullable<Int4>,
        snapshot_complete -> Nullable<Bool>,
        archived_count -> Int4,
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
        started_at -> Timestamptz,
        last_seen_at -> Timestamptz,
        updated_at -> Timestamptz,
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
        last_run_at -> Nullable<Timestamptz>,
        last_success_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        #[max_length = 16]
        scope_type -> Varchar,
        owner_user_id -> Nullable<Int4>,
        maintainer_id -> Nullable<Int4>,
    }
}

diesel::table! {
    external_identities (id) {
        id -> Int4,
        user_id -> Int4,
        #[max_length = 32]
        provider -> Varchar,
        #[max_length = 255]
        issuer -> Varchar,
        #[max_length = 255]
        subject -> Nullable<Varchar>,
        #[max_length = 36]
        tenant_id -> Varchar,
        #[max_length = 36]
        object_id -> Varchar,
        #[max_length = 320]
        preferred_username -> Nullable<Varchar>,
        #[max_length = 256]
        display_name -> Nullable<Varchar>,
        #[max_length = 320]
        email -> Nullable<Varchar>,
        last_login_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    maintainer_members (id) {
        id -> Int4,
        maintainer_id -> Int4,
        user_id -> Int4,
        #[max_length = 32]
        role -> Varchar,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    maintainers (id) {
        id -> Int4,
        display_name -> Varchar,
        email -> Varchar,
        created_at -> Timestamptz,
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
        started_at -> Timestamptz,
        finished_at -> Timestamptz,
        duration_ms -> Int8,
        health_checks_deleted -> Int4,
        connector_runs_deleted -> Int4,
        audit_logs_deleted -> Int4,
        error_message -> Nullable<Text>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    notification_receipts (id) {
        id -> Int4,
        notification_id -> Int4,
        user_id -> Int4,
        read_at -> Nullable<Timestamptz>,
        dismissed_at -> Nullable<Timestamptz>,
        snoozed_until -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
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
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        #[max_length = 128]
        external_id -> Nullable<Varchar>,
        connector_id -> Nullable<Int4>,
        owner_user_id -> Nullable<Int4>,
        maintainer_id -> Nullable<Int4>,
        source_updated_at -> Nullable<Timestamptz>,
        last_seen_run_id -> Nullable<Int4>,
        archived_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    oidc_login_transactions (state_hash) {
        #[max_length = 64]
        state_hash -> Varchar,
        #[max_length = 64]
        browser_binding_hash -> Varchar,
        #[max_length = 128]
        nonce -> Varchar,
        pkce_verifier_ciphertext -> Text,
        #[max_length = 512]
        return_to -> Varchar,
        expires_at -> Timestamptz,
        created_at -> Timestamptz,
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
        created_at -> Timestamptz,
        #[max_length = 32]
        status -> Varchar,
        #[max_length = 2048]
        repository_url -> Nullable<Varchar>,
        #[max_length = 2048]
        documentation_url -> Nullable<Varchar>,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    roles (id) {
        id -> Int4,
        #[max_length = 64]
        code -> Varchar,
        #[max_length = 128]
        name -> Varchar,
        created_at -> Timestamptz,
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
        checked_at -> Timestamptz,
        response_time_ms -> Nullable<Int4>,
        message -> Nullable<Text>,
        raw_payload -> Nullable<Text>,
        created_at -> Timestamptz,
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
        last_checked_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
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
        #[max_length = 64]
        token_hash -> Varchar,
        expires_at -> Timestamptz,
        created_at -> Timestamptz,
        #[max_length = 32]
        auth_method -> Varchar,
        last_seen_at -> Timestamptz,
        #[max_length = 64]
        ip_address -> Nullable<Varchar>,
        #[max_length = 512]
        user_agent -> Nullable<Varchar>,
    }
}

diesel::table! {
    users (id) {
        id -> Int4,
        #[max_length = 64]
        username -> Varchar,
        #[max_length = 128]
        password -> Varchar,
        created_at -> Timestamptz,
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
        #[max_length = 128]
        project -> Nullable<Varchar>,
        #[max_length = 128]
        work_item_type -> Nullable<Varchar>,
        #[max_length = 512]
        assignee_source_id -> Nullable<Varchar>,
        assignee_user_id -> Nullable<Int4>,
        due_at -> Nullable<Timestamptz>,
        #[max_length = 2048]
        url -> Nullable<Varchar>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        connector_id -> Nullable<Int4>,
        owner_user_id -> Nullable<Int4>,
        maintainer_id -> Nullable<Int4>,
        source_updated_at -> Nullable<Timestamptz>,
        last_seen_run_id -> Nullable<Int4>,
        archived_at -> Nullable<Timestamptz>,
    }
}

diesel::joinable!(audit_logs -> users (actor_user_id));
diesel::joinable!(calendar_events -> connector_runs (last_seen_run_id));
diesel::joinable!(calendar_events -> connectors (connector_id));
diesel::joinable!(calendar_events -> maintainers (maintainer_id));
diesel::joinable!(calendar_events -> users (owner_user_id));
diesel::joinable!(connector_configs -> connector_runs (last_scheduled_run_id));
diesel::joinable!(connector_run_item_errors -> connector_runs (connector_run_id));
diesel::joinable!(connector_run_items -> connector_runs (connector_run_id));
diesel::joinable!(connector_workers -> connector_runs (current_run_id));
diesel::joinable!(connectors -> maintainers (maintainer_id));
diesel::joinable!(connectors -> users (owner_user_id));
diesel::joinable!(external_identities -> users (user_id));
diesel::joinable!(maintainer_members -> maintainers (maintainer_id));
diesel::joinable!(maintainer_members -> users (user_id));
diesel::joinable!(notification_receipts -> notifications (notification_id));
diesel::joinable!(notification_receipts -> users (user_id));
diesel::joinable!(notifications -> connector_runs (last_seen_run_id));
diesel::joinable!(notifications -> connectors (connector_id));
diesel::joinable!(notifications -> maintainers (maintainer_id));
diesel::joinable!(notifications -> users (owner_user_id));
diesel::joinable!(packages -> maintainers (maintainer_id));
diesel::joinable!(service_health_checks -> connector_runs (connector_run_id));
diesel::joinable!(service_health_checks -> services (service_id));
diesel::joinable!(services -> maintainers (maintainer_id));
diesel::joinable!(sessions -> users (user_id));
diesel::joinable!(users_roles -> roles (role_id));
diesel::joinable!(users_roles -> users (user_id));
diesel::joinable!(work_cards -> connector_runs (last_seen_run_id));
diesel::joinable!(work_cards -> connectors (connector_id));
diesel::joinable!(work_cards -> maintainers (maintainer_id));
diesel::joinable!(work_cards -> users (owner_user_id));

diesel::allow_tables_to_appear_in_same_query!(
    audit_logs,
    calendar_events,
    connector_configs,
    connector_run_item_errors,
    connector_run_items,
    connector_runs,
    connector_workers,
    external_identities,
    login_throttle_buckets,
    connectors,
    maintainer_members,
    maintainers,
    maintenance_runs,
    notification_receipts,
    notifications,
    oidc_login_transactions,
    packages,
    roles,
    service_health_checks,
    services,
    sessions,
    users,
    users_roles,
    work_cards,
);
