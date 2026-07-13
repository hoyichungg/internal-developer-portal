CREATE TABLE external_identities (
  id SERIAL PRIMARY KEY,
  user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  provider VARCHAR(32) NOT NULL,
  issuer VARCHAR(255) NOT NULL,
  subject VARCHAR(255),
  tenant_id VARCHAR(36) NOT NULL,
  object_id VARCHAR(36) NOT NULL,
  preferred_username VARCHAR(320),
  display_name VARCHAR(256),
  email VARCHAR(320),
  last_login_at TIMESTAMP,
  created_at TIMESTAMP NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
  CONSTRAINT external_identities_provider_check CHECK (provider = 'entra'),
  CONSTRAINT external_identities_provider_tenant_object_unique
    UNIQUE (provider, tenant_id, object_id)
);

CREATE UNIQUE INDEX external_identities_provider_issuer_subject_idx
  ON external_identities(provider, issuer, subject)
  WHERE subject IS NOT NULL;

CREATE TABLE oidc_login_transactions (
  state_hash VARCHAR(64) PRIMARY KEY,
  browser_binding_hash VARCHAR(64) NOT NULL,
  nonce VARCHAR(128) NOT NULL,
  pkce_verifier_ciphertext TEXT NOT NULL,
  return_to VARCHAR(512) NOT NULL,
  expires_at TIMESTAMP NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX oidc_login_transactions_expires_at_idx
  ON oidc_login_transactions(expires_at);
