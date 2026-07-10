CREATE TABLE calendar_events (
  id serial PRIMARY KEY,
  source varchar(64) NOT NULL,
  external_id varchar(128) NOT NULL,
  title varchar(256) NOT NULL,
  body text,
  organizer varchar(256),
  location varchar(256),
  starts_at timestamp NOT NULL,
  ends_at timestamp NOT NULL,
  time_zone varchar(128),
  is_all_day boolean NOT NULL DEFAULT false,
  is_cancelled boolean NOT NULL DEFAULT false,
  web_url varchar(2048),
  join_url varchar(2048),
  connector_id integer REFERENCES connectors(id) ON DELETE SET NULL,
  owner_user_id integer REFERENCES users(id) ON DELETE CASCADE,
  maintainer_id integer REFERENCES maintainers(id) ON DELETE CASCADE,
  source_updated_at timestamp,
  last_seen_run_id integer REFERENCES connector_runs(id) ON DELETE SET NULL,
  archived_at timestamp,
  created_at timestamp NOT NULL DEFAULT now(),
  updated_at timestamp NOT NULL DEFAULT now(),
  CONSTRAINT calendar_events_time_check CHECK (ends_at >= starts_at),
  CONSTRAINT calendar_events_scope_check CHECK (
    NOT (owner_user_id IS NOT NULL AND maintainer_id IS NOT NULL)
  ),
  UNIQUE (source, external_id)
);

CREATE INDEX calendar_events_connector_idx ON calendar_events(connector_id);
CREATE INDEX calendar_events_active_start_idx
  ON calendar_events(starts_at, id)
  WHERE archived_at IS NULL AND is_cancelled = false;
CREATE INDEX calendar_events_owner_active_start_idx
  ON calendar_events(owner_user_id, starts_at, id)
  WHERE archived_at IS NULL AND owner_user_id IS NOT NULL;
CREATE INDEX calendar_events_maintainer_active_start_idx
  ON calendar_events(maintainer_id, starts_at, id)
  WHERE archived_at IS NULL AND maintainer_id IS NOT NULL;
CREATE INDEX calendar_events_archived_at_idx ON calendar_events(archived_at);
