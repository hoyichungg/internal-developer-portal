-- Operational warning: revoke Entra-authenticated sessions before rolling this
-- migration back. Dropping identity bindings does not revoke existing sessions.
DROP TABLE oidc_login_transactions;
DROP TABLE external_identities;
