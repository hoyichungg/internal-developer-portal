DROP INDEX connector_configs_next_run_at_idx;

ALTER TABLE connector_configs
  DROP COLUMN last_scheduled_run_id,
  DROP COLUMN next_run_at,
  DROP COLUMN last_scheduled_at;

DROP INDEX connector_runs_worker_id_idx;
DROP INDEX connector_runs_status_started_at_idx;

ALTER TABLE connector_runs
  DROP CONSTRAINT connector_runs_trigger_check,
  DROP COLUMN worker_id,
  DROP COLUMN claimed_at,
  DROP COLUMN payload,
  DROP COLUMN trigger;
