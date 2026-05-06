ALTER TABLE packages DROP CONSTRAINT packages_status_check;

ALTER TABLE packages
  DROP COLUMN updated_at,
  DROP COLUMN documentation_url,
  DROP COLUMN repository_url,
  DROP COLUMN status;
