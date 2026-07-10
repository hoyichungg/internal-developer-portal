DROP TABLE notification_receipts;

DROP INDEX notifications_archived_at_idx;
DROP INDEX notifications_maintainer_active_updated_idx;
DROP INDEX notifications_owner_active_updated_idx;
DROP INDEX notifications_connector_idx;

DROP INDEX work_cards_archived_at_idx;
DROP INDEX work_cards_maintainer_active_updated_idx;
DROP INDEX work_cards_owner_active_updated_idx;
DROP INDEX work_cards_connector_idx;

ALTER TABLE notifications
  DROP CONSTRAINT notifications_scope_check,
  DROP COLUMN archived_at,
  DROP COLUMN last_seen_run_id,
  DROP COLUMN source_updated_at,
  DROP COLUMN maintainer_id,
  DROP COLUMN owner_user_id,
  DROP COLUMN connector_id;

ALTER TABLE work_cards
  DROP CONSTRAINT work_cards_scope_check,
  DROP COLUMN archived_at,
  DROP COLUMN last_seen_run_id,
  DROP COLUMN source_updated_at,
  DROP COLUMN maintainer_id,
  DROP COLUMN owner_user_id,
  DROP COLUMN connector_id;

DROP INDEX connectors_maintainer_updated_at_idx;
DROP INDEX connectors_owner_user_updated_at_idx;

ALTER TABLE connectors
  DROP CONSTRAINT connectors_scope_check,
  DROP COLUMN maintainer_id,
  DROP COLUMN owner_user_id,
  DROP COLUMN scope_type;
