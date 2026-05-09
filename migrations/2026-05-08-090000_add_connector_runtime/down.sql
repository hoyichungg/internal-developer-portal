DROP TABLE connector_run_item_errors;
DROP TABLE connector_configs;

ALTER TABLE connector_runs DROP CONSTRAINT connector_runs_status_check;

ALTER TABLE connector_runs
  ALTER COLUMN finished_at SET NOT NULL,
  ADD CONSTRAINT connector_runs_status_check
    CHECK (status IN ('success', 'partial_success', 'failed'));
