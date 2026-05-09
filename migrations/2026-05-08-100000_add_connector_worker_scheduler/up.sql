ALTER TABLE connector_runs
  ADD COLUMN trigger varchar(32) NOT NULL DEFAULT 'manual',
  ADD COLUMN payload text,
  ADD COLUMN claimed_at TIMESTAMP,
  ADD COLUMN worker_id varchar(128),
  ADD CONSTRAINT connector_runs_trigger_check
    CHECK (trigger IN ('manual', 'scheduled', 'import'));

CREATE INDEX connector_runs_status_started_at_idx
  ON connector_runs(status, started_at);

CREATE INDEX connector_runs_worker_id_idx
  ON connector_runs(worker_id);

ALTER TABLE connector_configs
  ADD COLUMN last_scheduled_at TIMESTAMP,
  ADD COLUMN next_run_at TIMESTAMP,
  ADD COLUMN last_scheduled_run_id integer REFERENCES connector_runs(id) ON DELETE SET NULL;

UPDATE connector_configs
SET next_run_at = NOW()
WHERE schedule_cron IS NOT NULL;

CREATE INDEX connector_configs_next_run_at_idx
  ON connector_configs(next_run_at)
  WHERE enabled = true AND schedule_cron IS NOT NULL;
