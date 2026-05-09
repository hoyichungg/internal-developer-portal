CREATE TABLE connector_runs (
  id SERIAL PRIMARY KEY,
  source varchar(64) NOT NULL,
  target varchar(64) NOT NULL,
  status varchar(32) NOT NULL,
  success_count integer NOT NULL DEFAULT 0,
  failure_count integer NOT NULL DEFAULT 0,
  duration_ms bigint NOT NULL DEFAULT 0,
  error_message text,
  started_at TIMESTAMP NOT NULL,
  finished_at TIMESTAMP NOT NULL,
  CONSTRAINT connector_runs_status_check
    CHECK (status IN ('success', 'partial_success', 'failed'))
);

CREATE INDEX connector_runs_source_idx ON connector_runs(source);
CREATE INDEX connector_runs_target_idx ON connector_runs(target);
CREATE INDEX connector_runs_started_at_idx ON connector_runs(started_at DESC);
