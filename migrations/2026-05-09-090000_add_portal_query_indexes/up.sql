CREATE INDEX services_maintainer_updated_at_idx
  ON services (maintainer_id, updated_at DESC);

CREATE INDEX services_source_updated_at_idx
  ON services (source, updated_at DESC);

CREATE INDEX packages_maintainer_updated_at_idx
  ON packages (maintainer_id, updated_at DESC);

CREATE INDEX work_cards_source_status_updated_at_idx
  ON work_cards (source, status, updated_at DESC);

CREATE INDEX notifications_source_is_read_updated_at_idx
  ON notifications (source, is_read, updated_at DESC);

CREATE INDEX connector_runs_source_target_started_at_idx
  ON connector_runs (source, target, started_at DESC);

CREATE INDEX maintainer_members_user_maintainer_idx
  ON maintainer_members (user_id, maintainer_id);
