CREATE TABLE connector_run_items (
  id SERIAL PRIMARY KEY,
  connector_run_id integer NOT NULL REFERENCES connector_runs(id) ON DELETE CASCADE,
  source varchar(64) NOT NULL,
  target varchar(64) NOT NULL,
  record_id integer,
  external_id varchar(128),
  status varchar(32) NOT NULL,
  snapshot text,
  created_at TIMESTAMP DEFAULT NOW() NOT NULL,
  CONSTRAINT connector_run_items_status_check
    CHECK (status IN ('imported', 'failed'))
);

CREATE INDEX connector_run_items_run_id_idx
  ON connector_run_items(connector_run_id);

CREATE INDEX connector_run_items_source_target_idx
  ON connector_run_items(source, target);

CREATE INDEX connector_run_items_external_id_idx
  ON connector_run_items(external_id);
