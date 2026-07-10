DROP INDEX connector_runs_one_active_retry_per_parent_idx;
DROP INDEX connector_runs_parent_run_id_idx;
DROP INDEX connector_runs_expired_lease_idx;
DROP INDEX connector_runs_claim_idx;

ALTER TABLE connector_runs
  DROP CONSTRAINT connector_runs_attempt_limit_check,
  DROP CONSTRAINT connector_runs_max_attempts_check,
  DROP CONSTRAINT connector_runs_attempt_count_check,
  DROP CONSTRAINT connector_runs_status_check;

UPDATE connector_runs
SET status = 'failed',
    error_message = COALESCE(error_message || '; ', '') ||
      'cancelled status converted to failed during lease migration rollback'
WHERE status = 'cancelled';

ALTER TABLE connector_runs
  DROP COLUMN parent_run_id,
  DROP COLUMN cancelled_at,
  DROP COLUMN cancel_requested_at,
  DROP COLUMN heartbeat_at,
  DROP COLUMN lease_expires_at,
  DROP COLUMN next_attempt_at,
  DROP COLUMN max_attempts,
  DROP COLUMN attempt_count,
  ADD CONSTRAINT connector_runs_status_check
    CHECK (status IN ('queued', 'running', 'success', 'partial_success', 'failed'));
