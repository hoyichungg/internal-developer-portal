CREATE TABLE connectors (
  id SERIAL PRIMARY KEY,
  source varchar(64) NOT NULL UNIQUE,
  kind varchar(64) NOT NULL,
  display_name varchar(128) NOT NULL,
  status varchar(32) NOT NULL,
  last_run_at TIMESTAMP,
  last_success_at TIMESTAMP,
  created_at TIMESTAMP DEFAULT NOW() NOT NULL,
  updated_at TIMESTAMP DEFAULT NOW() NOT NULL,
  CONSTRAINT connectors_status_check
    CHECK (status IN ('active', 'paused', 'error'))
);

CREATE INDEX connectors_kind_idx ON connectors(kind);
CREATE INDEX connectors_status_idx ON connectors(status);
