CREATE TABLE maintainer_members (
  id SERIAL PRIMARY KEY,
  maintainer_id integer NOT NULL REFERENCES maintainers(id) ON DELETE CASCADE,
  user_id integer NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role varchar(32) NOT NULL,
  created_at TIMESTAMP DEFAULT NOW() NOT NULL,
  CONSTRAINT maintainer_members_role_check
    CHECK (role IN ('owner', 'maintainer', 'viewer')),
  CONSTRAINT maintainer_members_maintainer_user_unique
    UNIQUE (maintainer_id, user_id)
);
