DROP TABLE login_throttle_buckets;

-- Token hashes cannot be converted back into usable bearer tokens. Reverting
-- therefore invalidates sessions just like the forward security migration.
DELETE FROM sessions;

DROP INDEX sessions_user_expires_at_idx;
DROP INDEX sessions_token_hash_idx;

ALTER TABLE sessions
  DROP CONSTRAINT sessions_auth_method_check,
  DROP COLUMN auth_method,
  DROP COLUMN last_seen_at,
  DROP COLUMN ip_address,
  DROP COLUMN user_agent;

ALTER TABLE sessions
  RENAME COLUMN token_hash TO token;

ALTER TABLE sessions
  ALTER COLUMN token TYPE VARCHAR(128);

CREATE INDEX sessions_token_idx ON sessions(token);
