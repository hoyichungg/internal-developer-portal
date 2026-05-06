ALTER TABLE packages
  ADD COLUMN status varchar(32) NOT NULL DEFAULT 'active',
  ADD COLUMN repository_url varchar(2048),
  ADD COLUMN documentation_url varchar(2048),
  ADD COLUMN updated_at TIMESTAMP DEFAULT NOW() NOT NULL;

ALTER TABLE packages
  ADD CONSTRAINT packages_status_check
  CHECK (status IN ('active', 'deprecated', 'archived'));
