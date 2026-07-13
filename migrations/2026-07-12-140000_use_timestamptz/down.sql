-- A rollback keeps the same instant by materializing every value as a naive
-- UTC wall-clock timestamp. Never use a session-time-zone-dependent cast.
SET LOCAL TIME ZONE 'UTC';
SET LOCAL lock_timeout = '10s';

ALTER TABLE audit_logs
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE calendar_events
  DROP CONSTRAINT calendar_events_time_check,
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN starts_at TYPE TIMESTAMP USING (starts_at AT TIME ZONE 'UTC'),
  ALTER COLUMN ends_at TYPE TIMESTAMP USING (ends_at AT TIME ZONE 'UTC'),
  ALTER COLUMN source_updated_at TYPE TIMESTAMP USING (source_updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN archived_at TYPE TIMESTAMP USING (archived_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMP USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now(),
  ADD CONSTRAINT calendar_events_time_check CHECK (ends_at >= starts_at);

ALTER TABLE connector_configs
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMP USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN last_scheduled_at TYPE TIMESTAMP USING (last_scheduled_at AT TIME ZONE 'UTC'),
  ALTER COLUMN next_run_at TYPE TIMESTAMP USING (next_run_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE connector_run_item_errors
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE connector_run_items
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE connector_runs
  ALTER COLUMN next_attempt_at DROP DEFAULT,
  ALTER COLUMN started_at TYPE TIMESTAMP USING (started_at AT TIME ZONE 'UTC'),
  ALTER COLUMN finished_at TYPE TIMESTAMP USING (finished_at AT TIME ZONE 'UTC'),
  ALTER COLUMN claimed_at TYPE TIMESTAMP USING (claimed_at AT TIME ZONE 'UTC'),
  ALTER COLUMN next_attempt_at TYPE TIMESTAMP USING (next_attempt_at AT TIME ZONE 'UTC'),
  ALTER COLUMN lease_expires_at TYPE TIMESTAMP USING (lease_expires_at AT TIME ZONE 'UTC'),
  ALTER COLUMN heartbeat_at TYPE TIMESTAMP USING (heartbeat_at AT TIME ZONE 'UTC'),
  ALTER COLUMN cancel_requested_at TYPE TIMESTAMP USING (cancel_requested_at AT TIME ZONE 'UTC'),
  ALTER COLUMN cancelled_at TYPE TIMESTAMP USING (cancelled_at AT TIME ZONE 'UTC'),
  ALTER COLUMN next_attempt_at SET DEFAULT now();

ALTER TABLE connector_workers
  ALTER COLUMN started_at DROP DEFAULT,
  ALTER COLUMN last_seen_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN started_at TYPE TIMESTAMP USING (started_at AT TIME ZONE 'UTC'),
  ALTER COLUMN last_seen_at TYPE TIMESTAMP USING (last_seen_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMP USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN started_at SET DEFAULT now(),
  ALTER COLUMN last_seen_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE connectors
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN last_run_at TYPE TIMESTAMP USING (last_run_at AT TIME ZONE 'UTC'),
  ALTER COLUMN last_success_at TYPE TIMESTAMP USING (last_success_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMP USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE external_identities
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN last_login_at TYPE TIMESTAMP USING (last_login_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMP USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE login_throttle_buckets
  ALTER COLUMN window_started_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN window_started_at TYPE TIMESTAMP USING (window_started_at AT TIME ZONE 'UTC'),
  ALTER COLUMN locked_until TYPE TIMESTAMP USING (locked_until AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMP USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN window_started_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE maintainer_members
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE maintainers
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE maintenance_runs
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN started_at TYPE TIMESTAMP USING (started_at AT TIME ZONE 'UTC'),
  ALTER COLUMN finished_at TYPE TIMESTAMP USING (finished_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE notification_receipts
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN read_at TYPE TIMESTAMP USING (read_at AT TIME ZONE 'UTC'),
  ALTER COLUMN dismissed_at TYPE TIMESTAMP USING (dismissed_at AT TIME ZONE 'UTC'),
  ALTER COLUMN snoozed_until TYPE TIMESTAMP USING (snoozed_until AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMP USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE notifications
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMP USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN source_updated_at TYPE TIMESTAMP USING (source_updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN archived_at TYPE TIMESTAMP USING (archived_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE oidc_login_transactions
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN expires_at TYPE TIMESTAMP USING (expires_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE packages
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMP USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE roles
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE service_health_checks
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN checked_at TYPE TIMESTAMP USING (checked_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE services
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN last_checked_at TYPE TIMESTAMP USING (last_checked_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMP USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE sessions
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN last_seen_at DROP DEFAULT,
  ALTER COLUMN expires_at TYPE TIMESTAMP USING (expires_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN last_seen_at TYPE TIMESTAMP USING (last_seen_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN last_seen_at SET DEFAULT now();

ALTER TABLE users
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE work_cards
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN due_at TYPE TIMESTAMP USING (due_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMP USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMP USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN source_updated_at TYPE TIMESTAMP USING (source_updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN archived_at TYPE TIMESTAMP USING (archived_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();
