ALTER TABLE rustaceans RENAME COLUMN name TO display_name;
ALTER TABLE rustaceans RENAME TO maintainers;
ALTER TABLE maintainers RENAME CONSTRAINT rustaceans_pkey TO maintainers_pkey;
ALTER SEQUENCE rustaceans_id_seq RENAME TO maintainers_id_seq;
ALTER TABLE maintainers ALTER COLUMN id SET DEFAULT nextval('maintainers_id_seq');

ALTER TABLE crates RENAME COLUMN rustacean_id TO maintainer_id;
ALTER TABLE crates RENAME COLUMN code TO slug;
ALTER TABLE crates RENAME TO packages;
ALTER TABLE packages RENAME CONSTRAINT crates_pkey TO packages_pkey;
ALTER TABLE packages RENAME CONSTRAINT crates_rustacean_id_fkey TO packages_maintainer_id_fkey;
ALTER SEQUENCE crates_id_seq RENAME TO packages_id_seq;
ALTER TABLE packages ALTER COLUMN id SET DEFAULT nextval('packages_id_seq');
