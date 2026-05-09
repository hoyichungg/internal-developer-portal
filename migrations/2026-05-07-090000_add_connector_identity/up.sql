ALTER TABLE services
  ADD COLUMN source varchar(64) NOT NULL DEFAULT 'manual',
  ADD COLUMN external_id varchar(128);

ALTER TABLE notifications
  ADD COLUMN external_id varchar(128);

CREATE UNIQUE INDEX services_source_external_id_unique
  ON services (source, external_id)
  WHERE external_id IS NOT NULL;

CREATE UNIQUE INDEX work_cards_source_external_id_unique
  ON work_cards (source, external_id)
  WHERE external_id IS NOT NULL;

CREATE UNIQUE INDEX notifications_source_external_id_unique
  ON notifications (source, external_id)
  WHERE external_id IS NOT NULL;
