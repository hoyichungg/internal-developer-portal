-- Every application timestamp before this migration was written as a naive
-- UTC wall-clock value. Convert it explicitly so the result never depends on
-- the PostgreSQL session time zone.
SET LOCAL TIME ZONE 'UTC';
SET LOCAL lock_timeout = '10s';

-- Older Graph Calendar configs could request a non-UTC response through
-- Prefer: outlook.timezone. That response contained a naive local wall time,
-- so blindly treating it as UTC would silently move meetings. Require an
-- operator to remove the preference and complete one UTC reconciliation sync
-- before taking the schema lock.
DO $$
BEGIN
  IF EXISTS (
    SELECT 1
    FROM calendar_events ce
    JOIN connectors c ON c.id = ce.connector_id
    JOIN connector_configs cc ON cc.source = c.source
    WHERE lower(coalesce(cc.config::jsonb ->> 'adapter', '')) IN (
      'microsoft_graph_calendar',
      'graph_calendar',
      'outlook_calendar'
    )
      AND lower(coalesce(nullif(btrim(cc.config::jsonb ->> 'time_zone'), ''), 'utc'))
          NOT IN ('utc', 'etc/utc', 'gmt', 'etc/gmt')
  ) THEN
    RAISE EXCEPTION USING
      ERRCODE = 'check_violation',
      MESSAGE = 'Non-UTC Microsoft Graph Calendar data must be reconciled before TIMESTAMPTZ migration',
      DETAIL = 'Remove config.time_zone from affected Graph Calendar connectors and run a complete successful sync so start/end values are UTC.',
      HINT = 'Verify the affected connector rows, retry the migration, then keep the new adapter on UTC responses.';
  END IF;
END
$$;

ALTER TABLE audit_logs
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE calendar_events
  DROP CONSTRAINT calendar_events_time_check,
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN starts_at TYPE TIMESTAMPTZ USING (starts_at AT TIME ZONE 'UTC'),
  ALTER COLUMN ends_at TYPE TIMESTAMPTZ USING (ends_at AT TIME ZONE 'UTC'),
  ALTER COLUMN source_updated_at TYPE TIMESTAMPTZ USING (source_updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN archived_at TYPE TIMESTAMPTZ USING (archived_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now(),
  ADD CONSTRAINT calendar_events_time_check CHECK (ends_at >= starts_at);

ALTER TABLE connector_configs
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN last_scheduled_at TYPE TIMESTAMPTZ USING (last_scheduled_at AT TIME ZONE 'UTC'),
  ALTER COLUMN next_run_at TYPE TIMESTAMPTZ USING (next_run_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE connector_run_item_errors
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE connector_run_items
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE connector_runs
  ALTER COLUMN next_attempt_at DROP DEFAULT,
  ALTER COLUMN started_at TYPE TIMESTAMPTZ USING (started_at AT TIME ZONE 'UTC'),
  ALTER COLUMN finished_at TYPE TIMESTAMPTZ USING (finished_at AT TIME ZONE 'UTC'),
  ALTER COLUMN claimed_at TYPE TIMESTAMPTZ USING (claimed_at AT TIME ZONE 'UTC'),
  ALTER COLUMN next_attempt_at TYPE TIMESTAMPTZ USING (next_attempt_at AT TIME ZONE 'UTC'),
  ALTER COLUMN lease_expires_at TYPE TIMESTAMPTZ USING (lease_expires_at AT TIME ZONE 'UTC'),
  ALTER COLUMN heartbeat_at TYPE TIMESTAMPTZ USING (heartbeat_at AT TIME ZONE 'UTC'),
  ALTER COLUMN cancel_requested_at TYPE TIMESTAMPTZ USING (cancel_requested_at AT TIME ZONE 'UTC'),
  ALTER COLUMN cancelled_at TYPE TIMESTAMPTZ USING (cancelled_at AT TIME ZONE 'UTC'),
  ALTER COLUMN next_attempt_at SET DEFAULT now();

ALTER TABLE connector_workers
  ALTER COLUMN started_at DROP DEFAULT,
  ALTER COLUMN last_seen_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN started_at TYPE TIMESTAMPTZ USING (started_at AT TIME ZONE 'UTC'),
  ALTER COLUMN last_seen_at TYPE TIMESTAMPTZ USING (last_seen_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN started_at SET DEFAULT now(),
  ALTER COLUMN last_seen_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE connectors
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN last_run_at TYPE TIMESTAMPTZ USING (last_run_at AT TIME ZONE 'UTC'),
  ALTER COLUMN last_success_at TYPE TIMESTAMPTZ USING (last_success_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE external_identities
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN last_login_at TYPE TIMESTAMPTZ USING (last_login_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE login_throttle_buckets
  ALTER COLUMN window_started_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN window_started_at TYPE TIMESTAMPTZ USING (window_started_at AT TIME ZONE 'UTC'),
  ALTER COLUMN locked_until TYPE TIMESTAMPTZ USING (locked_until AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN window_started_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE maintainer_members
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE maintainers
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE maintenance_runs
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN started_at TYPE TIMESTAMPTZ USING (started_at AT TIME ZONE 'UTC'),
  ALTER COLUMN finished_at TYPE TIMESTAMPTZ USING (finished_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE notification_receipts
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN read_at TYPE TIMESTAMPTZ USING (read_at AT TIME ZONE 'UTC'),
  ALTER COLUMN dismissed_at TYPE TIMESTAMPTZ USING (dismissed_at AT TIME ZONE 'UTC'),
  ALTER COLUMN snoozed_until TYPE TIMESTAMPTZ USING (snoozed_until AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE notifications
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN source_updated_at TYPE TIMESTAMPTZ USING (source_updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN archived_at TYPE TIMESTAMPTZ USING (archived_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE oidc_login_transactions
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN expires_at TYPE TIMESTAMPTZ USING (expires_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE packages
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE roles
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE service_health_checks
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN checked_at TYPE TIMESTAMPTZ USING (checked_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE services
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN last_checked_at TYPE TIMESTAMPTZ USING (last_checked_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();

ALTER TABLE sessions
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN last_seen_at DROP DEFAULT,
  ALTER COLUMN expires_at TYPE TIMESTAMPTZ USING (expires_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN last_seen_at TYPE TIMESTAMPTZ USING (last_seen_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN last_seen_at SET DEFAULT now();

ALTER TABLE users
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now();

ALTER TABLE work_cards
  ALTER COLUMN created_at DROP DEFAULT,
  ALTER COLUMN updated_at DROP DEFAULT,
  ALTER COLUMN due_at TYPE TIMESTAMPTZ USING (due_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at TYPE TIMESTAMPTZ USING (created_at AT TIME ZONE 'UTC'),
  ALTER COLUMN updated_at TYPE TIMESTAMPTZ USING (updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN source_updated_at TYPE TIMESTAMPTZ USING (source_updated_at AT TIME ZONE 'UTC'),
  ALTER COLUMN archived_at TYPE TIMESTAMPTZ USING (archived_at AT TIME ZONE 'UTC'),
  ALTER COLUMN created_at SET DEFAULT now(),
  ALTER COLUMN updated_at SET DEFAULT now();
