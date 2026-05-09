DROP INDEX notifications_source_external_id_unique;
DROP INDEX work_cards_source_external_id_unique;
DROP INDEX services_source_external_id_unique;

ALTER TABLE notifications DROP COLUMN external_id;

ALTER TABLE services
  DROP COLUMN external_id,
  DROP COLUMN source;
