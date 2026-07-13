-- Existing rows contain bearer tokens in plaintext. Invalidate them during the
-- one-way security upgrade instead of pretending their values are hashes.
DELETE FROM sessions;

DROP INDEX sessions_token_idx;

ALTER TABLE sessions
  RENAME COLUMN token TO token_hash;

ALTER TABLE sessions
  ALTER COLUMN token_hash TYPE VARCHAR(64),
  ADD COLUMN auth_method VARCHAR(32) NOT NULL DEFAULT 'password',
  ADD COLUMN last_seen_at TIMESTAMP NOT NULL DEFAULT NOW(),
  ADD COLUMN ip_address VARCHAR(64),
  ADD COLUMN user_agent VARCHAR(512),
  ADD CONSTRAINT sessions_auth_method_check
    CHECK (auth_method IN ('password', 'entra'));

CREATE INDEX sessions_token_hash_idx ON sessions(token_hash);
CREATE INDEX sessions_user_expires_at_idx ON sessions(user_id, expires_at DESC);

CREATE TABLE login_throttle_buckets (
  bucket_hash VARCHAR(64) PRIMARY KEY,
  failure_count INT NOT NULL DEFAULT 0,
  window_started_at TIMESTAMP NOT NULL DEFAULT NOW(),
  locked_until TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
  CONSTRAINT login_throttle_failure_count_check CHECK (failure_count >= 0)
);

CREATE INDEX login_throttle_buckets_updated_at_idx
  ON login_throttle_buckets(updated_at);
