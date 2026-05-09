ALTER TABLE connector_runs DROP CONSTRAINT connector_runs_status_check;

ALTER TABLE connector_runs
  ALTER COLUMN finished_at DROP NOT NULL,
  ADD CONSTRAINT connector_runs_status_check
    CHECK (status IN ('queued', 'running', 'success', 'partial_success', 'failed'));

CREATE TABLE connector_configs (
  id SERIAL PRIMARY KEY,
  source varchar(64) NOT NULL UNIQUE REFERENCES connectors(source) ON DELETE CASCADE,
  target varchar(64) NOT NULL,
  enabled boolean NOT NULL DEFAULT true,
  schedule_cron varchar(128),
  config text NOT NULL DEFAULT '{}',
  sample_payload text NOT NULL DEFAULT '{"items":[]}',
  created_at TIMESTAMP DEFAULT NOW() NOT NULL,
  updated_at TIMESTAMP DEFAULT NOW() NOT NULL,
  CONSTRAINT connector_configs_target_check
    CHECK (target IN ('service_health', 'work_cards', 'notifications'))
);

CREATE INDEX connector_configs_target_idx ON connector_configs(target);
CREATE INDEX connector_configs_enabled_idx ON connector_configs(enabled);

CREATE TABLE connector_run_item_errors (
  id SERIAL PRIMARY KEY,
  connector_run_id integer NOT NULL REFERENCES connector_runs(id) ON DELETE CASCADE,
  source varchar(64) NOT NULL,
  target varchar(64) NOT NULL,
  external_id varchar(128),
  message text NOT NULL,
  raw_item text,
  created_at TIMESTAMP DEFAULT NOW() NOT NULL
);

CREATE INDEX connector_run_item_errors_run_id_idx
  ON connector_run_item_errors(connector_run_id);
