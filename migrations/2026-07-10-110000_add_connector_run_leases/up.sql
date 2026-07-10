ALTER TABLE connector_runs
  DROP CONSTRAINT connector_runs_status_check;

ALTER TABLE connector_runs
  ADD COLUMN attempt_count integer NOT NULL DEFAULT 0,
  ADD COLUMN max_attempts integer NOT NULL DEFAULT 3,
  ADD COLUMN next_attempt_at timestamp NOT NULL DEFAULT NOW(),
  ADD COLUMN lease_expires_at timestamp,
  ADD COLUMN heartbeat_at timestamp,
  ADD COLUMN cancel_requested_at timestamp,
  ADD COLUMN cancelled_at timestamp,
  ADD COLUMN parent_run_id integer REFERENCES connector_runs(id) ON DELETE SET NULL,
  ADD CONSTRAINT connector_runs_status_check
    CHECK (status IN ('queued', 'running', 'success', 'partial_success', 'failed', 'cancelled')),
  ADD CONSTRAINT connector_runs_attempt_count_check
    CHECK (attempt_count >= 0),
  ADD CONSTRAINT connector_runs_max_attempts_check
    CHECK (max_attempts > 0),
  ADD CONSTRAINT connector_runs_attempt_limit_check
    CHECK (attempt_count <= max_attempts);

-- Runs claimed by a pre-lease worker cannot prove ownership after deployment.
-- Requeue them so a lease-aware worker can claim them safely and visibly.
UPDATE connector_runs
SET status = 'queued',
    next_attempt_at = NOW(),
    claimed_at = NULL,
    worker_id = NULL,
    error_message = COALESCE(error_message || '; ', '') ||
      'requeued during connector lease migration'
WHERE status = 'running';

CREATE INDEX connector_runs_claim_idx
  ON connector_runs(next_attempt_at, started_at, id)
  WHERE status = 'queued';

CREATE INDEX connector_runs_expired_lease_idx
  ON connector_runs(lease_expires_at, id)
  WHERE status = 'running' AND lease_expires_at IS NOT NULL;

CREATE INDEX connector_runs_parent_run_id_idx
  ON connector_runs(parent_run_id)
  WHERE parent_run_id IS NOT NULL;

CREATE UNIQUE INDEX connector_runs_one_active_retry_per_parent_idx
  ON connector_runs(parent_run_id)
  WHERE parent_run_id IS NOT NULL AND status IN ('queued', 'running');
