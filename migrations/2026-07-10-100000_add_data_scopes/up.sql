ALTER TABLE connectors
  ADD COLUMN scope_type varchar(16) NOT NULL DEFAULT 'global',
  ADD COLUMN owner_user_id integer REFERENCES users(id) ON DELETE CASCADE,
  ADD COLUMN maintainer_id integer REFERENCES maintainers(id) ON DELETE CASCADE,
  ADD CONSTRAINT connectors_scope_check CHECK (
    (scope_type = 'global' AND owner_user_id IS NULL AND maintainer_id IS NULL)
    OR (scope_type = 'user' AND owner_user_id IS NOT NULL AND maintainer_id IS NULL)
    OR (scope_type = 'maintainer' AND owner_user_id IS NULL AND maintainer_id IS NOT NULL)
  );

CREATE INDEX connectors_owner_user_updated_at_idx
  ON connectors(owner_user_id, updated_at DESC)
  WHERE owner_user_id IS NOT NULL;
CREATE INDEX connectors_maintainer_updated_at_idx
  ON connectors(maintainer_id, updated_at DESC)
  WHERE maintainer_id IS NOT NULL;

ALTER TABLE work_cards
  ADD COLUMN connector_id integer REFERENCES connectors(id) ON DELETE SET NULL,
  ADD COLUMN owner_user_id integer REFERENCES users(id) ON DELETE CASCADE,
  ADD COLUMN maintainer_id integer REFERENCES maintainers(id) ON DELETE CASCADE,
  ADD COLUMN source_updated_at timestamp,
  ADD COLUMN last_seen_run_id integer REFERENCES connector_runs(id) ON DELETE SET NULL,
  ADD COLUMN archived_at timestamp,
  ADD CONSTRAINT work_cards_scope_check CHECK (
    NOT (owner_user_id IS NOT NULL AND maintainer_id IS NOT NULL)
  );

ALTER TABLE notifications
  ADD COLUMN connector_id integer REFERENCES connectors(id) ON DELETE SET NULL,
  ADD COLUMN owner_user_id integer REFERENCES users(id) ON DELETE CASCADE,
  ADD COLUMN maintainer_id integer REFERENCES maintainers(id) ON DELETE CASCADE,
  ADD COLUMN source_updated_at timestamp,
  ADD COLUMN last_seen_run_id integer REFERENCES connector_runs(id) ON DELETE SET NULL,
  ADD COLUMN archived_at timestamp,
  ADD CONSTRAINT notifications_scope_check CHECK (
    NOT (owner_user_id IS NOT NULL AND maintainer_id IS NOT NULL)
  );

UPDATE work_cards AS record
SET connector_id = connector.id
FROM connectors AS connector
WHERE record.source = connector.source;

UPDATE notifications AS record
SET connector_id = connector.id
FROM connectors AS connector
WHERE record.source = connector.source;

CREATE INDEX work_cards_connector_idx ON work_cards(connector_id);
CREATE INDEX work_cards_owner_active_updated_idx
  ON work_cards(owner_user_id, updated_at DESC)
  WHERE archived_at IS NULL;
CREATE INDEX work_cards_maintainer_active_updated_idx
  ON work_cards(maintainer_id, updated_at DESC)
  WHERE archived_at IS NULL;
CREATE INDEX work_cards_archived_at_idx ON work_cards(archived_at);

CREATE INDEX notifications_connector_idx ON notifications(connector_id);
CREATE INDEX notifications_owner_active_updated_idx
  ON notifications(owner_user_id, updated_at DESC)
  WHERE archived_at IS NULL;
CREATE INDEX notifications_maintainer_active_updated_idx
  ON notifications(maintainer_id, updated_at DESC)
  WHERE archived_at IS NULL;
CREATE INDEX notifications_archived_at_idx ON notifications(archived_at);

CREATE TABLE notification_receipts (
  id serial PRIMARY KEY,
  notification_id integer NOT NULL REFERENCES notifications(id) ON DELETE CASCADE,
  user_id integer NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  read_at timestamp,
  dismissed_at timestamp,
  snoozed_until timestamp,
  created_at timestamp NOT NULL DEFAULT now(),
  updated_at timestamp NOT NULL DEFAULT now(),
  UNIQUE (notification_id, user_id)
);

CREATE INDEX notification_receipts_user_updated_idx
  ON notification_receipts(user_id, updated_at DESC);
CREATE INDEX notification_receipts_notification_idx
  ON notification_receipts(notification_id);
