ALTER SEQUENCE packages_id_seq RENAME TO crates_id_seq;
ALTER TABLE packages ALTER COLUMN id SET DEFAULT nextval('crates_id_seq');
ALTER TABLE packages RENAME CONSTRAINT packages_maintainer_id_fkey TO crates_rustacean_id_fkey;
ALTER TABLE packages RENAME CONSTRAINT packages_pkey TO crates_pkey;
ALTER TABLE packages RENAME COLUMN slug TO code;
ALTER TABLE packages RENAME COLUMN maintainer_id TO rustacean_id;
ALTER TABLE packages RENAME TO crates;

ALTER SEQUENCE maintainers_id_seq RENAME TO rustaceans_id_seq;
ALTER TABLE maintainers ALTER COLUMN id SET DEFAULT nextval('rustaceans_id_seq');
ALTER TABLE maintainers RENAME CONSTRAINT maintainers_pkey TO rustaceans_pkey;
ALTER TABLE maintainers RENAME COLUMN display_name TO name;
ALTER TABLE maintainers RENAME TO rustaceans;
